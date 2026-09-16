import pytest
from pydantic import ValidationError
from sqlalchemy import text

from whisper.config import Settings
from whisper.database import Database


@pytest.mark.parametrize(
    "values",
    [
        {"max_block_range": 0},
        {"page_size": 0},
        {"network": "madeup"},
        {"bitcoin_rpc_url": "file:///local"},
        {"bitcoin_rpc_url": "http://user:secret@localhost"},
        {"port": 70000},
        {"start_height": -1},
    ],
)
def test_configuration_validation(values):
    with pytest.raises(ValidationError):
        Settings(_env_file=None, **values)


def test_postgres_url_compatibility():
    assert (
        Settings(_env_file=None, database_url="postgres://localhost/test").database_url
        == "postgresql+asyncpg://localhost/test"
    )


async def test_legacy_database_is_preserved(tmp_path):
    db = Database(f"sqlite+aiosqlite:///{tmp_path}/legacy.db")
    try:
        async with db.engine.begin() as connection:
            await connection.execute(
                text("CREATE TABLE blocks (height INTEGER PRIMARY KEY, hash BLOB)")
            )
            await connection.execute(text("INSERT INTO blocks VALUES (1, X'01')"))
        with pytest.raises(RuntimeError, match="Legacy Rust"):
            await db.initialize("regtest", 0)
        async with db.engine.connect() as connection:
            assert await connection.scalar(text("SELECT COUNT(*) FROM blocks")) == 1
    finally:
        await db.close()


async def test_database_network_cannot_change(api):
    app, _ = api
    with pytest.raises(RuntimeError, match="network"):
        await app.state.db.initialize("mainnet", 0)
