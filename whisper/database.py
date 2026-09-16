"""Atomic canonical-chain storage, shared by the API and a single indexer."""

import asyncio
from contextlib import asynccontextmanager

from sqlalchemy import (
    BigInteger,
    CheckConstraint,
    Column,
    ForeignKey,
    Index,
    Integer,
    LargeBinary,
    MetaData,
    String,
    Table,
    delete,
    event,
    func,
    insert,
    inspect,
    select,
)
from sqlalchemy.ext.asyncio import create_async_engine

metadata = MetaData()
blocks = Table(
    "blocks",
    metadata,
    Column("height", Integer, primary_key=True, autoincrement=False),
    Column("hash", String(64), nullable=False, unique=True),
    Column("previous_hash", String(64), nullable=False),
    Column("timestamp", BigInteger, nullable=False),
    Column("tx_count", Integer, nullable=False),
    Column("output_count", Integer, nullable=False),
    CheckConstraint("height >= 0"),
)
transactions = Table(
    "transactions",
    metadata,
    Column("txid", String(64), primary_key=True),
    Column(
        "block_height", Integer, ForeignKey("blocks.height", ondelete="CASCADE"), nullable=False
    ),
    Column("input_tweak", LargeBinary(33), nullable=True),
    Index("ix_transactions_height_txid", "block_height", "txid"),
)
outputs = Table(
    "taproot_outputs",
    metadata,
    Column(
        "txid", String(64), ForeignKey("transactions.txid", ondelete="CASCADE"), primary_key=True
    ),
    Column("vout", Integer, primary_key=True, autoincrement=False),
    Column(
        "block_height", Integer, ForeignKey("blocks.height", ondelete="CASCADE"), nullable=False
    ),
    Column("script_pubkey", String(68), nullable=False),
    Column("amount", BigInteger, nullable=False),
    Column("prefix", String(8), nullable=False),
    CheckConstraint("amount >= 0 AND amount <= 2100000000000000"),
    CheckConstraint("vout >= 0"),
    Index("ix_outputs_prefix_height", "prefix", "block_height"),
    Index("ix_outputs_height", "block_height"),
)
index_meta = Table(
    "whisper_meta",
    metadata,
    Column("key", String(40), primary_key=True),
    Column("value", String(100), nullable=False),
)


class Database:
    def __init__(self, url: str):
        self.engine = create_async_engine(url, pool_pre_ping=True)
        # One server process owns indexing. This lock also makes multi-query API
        # snapshots consistent while a block commits or a fork rolls back.
        self.lock = asyncio.Lock()
        if self.engine.dialect.name == "sqlite":

            @event.listens_for(self.engine.sync_engine, "connect")
            def configure_sqlite(connection, _):
                cursor = connection.cursor()
                cursor.execute("PRAGMA foreign_keys=ON")
                cursor.execute("PRAGMA busy_timeout=5000")
                cursor.execute("PRAGMA journal_mode=WAL")
                cursor.close()

    async def initialize(self, network: str, start_height: int) -> None:
        async with self.engine.begin() as connection:

            def check_schema(sync_connection):
                inspector = inspect(sync_connection)
                if inspector.has_table("blocks"):
                    columns = {c["name"] for c in inspector.get_columns("blocks")}
                    if "previous_hash" not in columns:
                        raise RuntimeError(
                            "Legacy Rust index detected. Use a new database and reindex; "
                            "the existing database has not been changed."
                        )

            await connection.run_sync(check_schema)
            await connection.run_sync(metadata.create_all)
            for key, value in {
                "schema_version": "1",
                "network": network,
                "start_height": str(start_height),
            }.items():
                existing = await connection.scalar(
                    select(index_meta.c.value).where(index_meta.c.key == key)
                )
                if existing is not None and existing != value:
                    raise RuntimeError(
                        f"Database {key} differs from configuration; use a new database"
                    )
                if existing is None:
                    await connection.execute(insert(index_meta).values(key=key, value=value))

    @asynccontextmanager
    async def snapshot(self):
        async with self.lock, self.engine.begin() as connection:
            yield connection

    async def tip(self):
        async with self.snapshot() as connection:
            return (
                (await connection.execute(select(blocks).order_by(blocks.c.height.desc()).limit(1)))
                .mappings()
                .first()
            )

    async def rollback_from(self, height: int) -> None:
        async with self.snapshot() as connection:
            await connection.execute(delete(blocks).where(blocks.c.height >= height))

    async def save_block(self, block: dict, txs: list[dict], tx_outputs: list[dict]) -> None:
        async with self.snapshot() as connection:
            old_hash = await connection.scalar(
                select(blocks.c.hash).where(blocks.c.height == block["height"])
            )
            if old_hash == block["hash"]:
                return
            if old_hash is not None:
                raise ValueError("Conflicting block must be rolled back before insertion")
            tip = (
                (await connection.execute(select(blocks).order_by(blocks.c.height.desc()).limit(1)))
                .mappings()
                .first()
            )
            if tip and (
                block["height"] != tip["height"] + 1 or block["previous_hash"] != tip["hash"]
            ):
                raise ValueError("Block does not extend the indexed chain")
            await connection.execute(insert(blocks).values(**block))
            if txs:
                await connection.execute(insert(transactions), txs)
            # Stay below PostgreSQL's parameter limit for large blocks.
            for offset in range(0, len(tx_outputs), 500):
                await connection.execute(insert(outputs), tx_outputs[offset : offset + 500])

    async def counts(self, connection) -> dict:
        row = (
            (
                await connection.execute(
                    select(
                        func.min(blocks.c.height).label("start_height"),
                        func.max(blocks.c.height).label("tip_height"),
                        func.count().label("total_blocks"),
                        func.coalesce(func.sum(blocks.c.output_count), 0).label("total_outputs"),
                    )
                )
            )
            .mappings()
            .one()
        )
        return dict(row)

    async def close(self) -> None:
        await self.engine.dispose()
