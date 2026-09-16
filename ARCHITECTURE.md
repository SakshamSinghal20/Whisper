# Architecture

Whisper is a single-process Python service with an async receiver SDK and a static browser dashboard.

| Module | Responsibility |
| --- | --- |
| `whisper/core.py` | BIP-352 input extraction, transaction tweaks, ECDH, labels, output ownership checks |
| `whisper/address.py` | Bech32m version-0 Silent Payment addresses |
| `whisper/client.py` | Bounded HTTP responses, pagination, local scanning, response validation |
| `whisper/rpc.py` | Authenticated Bitcoin Core RPC, request-ID validation, exact decimal amounts |
| `whisper/indexer.py` | Canonical-chain reconciliation, backfill, reorg recovery, optional ZMQ notifications |
| `whisper/database.py` | SQLAlchemy schema and atomic SQLite/PostgreSQL operations |
| `whisper/models.py` | Strict API input and client response models |
| `whisper/server.py` | FastAPI lifespan, health, scan endpoints, static assets, error handling |
| `whisper/config.py` | Validated environment and `.env` configuration |
| `whisper/static/` | Responsive dashboard with service states, recent blocks, and API explorer |

## Indexing

The indexer asks Bitcoin Core for its network and canonical height. It compares the persisted tip with the node, deleting replaced blocks until it finds a common ancestor. Foreign keys cascade those deletions to transactions and outputs. It then fetches each missing block with `getblock` verbosity 3.

For transactions containing Taproot outputs, the indexer derives a public input tweak from eligible input keys and the smallest serialized outpoint. Every non-coinbase input needs a prevout, even if its key type is ineligible. Missing prevouts fail the block rather than silently omit potentially relevant payments. Amounts use Decimal-to-satoshi conversion; hashes retain Bitcoin Core's conventional display order.

Each block and its outputs commit in one database transaction. Repeated delivery of the same block is idempotent. New blocks must extend the indexed chain. ZMQ only wakes reconciliation; polling recovers missed notifications and downtime. Failed synchronization retries with a capped delay. Shutdown cancels background work and closes sockets, HTTP connections, and the database pool.

## Storage

`blocks` stores the canonical height, hash, parent hash, timestamp, and counts. `transactions` stores Taproot-bearing transaction IDs, heights, and an optional 33-byte public input tweak. `taproot_outputs` stores outpoints, integer amounts, scripts, prefixes, and heights. Ineligible transactions still contribute Taproot candidates but do not appear in the local-scanning transaction endpoint.

`whisper_meta` binds the index to a schema version, network, and starting height. Startup refuses legacy Rust schemas and configuration mismatches. It does not overwrite or migrate an existing old index.

An async lock gives each API query a coherent snapshot while the single indexer commits or rolls back. Run one process per database; multiple Uvicorn workers or separate writer instances are unsupported. Database constraints enforce unique heights/outpoints, foreign keys, and valid amount ranges.

## Query consistency

Scan coverage is based on rows that exist, including blocks with no Taproot outputs. A query only succeeds when the complete inclusive range exists. Keyset pagination orders candidates by height, transaction ID, and output index, or transactions by height and transaction ID.

A cursor binds that position to the request parameters and the end-block hash. A reorganization invalidates it. Transaction responses also contain the previous hash at the start of the range, allowing the Python client to detect a chain change between adjacent range batches. Cursors are navigation tokens, not authorization credentials.

Candidate pages obey `PAGE_SIZE`. Transaction pages contain at most 25 transactions and stop around 10,000 outputs without splitting a transaction; one large transaction can exceed that target. Client HTTP responses are capped at 32 MiB. Query bounds and 64 KiB request limits prevent unbounded ordinary requests. Production reverse proxies should apply request rate limits.

## Receiver and trust boundary

Only the client holds the scan secret. It multiplies the public transaction tweak by that secret, derives the BIP-352 shared-secret tweaks in counter order, and tests unlabeled and labeled output keys. It always checks change label zero and stops after the first unmatched counter. Public spend keys keep their Y parity; only the final Taproot output is x-only.

The cryptographic point operations run in coincurve's libsecp256k1 backend. Python manages metadata and orchestration; it does not promise secure memory erasure. Output matching establishes ownership of an output key, not the authenticity of a server's claimed amount, transaction inclusion, confirmations, or unspent status.

## Dashboard

The dashboard uses the same API as the SDK. Polling waits for each request pair to finish, pauses in background tabs, and supports manual refresh and pause/resume. Connection failures retain and label previous metrics. The explorer derives the HTTP method from the endpoint, validates fields, supports pagination, and times out requests. Remote response content is rendered as text.

Assets, including the Outfit font, ship inside the Python package. Static serving uses module-relative paths, so launching from another directory or installing a wheel does not lose the dashboard. CSP restricts resources to the service origin.
