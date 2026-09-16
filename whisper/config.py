from typing import Literal
from urllib.parse import urlparse

from pydantic import Field, SecretStr, field_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    database_url: str = "sqlite+aiosqlite:///./whisper.db"
    bitcoin_rpc_url: str = "http://127.0.0.1:18443"
    bitcoin_rpc_user: str = "bitcoin"
    bitcoin_rpc_pass: SecretStr = SecretStr("password")
    network: Literal["mainnet", "testnet", "testnet4", "signet", "regtest"] = "regtest"
    host: str = "127.0.0.1"
    port: int = Field(default=3000, ge=1, le=65535)
    indexer_enabled: bool = True
    start_height: int = Field(default=0, ge=0, le=2_147_483_647)
    poll_interval: float = Field(default=5, ge=0.1, le=300)
    zmq_block_socket: str = ""
    max_block_range: int = Field(default=1000, ge=1, le=10000)
    max_prefixes: int = Field(default=1000, ge=1, le=10000)
    page_size: int = Field(default=100, ge=1, le=500)
    cors_origin: str = ""

    @field_validator("database_url")
    @classmethod
    def database_scheme(cls, value: str) -> str:
        for prefix in ("postgres://", "postgresql://"):
            if value.startswith(prefix):
                return value.replace(prefix, "postgresql+asyncpg://", 1)
        if not value.startswith(("sqlite+aiosqlite://", "postgresql+asyncpg://")):
            raise ValueError("Use sqlite+aiosqlite:// or postgresql+asyncpg://")
        return value

    @field_validator("bitcoin_rpc_url")
    @classmethod
    def rpc_url(cls, value: str) -> str:
        parsed = urlparse(value)
        if parsed.scheme not in {"http", "https"} or not parsed.hostname:
            raise ValueError("Bitcoin RPC URL must be an HTTP(S) URL")
        if parsed.username or parsed.password:
            raise ValueError("Set RPC credentials with BITCOIN_RPC_USER and BITCOIN_RPC_PASS")
        return value
