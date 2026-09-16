import copy

import pytest
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError

from whisper.database import outputs
from whisper.indexer import Indexer, prepare_block

from .conftest import vector_block


class FakeRPC:
    def __init__(self, chain):
        self.chain = chain
        self.network = "regtest"
        self.pruneheight = 0
        self.fail_at = None

    async def call(self, method, *params):
        if method == "getblockchaininfo":
            return {
                "chain": self.network,
                "blocks": len(self.chain) - 1,
                "pruned": self.pruneheight > 0,
                "pruneheight": self.pruneheight,
            }
        if method == "getblockhash":
            return self.chain[params[0]]["hash"]
        if method == "getblock":
            block = next(b for b in self.chain if b["hash"] == params[0])
            if block["height"] == self.fail_at:
                raise ConnectionError("node unavailable")
            return copy.deepcopy(block)
        raise AssertionError(method)


async def test_catchup_restart_and_reorg(api):
    app, client = api
    rpc = FakeRPC([vector_block(h) for h in range(4)])
    indexer = Indexer(app.state.db, rpc, app.state.settings)
    await indexer.sync_once()
    assert (await app.state.db.tip())["height"] == 3
    await indexer.sync_once()
    assert (await client.get("/api/v1/status")).json()["total_blocks"] == 4
    rpc.chain[2] = vector_block(2, "aa" * 32)
    rpc.chain[3] = vector_block(3, "bb" * 32, "aa" * 32)
    rpc.chain.append(vector_block(4, "cc" * 32, "bb" * 32))
    await indexer.sync_once()
    assert (await app.state.db.tip())["hash"] == "cc" * 32
    assert indexer.state.reorgs == 1
    async with app.state.db.snapshot() as connection:
        assert len((await connection.execute(select(outputs))).all()) == 5


async def test_node_failure_resumes_without_missing_blocks(api):
    app, _ = api
    rpc = FakeRPC([vector_block(h) for h in range(4)])
    rpc.fail_at = 2
    indexer = Indexer(app.state.db, rpc, app.state.settings)
    with pytest.raises(ConnectionError):
        await indexer.sync_once()
    assert (await app.state.db.tip())["height"] == 1
    rpc.fail_at = None
    await indexer.sync_once()
    assert (await app.state.db.tip())["height"] == 3


async def test_network_mismatch_never_indexes(api):
    app, _ = api
    rpc = FakeRPC([vector_block()])
    rpc.network = "main"
    with pytest.raises(ValueError, match="network"):
        await Indexer(app.state.db, rpc, app.state.settings).sync_once()
    assert await app.state.db.tip() is None


async def test_missing_prevouts_fail_whole_block(api):
    app, _ = api
    block = vector_block()
    del block["tx"][0]["vin"][0]["prevout"]
    rpc = FakeRPC([block])
    with pytest.raises(ValueError, match="prevouts"):
        await Indexer(app.state.db, rpc, app.state.settings).sync_once()
    assert await app.state.db.tip() is None


async def test_pruned_history_fails_explicitly(api):
    app, _ = api
    rpc = FakeRPC([vector_block()])
    rpc.pruneheight = 100
    with pytest.raises(ValueError, match="pruned"):
        await Indexer(app.state.db, rpc, app.state.settings).sync_once()


async def test_block_insert_is_atomic_on_invalid_output(api):
    app, _ = api
    row, txs, tx_outputs = prepare_block(vector_block())
    tx_outputs[0]["amount"] = -1
    with pytest.raises(IntegrityError):
        await app.state.db.save_block(row, txs, tx_outputs)
    assert await app.state.db.tip() is None
