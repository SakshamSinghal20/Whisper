# Whisper Project Structure

```
whisper/
├── Cargo.toml                      # Workspace configuration
├── README.md                       # Project overview
├── SETUP.md                        # Detailed setup guide
├── .env.example                    # Environment template
├── .gitignore                      # Git ignore rules
├── docker-compose.yml              # Docker orchestration
├── Dockerfile                      # Server container image
├── Makefile                        # Common tasks
├── quickstart.sh                   # Quick setup script
│
├── whisper-core/                   # Core cryptographic library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  # BIP-352 implementation
│       └── tests.rs                # Comprehensive tests
│
├── whisper-server/                 # Indexer & API server
│   ├── Cargo.toml
│   ├── migrations/
│   │   └── 001_initial_schema.sql  # Database schema
│   └── src/
│       ├── main.rs                 # Server entry point
│       ├── api.rs                  # REST API handlers
│       ├── indexer.rs              # Block ingestion
│       └── config.rs               # Configuration
│
└── whisper-client/                 # Client library
    ├── Cargo.toml
    ├── examples/
    │   └── scan_example.rs         # Usage example
    └── src/
        └── lib.rs                  # Client implementation
```

## Component Descriptions

### whisper-core
**Purpose**: Pure cryptographic implementation of BIP-352 Silent Payments

**Key Features**:
- Tagged hash functions (BIP-352 compliant)
- ECDH shared secret computation
- Output derivation with label support
- Prefix generation for privacy-preserving queries
- No external dependencies on server/client logic

**Public API**:
- `ScanKey`: Scanning key pair (secret + public)
- `SpendKey`: Spending key (public for scanning)
- `InputData`: Transaction input metadata
- `ScanResult`: Detected payment information
- `compute_prefixes()`: Generate query prefixes
- `prefix_from_xonly()`: Extract 4-byte prefix

### whisper-server
**Purpose**: Bitcoin block indexer and REST API server

**Components**:

1. **Block Indexer** (`indexer.rs`)
   - Connects to Bitcoin Core via ZMQ
   - Parses raw blocks in real-time
   - Extracts Taproot outputs (0x5120 + 32 bytes)
   - Computes 4-byte prefixes
   - Stores in PostgreSQL

2. **REST API** (`api.rs`)
   - `POST /api/v1/scan`: Query outputs by prefix
   - `GET /api/v1/status`: Server health check
   - Rate limiting and validation
   - CORS support

3. **Database Schema** (`migrations/`)
   - `blocks`: Block headers and reorg tracking
   - `transactions`: Full transaction data
   - `taproot_outputs`: Indexed SP candidates
   - Optimized indexes for prefix queries

### whisper-client
**Purpose**: Client library for scanning Silent Payments

**Key Features**:
- HTTP client for server queries
- Local verification of candidates
- Multi-label support
- Async/await API

**Usage Flow**:
1. Create `SilentPaymentClient` with keys
2. Provide transaction inputs
3. Client computes prefixes locally
4. Query server for candidates
5. Verify matches using ECDH
6. Return confirmed payments

## Data Flow

### Indexing (Server-side)
```
Bitcoin Core (ZMQ)
    ↓ rawblock
Block Parser
    ↓ extract Taproot outputs
Prefix Computation
    ↓ first 4 bytes of x-only pubkey
PostgreSQL
    ↓ indexed by (sp_prefix, block_height)
Ready for queries
```

### Scanning (Client-side)
```
Client has: scan_secret, spend_pubkey, tx_inputs
    ↓
Compute shared_secret = ECDH(scan_secret, inputs)
    ↓
Derive expected outputs (with labels)
    ↓
Extract 4-byte prefixes
    ↓
Query server: POST /scan with prefixes
    ↓
Server returns candidates
    ↓
Client verifies each candidate locally
    ↓
Return confirmed payments
```

## Privacy Model

**Server knows**:
- All Taproot outputs (public blockchain data)
- Which prefixes client queries (4-byte = 2^32 anonymity set)
- Block ranges of interest

**Server does NOT know**:
- Client's scan_secret or spend_secret
- Which outputs actually belong to client
- Client's full public keys (only sees prefixes)

**Mitigation**:
- Use Tor/VPN for queries
- Query multiple prefixes (including decoys)
- Batch queries across time

## Security Considerations

### Cryptographic
- ✅ BIP-352 compliant tagged hashes
- ✅ Proper ECDH implementation
- ✅ X-only pubkey handling (even-Y assumption)
- ✅ Scalar arithmetic for tweak computation
- ⚠️ Test vectors required for validation

### Server
- ✅ SQL injection prevention (sqlx compile-time checks)
- ✅ Rate limiting (configurable)
- ✅ Input validation (block range, prefix count)
- ⚠️ DoS protection (proof-of-work optional)
- ⚠️ Reorg handling (manual currently)

### Database
- ✅ Foreign key constraints
- ✅ Unique constraints on (txid, vout)
- ✅ Orphan block tracking
- ⚠️ Encryption at rest (optional)

## Performance Characteristics

### Indexing
- **Throughput**: 1 block/second (Bitcoin avg: 1/10min)
- **Latency**: Real-time via ZMQ
- **Storage**: ~100 bytes per Taproot output
- **Scaling**: Vertical (single writer)

### Queries
- **Latency**: p95 < 100ms for 1000 blocks
- **Throughput**: Limited by rate limiting
- **Index**: B-tree on (sp_prefix, block_height)
- **Scaling**: Horizontal (read replicas)

### Client
- **Computation**: O(n) where n = number of labels
- **Bandwidth**: O(m) where m = matching outputs
- **Memory**: Minimal (streaming verification)

## Future Enhancements

### Phase F: Mobile FFI
- UniFFI bindings for iOS/Android
- Kotlin/Swift wrappers
- Example mobile apps

### Phase G: Reorg Handling
- Automatic fork detection
- Chain reorganization recovery
- Orphan block pruning

### Phase H: Production Features
- Prometheus metrics
- Grafana dashboards
- Automated backups
- High availability setup
- CDN for static filters

### Additional Ideas
- Compact block filters (BIP-157/158 integration)
- Mempool monitoring
- Webhook notifications
- GraphQL API
- Multi-network support (testnet, signet)
- Payment proofs
- UTXO set management

## Testing Strategy

### Unit Tests
- Core cryptographic functions
- Tagged hash correctness
- Key derivation
- Prefix extraction

### Integration Tests
- Full scan flow (regtest)
- Database operations
- API endpoints
- ZMQ ingestion

### Property Tests
- Random key generation
- False positive rate (1/2^32)
- Reorg scenarios

### Performance Tests
- Query latency benchmarks
- Indexing throughput
- Database size growth

## Deployment Checklist

- [ ] Build release binaries
- [ ] Setup PostgreSQL with backups
- [ ] Configure Bitcoin Core (mainnet)
- [ ] Enable SSL/TLS (nginx/caddy)
- [ ] Setup monitoring (logs, metrics)
- [ ] Configure firewall
- [ ] Test disaster recovery
- [ ] Document runbooks
- [ ] Setup alerting
- [ ] Load testing

## Contributing Guidelines

1. **Code Style**: Run `cargo fmt` before committing
2. **Linting**: Fix all `cargo clippy` warnings
3. **Tests**: Add tests for new features
4. **Documentation**: Update relevant .md files
5. **Commits**: Use conventional commits format
6. **PRs**: Include description and test results

## License

MIT License - See LICENSE file for details
