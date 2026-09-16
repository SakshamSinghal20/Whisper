"""Async client. Receiver secrets stay local; server metadata is not a payment proof."""

import asyncio
import json
from dataclasses import dataclass, field

import httpx
from coincurve import PublicKey
from pydantic import ValidationError

from whisper.core import ScanKey
from whisper.models import TransactionsResponse


class ClientError(RuntimeError):
    pass


@dataclass(frozen=True)
class ScanResult:
    txid: str
    vout: int
    amount: int
    block_height: int
    block_hash: str
    label: int | None
    output_pubkey: bytes
    tweak: bytes = field(repr=False)


class SilentPaymentClient:
    def __init__(
        self,
        base_url: str,
        scan_key: ScanKey,
        spend_pubkey: bytes,
        labels: tuple[int, ...] = (),
        *,
        http_client: httpx.AsyncClient | None = None,
    ):
        parsed = httpx.URL(base_url)
        if parsed.scheme not in {"http", "https"} or not parsed.host:
            raise ValueError("base_url must be an HTTP(S) URL")
        if len(spend_pubkey) != 33:
            raise ValueError("Spend key must be a full compressed public key (33 bytes)")
        PublicKey(spend_pubkey)
        for label in labels:
            scan_key.label_tweak(label)
        self.base_url = base_url.rstrip("/")
        self.scan_key, self.spend_pubkey, self.labels = scan_key, spend_pubkey, labels
        self._owns_client = http_client is None
        self.http = http_client or httpx.AsyncClient(timeout=30, follow_redirects=False)

    async def __aenter__(self):
        return self

    async def __aexit__(self, *_):
        await self.close()

    async def close(self):
        if self._owns_client:
            await self.http.aclose()

    async def _get(self, path: str, params: dict | None = None, *, allow_degraded=False):
        try:
            async with self.http.stream("GET", self.base_url + path, params=params) as response:
                chunks, size = [], 0
                async for chunk in response.aiter_bytes():
                    size += len(chunk)
                    if size > 32 * 1024 * 1024:
                        raise ClientError("Server response exceeds 32 MiB")
                    chunks.append(chunk)
                payload = json.loads(b"".join(chunks))
                if not response.is_success and not (
                    allow_degraded
                    and response.status_code == 503
                    and isinstance(payload, dict)
                    and "limits" in payload
                ):
                    detail = (
                        payload.get("error", "Request failed")
                        if isinstance(payload, dict)
                        else "Request failed"
                    )
                    raise ClientError(f"Server returned {response.status_code}: {detail}")
                return payload
        except (httpx.HTTPError, ValueError) as exc:
            raise ClientError("Request failed or server returned invalid JSON") from exc

    async def get_status(self) -> dict:
        return await self._get("/api/v1/status", allow_degraded=True)

    async def scan_range(self, start_height: int, end_height: int) -> list[ScanResult]:
        if (
            type(start_height) is not int
            or type(end_height) is not int
            or not 0 <= start_height <= end_height <= 2_147_483_647
        ):
            raise ValueError("Use an ordered, non-negative block range")
        status = await self.get_status()
        batch = status.get("limits", {}).get("max_block_range", 1000)
        if type(batch) is not int or not 1 <= batch <= 10000:
            raise ClientError("Server returned an invalid block limit")
        results = []
        previous_batch_anchor = None
        for start in range(start_height, end_height + 1, batch):
            end = min(end_height, start + batch - 1)
            cursor, anchor, previous = None, None, None
            seen_cursors = set()
            while True:
                params = {"start_height": start, "end_height": end}
                if cursor:
                    params["cursor"] = cursor
                try:
                    page = TransactionsResponse.model_validate(
                        await self._get("/api/v1/transactions", params)
                    )
                except ValidationError as exc:
                    raise ClientError("Server returned invalid transaction data") from exc
                if page.scanned_blocks != list(range(start, end + 1)):
                    raise ClientError("Server did not cover the requested block range")
                if anchor is not None and page.chain_anchor != anchor:
                    raise ClientError("Chain changed during the scan; retry")
                if (
                    previous_batch_anchor is not None
                    and page.previous_hash != previous_batch_anchor
                ):
                    raise ClientError("Chain changed between scan ranges; retry")
                anchor = page.chain_anchor
                for tx in page.transactions:
                    position = (tx.block_height, tx.txid)
                    if (
                        not start <= tx.block_height <= end
                        or (previous and position <= previous)
                        or not tx.outputs
                    ):
                        raise ClientError(
                            "Server returned inconsistent transaction order or coverage"
                        )
                    previous = position
                    vouts, hashes = set(), set()
                    for output in tx.outputs:
                        if (
                            output.txid != tx.txid
                            or output.block_height != tx.block_height
                            or output.vout in vouts
                        ):
                            raise ClientError("Server returned inconsistent output metadata")
                        vouts.add(output.vout)
                        hashes.add(output.block_hash)
                    if len(hashes) != 1:
                        raise ClientError("Server mixed block hashes within a transaction")
                    try:
                        matches = await asyncio.to_thread(
                            self.scan_key.scan_outputs,
                            bytes.fromhex(tx.input_tweak),
                            self.spend_pubkey,
                            [bytes.fromhex(o.script_pubkey[4:]) for o in tx.outputs],
                            self.labels,
                        )
                    except ValueError as exc:
                        raise ClientError("Server returned an invalid transaction tweak") from exc
                    for match in matches:
                        output = tx.outputs[match.output_index]
                        results.append(
                            ScanResult(
                                tx.txid,
                                output.vout,
                                output.amount,
                                tx.block_height,
                                output.block_hash,
                                match.label,
                                match.output_pubkey,
                                match.tweak,
                            )
                        )
                cursor = page.next_cursor
                if cursor is None:
                    break
                if not page.transactions or cursor in seen_cursors:
                    raise ClientError("Server pagination did not advance")
                seen_cursors.add(cursor)
            previous_batch_anchor = anchor
        return results
