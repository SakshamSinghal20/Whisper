import asyncio
import base64
import hashlib
import json
import logging
import time
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Annotated

from fastapi import FastAPI, HTTPException, Query, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import FileResponse, JSONResponse
from fastapi.staticfiles import StaticFiles
from sqlalchemy import and_, or_, select
from sqlalchemy.exc import SQLAlchemyError
from starlette.middleware.cors import CORSMiddleware

from whisper import __version__
from whisper.config import Settings
from whisper.database import Database, blocks, outputs, transactions
from whisper.indexer import Indexer
from whisper.models import ScanRequest
from whisper.rpc import BitcoinRPC

STATIC = Path(__file__).parent / "static"
logger = logging.getLogger(__name__)


class BodyLimit:
    """Bound request bodies before JSON parsing, including chunked requests."""

    def __init__(self, app, max_bytes=65536):
        self.app, self.max_bytes = app, max_bytes

    async def __call__(self, scope, receive, send):
        if scope["type"] != "http":
            return await self.app(scope, receive, send)
        body = bytearray()
        while True:
            message = await receive()
            if message["type"] == "http.disconnect":
                return
            body.extend(message.get("body", b""))
            if len(body) > self.max_bytes:
                return await JSONResponse({"error": "Request body exceeds 64 KiB"}, 413)(
                    scope, receive, send
                )
            if not message.get("more_body", False):
                break
        delivered = False

        async def replay():
            nonlocal delivered
            if delivered:
                return await receive()
            delivered = True
            return {"type": "http.request", "body": bytes(body), "more_body": False}

        await self.app(scope, replay, send)


def scope_hash(start: int, end: int, prefixes: list[str] | None) -> str:
    return hashlib.sha256(json.dumps([start, end, prefixes]).encode()).hexdigest()[:24]


def encode_cursor(anchor: str, scope: str, key: list) -> str:
    return base64.urlsafe_b64encode(json.dumps([anchor, scope, key]).encode()).decode()


def decode_cursor(cursor: str | None, anchor: str, scope: str, size: int):
    if cursor is None:
        return None
    try:
        saved_anchor, saved_scope, key = json.loads(
            base64.b64decode(cursor, altchars=b"-_", validate=True)
        )
        if (
            saved_scope != scope
            or not isinstance(key, list)
            or len(key) != size
            or type(key[0]) is not int
            or not isinstance(key[1], str)
            or len(key[1]) != 64
            or any(c not in "0123456789abcdef" for c in key[1])
            or (size == 3 and type(key[2]) is not int)
        ):
            raise ValueError
    except (ValueError, TypeError, UnicodeDecodeError):
        raise HTTPException(400, "Invalid cursor for this query") from None
    if saved_anchor != anchor:
        raise HTTPException(409, "Chain changed during pagination; restart the scan")
    return key


async def coverage(connection, start: int, end: int, settings: Settings):
    if end < start or end - start + 1 > settings.max_block_range:
        raise HTTPException(400, f"Request 1 to {settings.max_block_range} blocks, inclusively")
    heights = (
        (
            await connection.execute(
                select(blocks.c.height)
                .where(blocks.c.height.between(start, end))
                .order_by(blocks.c.height)
            )
        )
        .scalars()
        .all()
    )
    if len(heights) != end - start + 1:
        raise HTTPException(409, "Range is not fully indexed; check index coverage before scanning")
    anchor = await connection.scalar(select(blocks.c.hash).where(blocks.c.height == end))
    return heights, anchor


def candidate_query():
    return select(
        outputs.c.txid,
        outputs.c.vout,
        outputs.c.amount,
        outputs.c.script_pubkey,
        outputs.c.block_height,
        blocks.c.hash.label("block_hash"),
        blocks.c.timestamp,
    ).join(blocks, blocks.c.height == outputs.c.block_height)


def create_app(settings: Settings | None = None) -> FastAPI:
    settings = settings or Settings()
    db = Database(settings.database_url)
    rpc = BitcoinRPC(settings)
    indexer = Indexer(db, rpc, settings)

    @asynccontextmanager
    async def lifespan(app):
        await db.initialize(settings.network, settings.start_height)
        app.state.started_at = time.monotonic()
        task = asyncio.create_task(indexer.run()) if settings.indexer_enabled else None
        if not task:
            indexer.state.status = "disabled"
        try:
            yield
        finally:
            if task:
                task.cancel()
                await asyncio.gather(task, return_exceptions=True)
            await rpc.close()
            await db.close()

    app = FastAPI(
        title="Whisper", version=__version__, lifespan=lifespan, docs_url=None, redoc_url=None
    )
    app.state.db, app.state.indexer, app.state.settings = db, indexer, settings
    app.add_middleware(BodyLimit)
    if settings.cors_origin:
        app.add_middleware(
            CORSMiddleware,
            allow_origins=settings.cors_origin.split(","),
            allow_methods=["GET", "POST"],
            allow_headers=["Content-Type"],
        )

    @app.middleware("http")
    async def headers(request: Request, call_next):
        response = await call_next(request)
        response.headers.update(
            {
                "X-Content-Type-Options": "nosniff",
                "X-Frame-Options": "DENY",
                "Referrer-Policy": "no-referrer",
                "Content-Security-Policy": (
                    "default-src 'self'; script-src 'self'; style-src 'self'; "
                    "img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; "
                    "base-uri 'none'; form-action 'self'"
                ),
            }
        )
        if request.url.path.startswith("/api/"):
            response.headers["Cache-Control"] = "no-store"
        return response

    @app.exception_handler(HTTPException)
    async def http_error(request, exc):
        return JSONResponse({"error": exc.detail, "status": exc.status_code}, exc.status_code)

    @app.exception_handler(RequestValidationError)
    async def validation_error(request, exc):
        issues = [
            {"field": ".".join(map(str, e["loc"])), "message": e["msg"]} for e in exc.errors()
        ]
        return JSONResponse(
            {"error": "Check request fields", "details": issues, "status": 422}, 422
        )

    @app.exception_handler(SQLAlchemyError)
    async def database_error(request, exc):
        logger.error("Database operation failed: %s", type(exc).__name__)
        return JSONResponse(
            {"error": "Database unavailable; try again shortly", "status": 503}, 503
        )

    @app.get("/api/v1/status")
    async def status():
        async with db.snapshot() as connection:
            counts = await db.counts(connection)
        state = indexer.state
        stale = state.last_success is not None and time.time() - state.last_success > max(
            120, settings.poll_interval * 3
        )
        healthy = state.status not in {"starting", "error", "stopped"} and not stale
        return JSONResponse(
            {
                "status": "ok" if healthy else "degraded",
                "version": __version__,
                "network": settings.network,
                **counts,
                "uptime_seconds": int(time.monotonic() - app.state.started_at),
                "database": "connected",
                "indexer": state.status,
                "indexer_error": state.error,
                "node_height": state.node_height,
                "last_indexer_check": state.last_success,
                "reorgs": state.reorgs,
                "limits": {
                    "max_block_range": settings.max_block_range,
                    "max_prefixes": settings.max_prefixes,
                    "page_size": settings.page_size,
                },
            },
            200 if healthy else 503,
        )

    @app.get("/api/v1/blocks")
    async def recent_blocks(limit: Annotated[int, Query(ge=1, le=100)] = 10):
        async with db.snapshot() as connection:
            rows = (
                (
                    await connection.execute(
                        select(blocks).order_by(blocks.c.height.desc()).limit(limit)
                    )
                )
                .mappings()
                .all()
            )
        return {"blocks": [dict(row) for row in rows]}

    @app.post("/api/v1/scan")
    async def scan(req: ScanRequest):
        started = time.monotonic()
        if len(req.prefixes) > settings.max_prefixes:
            raise HTTPException(400, f"Use at most {settings.max_prefixes} prefixes")
        async with db.snapshot() as connection:
            heights, anchor = await coverage(connection, req.start_height, req.end_height, settings)
            scope = scope_hash(req.start_height, req.end_height, req.prefixes)
            key = decode_cursor(req.cursor, anchor, scope, 3)
            query = candidate_query().where(
                outputs.c.block_height.between(req.start_height, req.end_height),
                outputs.c.prefix.in_(req.prefixes),
            )
            if key:
                query = query.where(
                    or_(
                        outputs.c.block_height > key[0],
                        and_(outputs.c.block_height == key[0], outputs.c.txid > key[1]),
                        and_(
                            outputs.c.block_height == key[0],
                            outputs.c.txid == key[1],
                            outputs.c.vout > key[2],
                        ),
                    )
                )
            rows = (
                (
                    await connection.execute(
                        query.order_by(
                            outputs.c.block_height, outputs.c.txid, outputs.c.vout
                        ).limit(settings.page_size + 1)
                    )
                )
                .mappings()
                .all()
            )
            more = len(rows) > settings.page_size
            rows = rows[: settings.page_size]
            last = rows[-1] if rows else None
            cursor = (
                encode_cursor(anchor, scope, [last["block_height"], last["txid"], last["vout"]])
                if more
                else None
            )
        return {
            "candidates": [dict(r) for r in rows],
            "scanned_blocks": heights,
            "chain_anchor": anchor,
            "next_cursor": cursor,
            "server_time_ms": round((time.monotonic() - started) * 1000),
        }

    @app.get("/api/v1/transactions")
    async def scan_transactions(
        start_height: Annotated[int, Query(ge=0, le=2_147_483_647)],
        end_height: Annotated[int, Query(ge=0, le=2_147_483_647)],
        cursor: Annotated[str | None, Query(max_length=1024)] = None,
    ):
        async with db.snapshot() as connection:
            heights, anchor = await coverage(connection, start_height, end_height, settings)
            previous_hash = await connection.scalar(
                select(blocks.c.previous_hash).where(blocks.c.height == start_height)
            )
            scope = scope_hash(start_height, end_height, None)
            key = decode_cursor(cursor, anchor, scope, 2)
            query = select(transactions).where(
                transactions.c.block_height.between(start_height, end_height),
                transactions.c.input_tweak.is_not(None),
            )
            if key:
                query = query.where(
                    or_(
                        transactions.c.block_height > key[0],
                        and_(transactions.c.block_height == key[0], transactions.c.txid > key[1]),
                    )
                )
            rows = (
                (
                    await connection.execute(
                        query.order_by(transactions.c.block_height, transactions.c.txid).limit(
                            min(settings.page_size, 25) + 1
                        )
                    )
                )
                .mappings()
                .all()
            )
            page, output_total = [], 0
            for row in rows[: min(settings.page_size, 25)]:
                tx_outputs = (
                    (
                        await connection.execute(
                            candidate_query()
                            .where(outputs.c.txid == row["txid"])
                            .order_by(outputs.c.vout)
                        )
                    )
                    .mappings()
                    .all()
                )
                if page and output_total + len(tx_outputs) > 10000:
                    break
                output_total += len(tx_outputs)
                page.append(
                    {
                        "txid": row["txid"],
                        "block_height": row["block_height"],
                        "input_tweak": row["input_tweak"].hex(),
                        "outputs": [dict(o) for o in tx_outputs],
                    }
                )
            next_cursor = None
            if len(rows) > len(page):
                last = page[-1]
                next_cursor = encode_cursor(anchor, scope, [last["block_height"], last["txid"]])
        return {
            "transactions": page,
            "scanned_blocks": heights,
            "chain_anchor": anchor,
            "previous_hash": previous_hash,
            "next_cursor": next_cursor,
        }

    @app.get("/", include_in_schema=False)
    async def dashboard():
        return FileResponse(STATIC / "index.html")

    app.mount("/assets", StaticFiles(directory=STATIC, check_dir=False), name="assets")
    return app


app = create_app()
