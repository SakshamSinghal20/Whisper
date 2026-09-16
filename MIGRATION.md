# Moving from Rust 0.1 to Python 0.2

The Python package replaces all three Rust crates. The old source remains available in Git history. No Rust compiler, Cargo workspace, or SQLx build-time database is required.

## Database

Use a **new database**. The old schema has no public transaction tweaks, uses insertion times instead of block times, and cannot represent the new receiver protocol. The Python server detects it and refuses startup without changing it. Keep any old database or Docker volume until you have verified the new index.

SQLite uses a new `whisper.db` by default. For PostgreSQL, create a new database such as `whisper_python`, point `DATABASE_URL` at it, and let Whisper reindex from Bitcoin Core. The new Compose file uses the separate `postgres_python_data` volume. `docker compose down` does not remove volumes.

There is no private wallet state to migrate from the indexer. Existing private scan keys can be used locally, but public scan/spend keys must retain their full 33-byte compressed representation. A 32-byte x-only spend key cannot recover its original parity by itself.

## API and SDK

- `scan_pubkey` is removed from scan requests. The server never used it for lookup; new requests contain only heights, prefixes, and an optional cursor.
- Prefixes are exactly eight hex characters, without `0x`. Requests with incomplete indexed coverage return 409. Range limits count both endpoints.
- Scan responses are paginated. Follow `next_cursor` with the same query; restart on a chain-change response.
- `tip_height` is null when the database is empty. Status failures no longer return healthy zero totals.
- Block timestamps come from Bitcoin headers. Hashes and transaction IDs use standard display order.
- The Python client's `scan_range(start, end)` obtains public transaction context from the server. It does not reuse one supplied input set across an entire block range.
- Labels support unsigned 32-bit values and include the reserved change label zero. Output counters follow BIP-352.

The Rust derivation omitted required BIP-352 input hashes and counters and used different label derivation. Outputs derived with that old algorithm are not BIP-352-compatible. Keep the original source and any wallet recovery material for investigating such outputs; the new receiver must not pretend to recognize them as standard Silent Payments.

## Operations

Replace Cargo commands with `python -m whisper`, `python -m pytest`, and Ruff. The service loads `.env` itself. Static assets are included in the package, and the Docker image runs as a non-root user.

Use one server process per database. RPC backfill is required even when ZMQ is enabled. Match the configured network to the node and use an archival node for historical prevouts.
