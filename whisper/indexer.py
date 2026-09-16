import asyncio
import logging
import time
from dataclasses import dataclass
from decimal import Decimal

from whisper.config import Settings
from whisper.core import transaction_tweak
from whisper.database import Database
from whisper.rpc import BitcoinRPC

logger = logging.getLogger(__name__)
CHAINS = {
    "mainnet": "main",
    "testnet": "test",
    "testnet4": "testnet4",
    "signet": "signet",
    "regtest": "regtest",
}


@dataclass
class IndexerState:
    status: str = "starting"
    node_height: int | None = None
    last_success: float | None = None
    error: str | None = None
    reorgs: int = 0


def prepare_block(block: dict) -> tuple[dict, list[dict], list[dict]]:
    height = block["height"]
    txs, outputs = [], []
    for tx in block["tx"]:
        candidates = [
            out
            for out in tx["vout"]
            if (
                len(out["scriptPubKey"]["hex"]) == 68
                and out["scriptPubKey"]["hex"].startswith("5120")
            )
        ]
        if not candidates:
            continue
        tweak = transaction_tweak(tx["vin"])
        txs.append({"txid": tx["txid"], "block_height": height, "input_tweak": tweak})
        for output in candidates:
            sats = Decimal(str(output["value"])) * 100_000_000
            if sats != sats.to_integral_value() or not 0 <= sats <= 2_100_000_000_000_000:
                raise ValueError("Invalid Bitcoin output amount")
            script = output["scriptPubKey"]["hex"].lower()
            outputs.append(
                {
                    "txid": tx["txid"],
                    "vout": output["n"],
                    "block_height": height,
                    "script_pubkey": script,
                    "amount": int(sats),
                    "prefix": script[4:12],
                }
            )
    row = {
        "height": height,
        "hash": block["hash"],
        "previous_hash": block.get("previousblockhash", "0" * 64),
        "timestamp": block["time"],
        "tx_count": len(block["tx"]),
        "output_count": len(outputs),
    }
    return row, txs, outputs


class Indexer:
    def __init__(self, db: Database, rpc: BitcoinRPC, settings: Settings):
        self.db, self.rpc, self.settings = db, rpc, settings
        self.state = IndexerState()
        self.wake = asyncio.Event()

    async def sync_once(self) -> None:
        info = await self.rpc.call("getblockchaininfo")
        if info["chain"] != CHAINS[self.settings.network]:
            raise ValueError("Bitcoin Core network differs from NETWORK")
        self.state.node_height = info["blocks"]
        self.state.status = "syncing"
        self.state.error = None
        tip = await self.db.tip()
        fork = False
        while tip:
            canonical = (
                (await self.rpc.call("getblockhash", tip["height"]))
                if tip["height"] <= info["blocks"]
                else None
            )
            if canonical == tip["hash"]:
                break
            await self.db.rollback_from(tip["height"])
            fork = True
            tip = await self.db.tip()
        if fork:
            self.state.reorgs += 1
            logger.warning(
                "Rolled back replaced blocks to height %s", tip["height"] if tip else None
            )
        next_height = tip["height"] + 1 if tip else self.settings.start_height
        if info.get("pruned") and next_height < info.get("pruneheight", 0):
            raise ValueError(
                "Required history is pruned; use an archival node or a new index start"
            )
        for height in range(next_height, info["blocks"] + 1):
            block_hash = await self.rpc.call("getblockhash", height)
            block = await self.rpc.call("getblock", block_hash, 3)
            if block["hash"] != block_hash or block["height"] != height:
                raise ValueError("Bitcoin Core returned inconsistent block metadata")
            prepared = await asyncio.to_thread(prepare_block, block)
            # A notification is only a hint; recheck the active chain before commit.
            if await self.rpc.call("getblockhash", height) != block_hash:
                self.wake.set()
                return
            await self.db.save_block(*prepared)
            self.state.last_success = time.time()
        self.state.last_success = time.time()
        self.state.status = "synced"

    async def listen(self) -> None:
        import zmq
        import zmq.asyncio

        context = zmq.asyncio.Context()
        socket = context.socket(zmq.SUB)
        socket.setsockopt(zmq.LINGER, 0)
        socket.setsockopt(zmq.RCVHWM, 10)
        socket.setsockopt(zmq.SUBSCRIBE, b"hashblock")
        socket.setsockopt(zmq.SUBSCRIBE, b"rawblock")
        try:
            socket.connect(self.settings.zmq_block_socket)
            while True:
                await socket.recv_multipart()
                self.wake.set()
        except asyncio.CancelledError:
            raise
        except Exception:
            logger.exception("ZMQ listener stopped; RPC polling remains active")
        finally:
            socket.close()
            context.term()

    async def run(self) -> None:
        listener = asyncio.create_task(self.listen()) if self.settings.zmq_block_socket else None
        failures = 0
        try:
            while True:
                self.wake.clear()
                try:
                    await self.sync_once()
                    failures = 0
                except asyncio.CancelledError:
                    raise
                except Exception as exc:
                    failures += 1
                    self.state.status = "error"
                    self.state.error = (
                        str(exc)
                        if isinstance(exc, ValueError)
                        else "Cannot index Bitcoin Core. Check the RPC connection and server logs."
                    )
                    logger.warning("Indexer attempt failed (%s): %s", type(exc).__name__, exc)
                delay = min(60, self.settings.poll_interval * 2 ** min(failures, 4))
                try:
                    await asyncio.wait_for(self.wake.wait(), timeout=delay)
                except TimeoutError:
                    pass
        finally:
            self.state.status = "stopped"
            if listener:
                listener.cancel()
                await asyncio.gather(listener, return_exceptions=True)
