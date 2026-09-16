"""Scan an indexed range without sending receiver keys to the server."""

import argparse
import asyncio
import os

from whisper.client import SilentPaymentClient
from whisper.core import ScanKey


async def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:3000")
    parser.add_argument("--start", type=int, required=True)
    parser.add_argument("--end", type=int, required=True)
    parser.add_argument(
        "--spend-key", required=True, help="Compressed public spend key (66 hex chars)"
    )
    parser.add_argument("--label", type=int, action="append", default=[])
    args = parser.parse_args()
    secret = os.environ.get("WHISPER_SCAN_SECRET")
    if not secret:
        parser.error("Set WHISPER_SCAN_SECRET to your 32-byte scan secret in hex")
    async with SilentPaymentClient(
        args.url, ScanKey(bytes.fromhex(secret)), bytes.fromhex(args.spend_key), tuple(args.label)
    ) as client:
        results = await client.scan_range(args.start, args.end)
    for result in results:
        print(f"{result.txid}:{result.vout}  {result.amount} sats  label={result.label}")
    print(f"{len(results)} matching output(s). Verify confirmations with your trusted node.")


if __name__ == "__main__":
    asyncio.run(main())
