import copy
import json
from pathlib import Path

import httpx
import pytest

from whisper.config import Settings
from whisper.indexer import prepare_block
from whisper.server import create_app


def vector_block(height=0, block_hash=None, previous_hash=None):
    fixture = json.loads((Path(__file__).parent / "fixtures/bip352.json").read_text())[0]
    receiver = fixture["receiving"][0]
    return {
        "height": height,
        "hash": block_hash or f"{height + 1:064x}",
        "previousblockhash": previous_hash or f"{height:064x}",
        "time": 1720000000 + height * 600,
        "tx": [
            {
                "txid": f"{height + 100:064x}",
                "vin": copy.deepcopy(receiver["given"]["vin"]),
                "vout": [
                    {
                        "n": 0,
                        "value": "0.00100001",
                        "scriptPubKey": {"hex": "5120" + receiver["given"]["outputs"][0]},
                    }
                ],
            }
        ],
    }


@pytest.fixture
async def api(tmp_path):
    settings = Settings(
        _env_file=None,
        database_url=f"sqlite+aiosqlite:///{tmp_path}/test.db",
        indexer_enabled=False,
        page_size=1,
        max_block_range=3,
    )
    app = create_app(settings)
    async with app.router.lifespan_context(app):
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=app), base_url="http://test"
        ) as client:
            yield app, client


@pytest.fixture
async def indexed(api):
    app, client = api
    for height in range(3):
        await app.state.db.save_block(*prepare_block(vector_block(height)))
    return app, client
