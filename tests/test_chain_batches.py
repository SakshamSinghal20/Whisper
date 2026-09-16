import httpx
import pytest

from whisper.client import ClientError
from whisper.indexer import prepare_block

from .conftest import vector_block
from .test_client import receiver


async def test_client_splits_large_ranges_without_losing_payments(indexed):
    app, http = indexed
    await app.state.db.save_block(*prepare_block(vector_block(3)))
    results = await receiver(http).scan_range(0, 3)
    assert [result.block_height for result in results] == [0, 1, 2, 3]


async def test_reorg_between_range_batches_fails(indexed):
    app, http = indexed
    await app.state.db.save_block(*prepare_block(vector_block(3)))

    async def handler(request):
        response = await http.get(request.url.path, params=request.url.params)
        payload = response.json()
        if (
            request.url.path.endswith("transactions")
            and request.url.params["end_height"] == "2"
            and not payload["next_cursor"]
        ):
            await app.state.db.rollback_from(2)
            await app.state.db.save_block(*prepare_block(vector_block(2, "aa" * 32)))
            await app.state.db.save_block(*prepare_block(vector_block(3, "bb" * 32, "aa" * 32)))
        return httpx.Response(response.status_code, json=payload)

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as mocked:
        with pytest.raises(ClientError, match="between scan ranges"):
            await receiver(mocked).scan_range(0, 3)
