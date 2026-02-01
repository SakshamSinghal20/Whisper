# Whisper Setup Guide

## Table of Contents
1. [Development Setup](#development-setup)
2. [Production Deployment](#production-deployment)
3. [Testing](#testing)
4. [Troubleshooting](#troubleshooting)

## Development Setup

### Prerequisites

Install the following:
- **Rust** 1.75 or later: https://rustup.rs/
- **PostgreSQL** 15+: https://www.postgresql.org/download/
- **Bitcoin Core** 26.0+: https://bitcoin.org/en/download
- **Docker** (optional): https://docs.docker.com/get-docker/

### Option 1: Docker Compose (Recommended for Development)

```bash
# Clone repository
git clone <your-repo-url>
cd whisper

# Copy environment template
cp .env.example .env

# Start all services (PostgreSQL, Bitcoin Core, Whisper Server)
docker-compose up -d

# View logs
docker-compose logs -f whisper-server

# Check status
curl http://localhost:3000/api/v1/status
```

### Option 2: Manual Setup

#### 1. Setup PostgreSQL

```bash
# Create database
createdb whisper

# Set connection string
export DATABASE_URL="postgres://username:password@localhost/whisper"
```

#### 2. Configure Bitcoin Core

Edit `bitcoin.conf`:

```ini
# Network (use regtest for development)
regtest=1

# RPC
server=1
rpcuser=bitcoin
rpcpassword=your_secure_password
rpcallowip=127.0.0.1

# Indexing
txindex=1

# ZMQ (for real-time block notifications)
zmqpubrawblock=tcp://127.0.0.1:28332
zmqpubrawtx=tcp://127.0.0.1:28333
```

Start Bitcoin Core:
```bash
bitcoind -daemon
```

#### 3. Build Whisper

```bash
# Build all crates
cargo build --release

# Run tests
cargo test --all
```

#### 4. Setup Environment

Create `.env` file:
```bash
DATABASE_URL=postgres://username:password@localhost/whisper
BITCOIN_RPC_URL=http://localhost:18443
BITCOIN_RPC_USER=bitcoin
BITCOIN_RPC_PASS=your_secure_password
ZMQ_BLOCK_SOCKET=tcp://127.0.0.1:28332
NETWORK=regtest
HOST=0.0.0.0
PORT=3000
RUST_LOG=info
```

#### 5. Run Server

```bash
cd whisper-server
cargo run --release
```

The server will:
1. Connect to PostgreSQL
2. Run database migrations
3. Connect to Bitcoin Core via ZMQ
4. Start REST API on port 3000

## Production Deployment

### System Requirements

- **CPU**: 4+ cores
- **RAM**: 8GB minimum, 16GB recommended
- **Storage**: 500GB SSD (for mainnet with full history)
- **Network**: 100 Mbps+

### Security Checklist

- [ ] Use strong PostgreSQL passwords
- [ ] Enable SSL/TLS for API (use nginx/caddy reverse proxy)
- [ ] Configure firewall (only expose necessary ports)
- [ ] Enable rate limiting
- [ ] Use Tor for client queries (privacy)
- [ ] Regular database backups
- [ ] Monitor disk usage
- [ ] Set up log rotation

### Production Configuration

```bash
# .env for production
DATABASE_URL=postgres://whisper:STRONG_PASSWORD@localhost/whisper
BITCOIN_RPC_URL=http://localhost:8332
BITCOIN_RPC_USER=bitcoin
BITCOIN_RPC_PASS=STRONG_PASSWORD
ZMQ_BLOCK_SOCKET=tcp://127.0.0.1:28332
NETWORK=mainnet
HOST=127.0.0.1  # Bind to localhost, use reverse proxy
PORT=3000
MAX_BLOCK_RANGE=1000
MAX_PREFIXES=1000
RUST_LOG=info
```

### Nginx Reverse Proxy

```nginx
server {
    listen 443 ssl http2;
    server_name whisper.yourdomain.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location /api/ {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        
        # Rate limiting
        limit_req zone=api burst=20 nodelay;
    }
}

# Rate limit zone
limit_req_zone $binary_remote_addr zone=api:10m rate=100r/h;
```

### Systemd Service

Create `/etc/systemd/system/whisper.service`:

```ini
[Unit]
Description=Whisper Silent Payments Indexer
After=network.target postgresql.service bitcoind.service

[Service]
Type=simple
User=whisper
WorkingDirectory=/opt/whisper
EnvironmentFile=/opt/whisper/.env
ExecStart=/opt/whisper/target/release/whisper-server
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl enable whisper
sudo systemctl start whisper
sudo systemctl status whisper
```

## Testing

### Unit Tests

```bash
# Run all tests
cargo test --all

# Run specific crate tests
cargo test -p whisper-core

# Run with output
cargo test -- --nocapture
```

### Integration Tests

Requires running regtest Bitcoin Core:

```bash
# Start regtest node
bitcoind -regtest -daemon

# Generate blocks
bitcoin-cli -regtest generatetoaddress 101 <address>

# Run integration tests
cargo test --test integration
```

### Client Example

```bash
# Run example client
cd whisper-client
cargo run --example scan_example
```

## Troubleshooting

### Server won't start

**Error**: "Failed to connect to database"
```bash
# Check PostgreSQL is running
pg_isready

# Verify connection string
psql $DATABASE_URL
```

**Error**: "ZMQ connection failed"
```bash
# Check Bitcoin Core is running with ZMQ
bitcoin-cli getblockchaininfo

# Verify ZMQ endpoint in bitcoin.conf
grep zmq ~/.bitcoin/bitcoin.conf
```

### No blocks being indexed

```bash
# Check ZMQ is publishing
bitcoin-cli getzmqnotifications

# Verify server logs
docker-compose logs whisper-server

# Test ZMQ manually
zmq_sub tcp://127.0.0.1:28332 rawblock
```

### Slow queries

```bash
# Check database indexes
psql $DATABASE_URL -c "\d+ taproot_outputs"

# Analyze query performance
psql $DATABASE_URL -c "EXPLAIN ANALYZE SELECT * FROM taproot_outputs WHERE sp_prefix = 123456"

# Vacuum database
psql $DATABASE_URL -c "VACUUM ANALYZE"
```

### High memory usage

- Reduce `max_connections` in PostgreSQL
- Adjust `MAX_BLOCK_RANGE` in config
- Enable connection pooling
- Monitor with `htop` or `docker stats`

### Reorg handling

Currently, blocks are marked as `is_orphaned = TRUE` but not automatically detected. Manual reorg handling:

```sql
-- Mark orphaned blocks
UPDATE blocks SET is_orphaned = TRUE WHERE height > <fork_height>;

-- Queries automatically exclude orphaned blocks
```

## Monitoring

### Health Check Endpoint

```bash
curl http://localhost:3000/api/v1/status
```

### Database Metrics

```sql
-- Total indexed outputs
SELECT COUNT(*) FROM taproot_outputs;

-- Outputs per block
SELECT block_height, COUNT(*) 
FROM taproot_outputs 
GROUP BY block_height 
ORDER BY block_height DESC 
LIMIT 10;

-- Database size
SELECT pg_size_pretty(pg_database_size('whisper'));
```

### Logs

```bash
# Docker
docker-compose logs -f whisper-server

# Systemd
journalctl -u whisper -f

# Adjust log level
export RUST_LOG=debug
```

## Backup & Recovery

### Database Backup

```bash
# Backup
pg_dump whisper > whisper_backup_$(date +%Y%m%d).sql

# Restore
psql whisper < whisper_backup_20240101.sql
```

### Disaster Recovery

1. Stop server
2. Restore database from backup
3. Verify tip height matches Bitcoin Core
4. Restart server
5. Server will catch up from last indexed block

## Performance Tuning

### PostgreSQL

```sql
-- Increase shared buffers (25% of RAM)
ALTER SYSTEM SET shared_buffers = '2GB';

-- Increase work memory
ALTER SYSTEM SET work_mem = '64MB';

-- Reload config
SELECT pg_reload_conf();
```

### Whisper Server

- Increase `max_connections` in PgPoolOptions
- Use connection pooling
- Enable query result caching (future feature)
- Horizontal scaling with read replicas

## Support

- GitHub Issues: <your-repo-url>/issues
- Documentation: <your-docs-url>
- BIP-352 Spec: https://github.com/bitcoin/bips/blob/master/bip-0352.mediawiki
