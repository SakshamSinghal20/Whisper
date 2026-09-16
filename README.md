# Whisper

A Python Bitcoin Taproot indexer, operator dashboard, and Silent Payments receiver library. FastAPI serves the API; SQLAlchemy stores the canonical chain in SQLite or PostgreSQL; the client verifies BIP-352 output ownership locally using libsecp256k1 through coincurve.

The server never needs a scan secret or scan public key. The browser dashboard remains HTML, CSS, and JavaScript, with local assets and no external font requests.

## Run locally

Requires Python 3.11+. No Rust toolchain, Node build, or database service is needed for the SQLite setup.

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -c requirements.lock -e ".[dev]"
Copy-Item .env.example .env
python -m whisper
```

On macOS/Linux, activate with `source .venv/bin/activate` and copy configuration with `cp .env.example .env`. The remaining Python commands are the same. If PowerShell activation is restricted, run `.\.venv\Scripts\python.exe` directly instead of `python`.

Open [the dashboard](http://127.0.0.1:3000). The API starts even when Bitcoin Core is unavailable; the dashboard reports the connection failure and the indexer retries. Set `INDEXER_ENABLED=false` to run the API against an existing index without contacting a node.

Configure an unpruned Bitcoin Core 26+ node for indexing:

```ini
# bitcoin.conf, for a local test network
regtest=1
server=1
txindex=1
rpcuser=bitcoin
rpcpassword=replace-this-password
zmqpubhashblock=tcp://127.0.0.1:28332
```

Set the matching RPC URL, credentials, and `NETWORK` in `.env`. Regtest RPC normally uses port 18443. Set `ZMQ_BLOCK_SOCKET=tcp://127.0.0.1:28332` to enable notifications. RPC polling remains active when ZMQ is absent or disconnected.

Whisper indexes from `START_HEIGHT` (default 0), resumes from its persisted tip, and removes replaced blocks and dependent outputs before following a new branch. Choose your network and starting height before populating a database; changing either requires a new index.

## Docker

```sh
docker compose up --build -d
```

The Compose setup runs a local regtest node, PostgreSQL, and Whisper. Only the dashboard port is published, bound to loopback. Generate test blocks when needed:

```sh
docker compose exec bitcoind bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=local-development-only createwallet demo
docker compose exec bitcoind bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=local-development-only getnewaddress "" bech32m
# Substitute the returned address:
docker compose exec bitcoind bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=local-development-only generatetoaddress 6 ADDRESS
```

These commands assume the Compose fallback password. If `.env` defines `BITCOIN_RPC_PASS`, use that value instead. Set your own credentials for shared environments. The PostgreSQL volume for Python is separate from the old Rust database volume.

## API

Machine-readable documentation: [`GET /openapi.json`](http://127.0.0.1:3000/openapi.json).

| Endpoint | Purpose |
| --- | --- |
| `GET /api/v1/status` | Actual database totals, network, indexer state, node height, limits |
| `GET /api/v1/blocks?limit=10` | Recently indexed canonical blocks and Bitcoin header timestamps |
| `POST /api/v1/scan` | Paginated Taproot candidates matching exact four-byte prefixes |
| `GET /api/v1/transactions?start_height=0&end_height=100` | Public transaction tweaks and outputs for local ownership checks |

A prefix query looks like this:

```json
{
  "start_height": 0,
  "end_height": 100,
  "prefixes": ["a1b2c3d4", "e5f6a7b8"]
}
```

Ranges are inclusive. A 1,000-block limit allows `0..999`. Prefixes must be exactly eight hexadecimal characters; duplicates are removed. Neither scan keys nor public scan keys belong in the request. Merkle proofs are not implemented, so `include_proofs=true` fails explicitly.

Scan responses include `scanned_blocks`, `chain_anchor`, and `next_cursor`. Pass `next_cursor` as `cursor` with the same query to continue. Every requested block must be indexed: incomplete coverage returns HTTP 409. A cursor from a replaced chain also returns 409; restart that scan. Input errors return 400/422, oversized request bodies return 413, and database failures return 503 without exposing SQL.

`/status` returns 503 while the enabled indexer is starting, failing, or stale. Existing covered ranges remain queryable. `tip_height=null` means no indexed blocks; height zero is a real genesis block. A disabled indexer is reported separately from a failed connection.

## Python client

```python
import os
from whisper.client import SilentPaymentClient
from whisper.core import ScanKey

scan_key = ScanKey(bytes.fromhex(os.environ["WHISPER_SCAN_SECRET"]))
spend_pubkey = bytes.fromhex(os.environ["WHISPER_SPEND_PUBKEY"])  # 33 bytes, compressed

async def receive():
    async with SilentPaymentClient(
        "http://127.0.0.1:3000", scan_key, spend_pubkey, labels=(1, 5),
    ) as client:
        payments = await client.scan_range(0, 100)
        for payment in payments:
            print(payment.txid, payment.vout, payment.amount, payment.label)
```

The client downloads paginated public transaction data, computes shared secrets locally, and checks sequential outputs and labels, including the reserved change label zero. It validates response metadata and chain continuity. It does not require you to supply transaction inputs by hand. See [`examples/scan.py`](examples/scan.py) for a command-line example.

`whisper.address.SilentPaymentAddress` encodes and decodes version-0 `sp`/`tsp` addresses and creates labeled addresses. `whisper.core.transaction_tweak` extracts eligible P2PKH, P2WPKH, P2SH-P2WPKH, and P2TR input keys from decoded transactions with prevouts.

## Configuration

Settings load from environment variables and `.env`; environment variables take precedence. See [`.env.example`](.env.example) for defaults.

- `DATABASE_URL`: SQLite by default; accepts `postgresql+asyncpg://...` and normal `postgresql://...` URLs.
- `START_HEIGHT`: first block to index; useful for wallet-birthday deployments.
- `POLL_INTERVAL`: seconds between RPC reconciliation attempts.
- `MAX_BLOCK_RANGE`, `MAX_PREFIXES`, `PAGE_SIZE`: bounded queries and pagination.
- `CORS_ORIGIN`: empty for same-origin access, or comma-separated allowed origins.
- `HOST`: loopback by default. Docker listens inside its network and publishes loopback only.

Run one Whisper process per database. Keep SQLite on local storage rather than a synchronized/network drive for sustained indexing. Use PostgreSQL for larger deployments. Put authentication, TLS, and per-client rate limits at your reverse proxy if you expose the service outside your machine.

## Tests

```sh
python -m pytest -q
python -m ruff check whisper tests examples
python -m ruff format --check whisper tests examples
```

The tests include all 29 receiving cases and their addresses from the published BIP-352 vectors, API/client integration, key privacy, invalid input, pagination, atomic block writes, restart recovery, and reorganizations. The fixture is vendored, so normal tests work offline after dependency installation.

A real-node test also mines a labeled payment on an isolated regtest chain and detects it through the Python client. It starts and stops its own node and never contacts mainnet:

```powershell
$env:BITCOIND = 'C:\path\to\bitcoind.exe'
python -m pytest tests/test_regtest.py -q
```

On POSIX shells use `BITCOIND=/path/to/bitcoind python -m pytest tests/test_regtest.py -q`. Without that variable, pytest skips the real-node test. Windows ZMQ may log that it uses a Tornado selector thread; the Windows dependency is included.

## Scope and trust

Whisper detects output ownership. Server-supplied amounts, inclusion, and confirmations still require a trusted node or independently verified proofs. This implementation has no independent cryptographic audit and is not a spending wallet. It does not track UTXO spend status, scan the mempool, or derive BIP32 wallet keys.

Four-byte prefixes disclose a filter on public output keys; they do **not** guarantee a 2^32 anonymity set. The receiver endpoint returns public tweaks and outputs, trading bandwidth for local verification. No fixed bandwidth reduction or throughput claim is made.

For conversion details and old-database handling, see [MIGRATION.md](MIGRATION.md). See [ARCHITECTURE.md](ARCHITECTURE.md) for the module map.

## License

[MIT](LICENSE). Upstream test-vector and font attribution appears in [THIRD_PARTY.md](THIRD_PARTY.md).
