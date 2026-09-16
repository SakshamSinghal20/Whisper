import itertools
from decimal import Decimal
from typing import Any

import httpx

from whisper.config import Settings


class RPCError(RuntimeError):
    pass


class BitcoinRPC:
    def __init__(self, settings: Settings):
        self.client = httpx.AsyncClient(
            base_url=settings.bitcoin_rpc_url,
            auth=(settings.bitcoin_rpc_user, settings.bitcoin_rpc_pass.get_secret_value()),
            timeout=httpx.Timeout(60, connect=5),
            trust_env=False,
        )
        self.ids = itertools.count(1)

    async def call(self, method: str, *params: Any) -> Any:
        request_id = next(self.ids)
        response = await self.client.post(
            "",
            json={
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": list(params),
            },
        )
        try:
            payload = response.json(parse_float=Decimal)
        except ValueError as exc:
            raise RPCError("Bitcoin Core returned an invalid JSON response") from exc
        if not isinstance(payload, dict) or payload.get("id") != request_id:
            raise RPCError("Bitcoin Core returned a mismatched response")
        if payload.get("error"):
            error = payload["error"]
            raise RPCError(f"Bitcoin Core {method} failed (code {error.get('code', 'unknown')})")
        response.raise_for_status()
        if "result" not in payload:
            raise RPCError("Bitcoin Core response has no result")
        return payload["result"]

    async def close(self) -> None:
        await self.client.aclose()
