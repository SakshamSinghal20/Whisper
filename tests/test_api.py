import pytest
from sqlalchemy import select

from whisper.database import blocks, outputs
from whisper.indexer import prepare_block

from .conftest import vector_block

PREFIX = "3e9fce73"


async def test_empty_status_does_not_invent_height(api):
    _, client = api
    response = await client.get("/api/v1/status")
    assert response.status_code == 200
    assert response.json()["tip_height"] is None
    assert response.json()["total_blocks"] == 0
    assert response.json()["indexer"] == "disabled"


async def test_scan_refuses_missing_coverage(api):
    _, client = api
    response = await client.post(
        "/api/v1/scan", json={"start_height": 0, "end_height": 0, "prefixes": [PREFIX]}
    )
    assert response.status_code == 409


@pytest.mark.parametrize(
    "changes,status",
    [
        ({"start_height": -1}, 422),
        ({"start_height": True}, 422),
        ({"end_height": 2147483648}, 422),
        ({"end_height": 3}, 400),
        ({"start_height": 2, "end_height": 1}, 422),
        ({"prefixes": []}, 422),
        ({"prefixes": ["0x" + PREFIX]}, 422),
        ({"prefixes": ["a"]}, 422),
        ({"prefixes": ["zzzzzzzz"]}, 422),
        ({"include_proofs": True}, 422),
        ({"scan_pubkey": "ab" * 32}, 422),
        ({"cursor": "garbage"}, 400),
    ],
)
async def test_scan_validation(indexed, changes, status):
    _, client = indexed
    body = {"start_height": 0, "end_height": 2, "prefixes": [PREFIX], **changes}
    assert (await client.post("/api/v1/scan", json=body)).status_code == status


async def test_prefix_scan_pagination_and_real_block_time(indexed):
    _, client = indexed
    body = {"start_height": 0, "end_height": 2, "prefixes": [PREFIX.upper(), PREFIX]}
    seen = []
    while True:
        response = await client.post("/api/v1/scan", json=body)
        assert response.status_code == 200
        page = response.json()
        assert page["scanned_blocks"] == [0, 1, 2]
        seen.extend(page["candidates"])
        if not page["next_cursor"]:
            break
        body["cursor"] = page["next_cursor"]
    assert [r["block_height"] for r in seen] == [0, 1, 2]
    assert seen[0]["timestamp"] == 1720000000
    assert seen[0]["amount"] == 100001


async def test_reorg_invalidates_cursor_and_cascades(indexed):
    app, client = indexed
    body = {"start_height": 0, "end_height": 2, "prefixes": [PREFIX]}
    body["cursor"] = (await client.post("/api/v1/scan", json=body)).json()["next_cursor"]
    await app.state.db.rollback_from(2)
    await app.state.db.save_block(*prepare_block(vector_block(2, block_hash="ff" * 32)))
    response = await client.post("/api/v1/scan", json=body)
    assert response.status_code == 409
    assert "Chain changed" in response.json()["error"]
    async with app.state.db.snapshot() as connection:
        assert len((await connection.execute(select(outputs))).all()) == 3


async def test_database_failure_is_not_healthy(indexed):
    app, client = indexed
    async with app.state.db.snapshot() as connection:
        await connection.run_sync(blocks.drop)
    response = await client.get("/api/v1/status")
    assert response.status_code == 503
    assert "SELECT" not in response.text
    assert "unavailable" in response.json()["error"]


async def test_static_assets_and_missing_api_routes(api):
    _, client = api
    assert (await client.get("/")).status_code == 200
    assert "javascript" in (await client.get("/assets/app.js")).headers["content-type"]
    response = await client.get("/api/v1/no-such-route")
    assert response.status_code == 404
    assert response.headers["content-type"].startswith("application/json")
    assert response.headers["x-content-type-options"] == "nosniff"
    assert (await client.get("/assets/no-such.js")).status_code == 404


async def test_body_limit(api):
    _, client = api
    response = await client.post("/api/v1/scan", content=b" " * 65537)
    assert response.status_code == 413


async def test_idempotent_block_commit(indexed):
    app, client = indexed
    await app.state.db.save_block(*prepare_block(vector_block(2)))
    status = (await client.get("/api/v1/status")).json()
    assert status["total_outputs"] == status["total_blocks"] == 3


async def test_block_gap_rejected(indexed):
    app, _ = indexed
    with pytest.raises(ValueError, match="extend"):
        await app.state.db.save_block(*prepare_block(vector_block(5)))


async def test_cursor_cannot_be_reused_for_another_query(indexed):
    _, client = indexed
    body = {"start_height": 0, "end_height": 2, "prefixes": [PREFIX]}
    page = (await client.post("/api/v1/scan", json=body)).json()
    body.update(cursor=page["next_cursor"], prefixes=["ffffffff"])
    assert (await client.post("/api/v1/scan", json=body)).status_code == 400
