"""Optional real-node smoke test. Set BITCOIND to a Bitcoin Core executable."""

import asyncio
import os
import socket
import subprocess

import httpx
import pytest
from coincurve import PrivateKey

from whisper.client import SilentPaymentClient
from whisper.config import Settings
from whisper.core import ScanKey, transaction_tweak
from whisper.indexer import Indexer
from whisper.rpc import BitcoinRPC


@pytest.mark.skipif(not os.environ.get("BITCOIND"), reason="Set BITCOIND to run real-node tests")
async def test_real_bitcoin_catchup_restart_reorg_and_zmq(api, tmp_path):
    app, http = api
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        zmq_port = sock.getsockname()[1]
    process = await asyncio.to_thread(
        subprocess.Popen,
        [
            os.environ["BITCOIND"],
            "-regtest",
            "-server",
            "-listen=0",
            "-discover=0",
            "-dnsseed=0",
            "-rpcuser=whisper-test",
            "-rpcpassword=local-test-only",
            "-rpcbind=127.0.0.1",
            "-rpcallowip=127.0.0.1",
            f"-rpcport={port}",
            f"-datadir={tmp_path}",
            f"-zmqpubhashblock=tcp://127.0.0.1:{zmq_port}",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
    )
    settings = Settings(
        _env_file=None,
        bitcoin_rpc_url=f"http://127.0.0.1:{port}",
        bitcoin_rpc_user="whisper-test",
        bitcoin_rpc_pass="local-test-only",
        zmq_block_socket=f"tcp://127.0.0.1:{zmq_port}",
    )
    rpc = BitcoinRPC(settings)
    listener = None
    try:
        for _ in range(100):
            try:
                await rpc.call("getblockchaininfo")
                break
            except (httpx.HTTPError, RuntimeError):
                await asyncio.sleep(0.1)
        else:
            pytest.fail("Bitcoin Core did not start")
        await rpc.call("createwallet", "smoke")
        address = await rpc.call("getnewaddress", "", "bech32m")
        await rpc.call("generatetoaddress", 3, address)
        indexer = Indexer(app.state.db, rpc, settings)
        await indexer.sync_once()
        assert (await app.state.db.tip())["height"] == 3
        async with app.state.db.snapshot() as connection:
            assert (await app.state.db.counts(connection))["total_outputs"] == 3
        # A fresh indexer resumes from the persisted database.
        indexer = Indexer(app.state.db, rpc, settings)
        listener = asyncio.create_task(indexer.listen())
        await asyncio.sleep(0.2)
        old_hash = (await app.state.db.tip())["hash"]
        await rpc.call("invalidateblock", old_hash)
        replacement_address = await rpc.call("getnewaddress", "", "bech32m")
        await rpc.call("generatetoaddress", 2, replacement_address)
        await asyncio.wait_for(indexer.wake.wait(), timeout=5)
        await indexer.sync_once()
        assert (await app.state.db.tip())["height"] == 4
        assert indexer.state.reorgs == 1
        assert (await app.state.db.tip())["hash"] != old_hash
        # Build a real labeled payment from a mature P2TR coinbase input.
        await rpc.call("generatetoaddress", 101, replacement_address)
        coin = (await rpc.call("listunspent", 100))[0]
        inputs = [
            {
                "txid": coin["txid"],
                "vout": coin["vout"],
                "prevout": {"scriptPubKey": {"hex": coin["scriptPubKey"]}},
            }
        ]
        scan_key = ScanKey(bytes([1]) * 32)
        spend = PrivateKey(bytes([2]) * 32).public_key.format()
        tweak = transaction_tweak(inputs)
        output_key, _ = scan_key.derive_output(scan_key.shared_secret(tweak), spend, label=3)
        descriptor = await rpc.call("getdescriptorinfo", f"rawtr({output_key.hex()})")
        destination = (await rpc.call("deriveaddresses", descriptor["descriptor"]))[0]
        raw = await rpc.call(
            "createrawtransaction",
            [{"txid": coin["txid"], "vout": coin["vout"]}],
            [{destination: float(coin["amount"]) - 0.001}],
        )
        signed = await rpc.call("signrawtransactionwithwallet", raw)
        assert signed["complete"]
        payment_txid = await rpc.call("sendrawtransaction", signed["hex"])
        await rpc.call("generatetoaddress", 1, replacement_address)
        await indexer.sync_once()
        tip_height = (await app.state.db.tip())["height"]
        client = SilentPaymentClient("http://test", scan_key, spend, (3,), http_client=http)
        found = await client.scan_range(tip_height, tip_height)
        assert len(found) == 1
        assert found[0].txid == payment_txid
        assert found[0].label == 3
        assert found[0].amount == int(coin["amount"] * 100_000_000) - 100_000
    finally:
        if listener:
            listener.cancel()
            await asyncio.gather(listener, return_exceptions=True)
        try:
            await rpc.call("stop")
        except Exception:
            process.terminate()
        await rpc.close()
        await asyncio.to_thread(process.wait, timeout=10)
