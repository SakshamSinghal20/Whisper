-- Initial schema for Whisper Silent Payments indexer

CREATE TABLE blocks (
    height INTEGER PRIMARY KEY,
    hash BYTEA NOT NULL UNIQUE CHECK (length(hash) = 32),
    header BYTEA NOT NULL CHECK (length(header) = 80),
    is_orphaned BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE transactions (
    txid BYTEA PRIMARY KEY CHECK (length(txid) = 32),
    block_height INTEGER NOT NULL REFERENCES blocks(height) ON DELETE CASCADE,
    block_index INTEGER NOT NULL,
    is_coinbase BOOLEAN DEFAULT FALSE,
    raw_tx BYTEA NOT NULL
);

CREATE TABLE taproot_outputs (
    id BIGSERIAL PRIMARY KEY,
    txid BYTEA NOT NULL REFERENCES transactions(txid) ON DELETE CASCADE,
    vout INTEGER NOT NULL,
    block_height INTEGER NOT NULL REFERENCES blocks(height) ON DELETE CASCADE,
    script_pubkey BYTEA NOT NULL CHECK (length(script_pubkey) = 34),
    amount BIGINT NOT NULL,
    x_only_pubkey BYTEA NOT NULL CHECK (length(x_only_pubkey) = 32),
    sp_prefix INTEGER NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(txid, vout)
);

CREATE INDEX idx_outputs_prefix_height ON taproot_outputs(sp_prefix, block_height);
CREATE INDEX idx_outputs_height ON taproot_outputs(block_height);
CREATE INDEX idx_outputs_lookup ON taproot_outputs(txid, vout);
CREATE INDEX idx_blocks_orphaned ON blocks(height) WHERE is_orphaned = TRUE;
