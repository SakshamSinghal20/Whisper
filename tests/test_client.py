import json

import httpx
import pytest
from coincurve import PrivateKey

from whisper.client import ClientError, SilentPaymentClient
from whisper.core import ScanKey

from .test_core import RECEIVERS


def receiver(http):
    keys = RECEIVERS[0][1]["given"]["key_material"]
    return SilentPaymentClient(
        "http://test/",
        ScanKey(bytes.fromhex(keys["scan_priv_key"])),
        PrivateKey(bytes.fromhex(keys["spend_priv_key"])).public_key.format(),
        http_client=http,
    )


async def test_end_to_end_scan_with_pagination(indexed):
    _, http = indexed
    client = receiver(http)
    results = await client.scan_range(0, 2)
    assert len(results) == 3
    assert [r.amount for r in results] == [100001] * 3
    assert [r.block_height for r in results] == [0, 1, 2]
    assert results[0].txid == f"{100:064x}"


async def test_missing_blocks_raise_instead_of_empty_wallet(api):
    _, http = api
    with pytest.raises(ClientError, match="409"):
        await receiver(http).scan_range(0, 1)


@pytest.mark.parametrize("mutation", ["negative_amount", "wrong_txid", "coverage", "repeat_cursor"])
async def test_client_rejects_bad_server_metadata(indexed, mutation):
    _, http = indexed
    payload = (await http.get("/api/v1/transactions?start_height=0&end_height=2")).json()
    if mutation == "negative_amount":
        payload["transactions"][0]["outputs"][0]["amount"] = -1
    elif mutation == "wrong_txid":
        payload["transactions"][0]["outputs"][0]["txid"] = "00" * 32
    elif mutation == "coverage":
        payload["scanned_blocks"] = [0]
    requests = []

    def handler(request):
        requests.append(request)
        if request.url.path.endswith("status"):
            return httpx.Response(200, json={"limits": {"max_block_range": 3}})
        return httpx.Response(200, json=payload)

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as mocked:
        with pytest.raises(ClientError):
            await receiver(mocked).scan_range(0, 2)
    assert all("scan_pubkey" not in str(r.url) for r in requests)
    assert all(not r.content for r in requests)


async def test_client_never_transmits_scan_key(indexed):
    _, http = indexed
    requests = []

    async def capture(request):
        requests.append(str(request.url) + request.content.decode())

    http.event_hooks["request"] = [capture]
    client = receiver(http)
    await client.scan_range(0, 2)
    wire = json.dumps(requests)
    assert client.scan_key.secret.hex() not in wire
    assert client.scan_key.public.hex() not in wire
