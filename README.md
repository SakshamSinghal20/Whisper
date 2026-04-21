# Whisper

> A privacy-preserving BIP-352 Silent Payments indexing service. Light clients detect Taproot-based silent payments without revealing scan keys to the server.

## Highlights

- **Privacy-Preserving** — 4-byte prefix filtering, 2³² anonymity set per query
- **Efficient** — 99.9% bandwidth reduction vs full block download
- **Real-time** — ZMQ-based block ingestion from Bitcoin Core
- **Scalable** — PostgreSQL with optimized B-tree indexes
- **Dashboard** — Built-in dark-themed admin UI at `http://localhost:3000`

## Architecture

```
┌──────────────────────┐
│   Client (Rust/FFI)  │   Local ECDH verification
│   Prefix Generation  │   Server never sees secrets
└──────────┬───────────┘
           │ HTTPS
┌──────────▼───────────┐
│   Whisper Server     │   Axum REST API
│   PostgreSQL Index   │   Prefix-based queries
└──────────┬───────────┘
           │ ZMQ
┌──────────▼───────────┐
│   Bitcoin Core       │   Raw block notifications
│   + txindex + ZMQ    │   Taproot output extraction
└──────────────────────┘
```

## Quick Start

### Docker (Recommended)

```bash
cp .env.example .env
docker-compose up -d
open http://localhost:3000        # Dashboard
curl http://localhost:3000/api/v1/status
```

### Manual

**Prerequisites:** Rust 1.75+, PostgreSQL 15+, Bitcoin Core 26.0+ with ZMQ

```bash
# Database
createdb whisper
export DATABASE_URL=postgres://user:pass@localhost/whisper

# Bitcoin Core (bitcoin.conf)
# regtest=1, server=1, txindex=1
# zmqpubrawblock=tcp://127.0.0.1:28332

# Build & Run
cargo build --release
cd whisper-server && cargo run --release
```

## API

### `POST /api/v1/scan`

Query blocks for Silent Payment candidates by prefix.

```json
// Request
{
  "scan_pubkey": "02a1b2c3...",
  "start_height": 100,
  "end_height": 200,
  "prefixes": ["a1b2c3d4", "e5f6a7b8"]
}

// Response
{
  "candidates": [{
    "txid": "abc123...",
    "vout": 0,
    "amount": 100000,
    "script_pubkey": "5120...",
    "block_height": 150,
    "block_hash": "000000...",
    "timestamp": 1234567890
  }],
  "scanned_blocks": [100, 101, ...],
  "server_time_ms": 45
}
```

### `GET /api/v1/status`

```json
{
  "status": "ok",
  "version": "0.1.0",
  "tip_height": 12345,
  "total_outputs": 98765,
  "total_blocks": 12345,
  "network": "regtest",
  "uptime_seconds": 3600
}
```

## Client Library

```rust
use whisper_client::SilentPaymentClient;
use whisper_core::{ScanKey, InputData};

let scan_key = ScanKey::from_slice(&secret_bytes)?;
let client = SilentPaymentClient::new(
    "http://localhost:3000".into(),
    scan_key, spend_pubkey, 10,
);

let results = client.scan_range(100, 200, &inputs).await?;
for r in results {
    println!("Payment: {} sats (label: {:?})", r.amount, r.label);
}
```

## Project Structure

```
whisper/
├── whisper-core/           # BIP-352 cryptographic library
│   └── src/lib.rs          # ECDH, tagged hashes, output derivation
├── whisper-server/         # Indexer & REST API
│   ├── src/
│   │   ├── main.rs         # Server entry, static file serving
│   │   ├── api.rs          # REST endpoints with validation
│   │   ├── indexer.rs      # ZMQ block ingestion with reconnect
│   │   └── config.rs       # Environment configuration
│   ├── migrations/         # PostgreSQL schema
│   └── static/             # Dashboard UI
└── whisper-client/         # Async client library
    └── examples/           # Usage example
```

## Security

| Layer | Protection |
|-------|-----------|
| **Crypto** | BIP-352 tagged hashes, proper ECDH (mul_tweak) |
| **Privacy** | 4-byte prefix = 2³² anonymity set, no key leakage |
| **API** | Input validation, block range limits, prefix count limits |
| **Database** | Parameterized queries (sqlx), FK constraints, CHECK constraints |
| **Server** | Configurable CORS, security headers, graceful shutdown |

## Development

```bash
make build       # Build all crates
make test        # Run all tests
make run         # Start server
make check       # Format + Clippy + Test
```

## BIP-352 Compliance

- Tagged hashes: `BIP0352/SharedSecret`, `BIP0352/Outputs`
- ECDH using secp256k1 scalar multiplication
- X-only public keys (BIP-340, even-Y assumption)
- Label support (m = 1..255)

## Roadmap

- [x] Core BIP-352 implementation
- [x] PostgreSQL schema & indexing
- [x] REST API with validation
- [x] Client library with local verification
- [x] Dashboard UI
- [ ] FFI bindings (UniFFI for iOS/Android)
- [ ] Automatic reorg handling
- [ ] Production monitoring (Prometheus)

## License

MIT — See [LICENSE](LICENSE) for details.

> ⚠️ **Cryptographic Warning:** This software handles private keys and Bitcoin transactions. Audit thoroughly before production use.
