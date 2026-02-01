-- SECTION 2: DATABASE INTEGRITY AUDIT
-- Run this script against your PostgreSQL database

\echo '=== SECTION 2.1: Schema Compliance ==='

-- Check table structure
SELECT 
    column_name, 
    data_type, 
    is_nullable,
    character_maximum_length
FROM information_schema.columns 
WHERE table_name = 'taproot_outputs'
ORDER BY ordinal_position;

-- Verify constraints
SELECT
    tc.constraint_name,
    tc.constraint_type,
    kcu.column_name
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu 
    ON tc.constraint_name = kcu.constraint_name
WHERE tc.table_name = 'taproot_outputs';

\echo ''
\echo '=== SECTION 2.2: Index Performance ==='

-- List all indexes
SELECT
    indexname,
    indexdef
FROM pg_indexes
WHERE tablename = 'taproot_outputs';

-- Test query performance (requires data)
EXPLAIN ANALYZE 
SELECT * FROM taproot_outputs 
WHERE sp_prefix = 123456789 
AND block_height BETWEEN 100 AND 200;

\echo ''
\echo '=== SECTION 2.3: Data Integrity Checks ==='

-- Verify all script_pubkeys start with 0x5120
SELECT 
    COUNT(*) as invalid_scripts,
    'MUST BE 0' as expected
FROM taproot_outputs 
WHERE get_byte(script_pubkey, 0) != 81 
   OR get_byte(script_pubkey, 1) != 32;

-- Verify script_pubkey length
SELECT 
    COUNT(*) as invalid_length,
    'MUST BE 0' as expected
FROM taproot_outputs 
WHERE length(script_pubkey) != 34;

-- Verify x_only_pubkey length
SELECT 
    COUNT(*) as invalid_xonly_length,
    'MUST BE 0' as expected
FROM taproot_outputs 
WHERE length(x_only_pubkey) != 32;

-- Verify prefix extraction correctness (sample 100 rows)
SELECT 
    COUNT(*) as mismatched_prefixes,
    'MUST BE 0' as expected
FROM (
    SELECT 
        sp_prefix,
        (get_byte(script_pubkey, 2)::bigint << 24) |
        (get_byte(script_pubkey, 3)::bigint << 16) |
        (get_byte(script_pubkey, 4)::bigint << 8) |
        get_byte(script_pubkey, 5)::bigint as calculated_prefix
    FROM taproot_outputs 
    LIMIT 100
) AS prefix_check
WHERE sp_prefix != calculated_prefix;

\echo ''
\echo '=== SECTION 2.4: Orphan Handling ==='

-- Count orphaned blocks
SELECT 
    COUNT(*) as orphaned_blocks
FROM blocks 
WHERE is_orphaned = TRUE;

-- Verify orphaned outputs are excluded from queries
SELECT 
    COUNT(*) as outputs_in_orphaned_blocks,
    'Should be excluded from normal queries' as note
FROM taproot_outputs o
JOIN blocks b ON b.height = o.block_height
WHERE b.is_orphaned = TRUE;

\echo ''
\echo '=== Database Size Analysis ==='

SELECT 
    pg_size_pretty(pg_total_relation_size('taproot_outputs')) as total_size,
    pg_size_pretty(pg_relation_size('taproot_outputs')) as table_size,
    pg_size_pretty(pg_total_relation_size('taproot_outputs') - pg_relation_size('taproot_outputs')) as indexes_size;

SELECT 
    COUNT(*) as total_outputs,
    MIN(block_height) as min_height,
    MAX(block_height) as max_height
FROM taproot_outputs;

\echo ''
\echo '=== Foreign Key Integrity ==='

-- Check for orphaned transactions (no matching block)
SELECT 
    COUNT(*) as orphaned_transactions,
    'MUST BE 0' as expected
FROM transactions t
LEFT JOIN blocks b ON b.height = t.block_height
WHERE b.height IS NULL;

-- Check for orphaned outputs (no matching transaction)
SELECT 
    COUNT(*) as orphaned_outputs,
    'MUST BE 0' as expected
FROM taproot_outputs o
LEFT JOIN transactions t ON t.txid = o.txid
WHERE t.txid IS NULL;

\echo ''
\echo '=== AUDIT COMPLETE ==='
\echo 'Review all "MUST BE 0" checks above'
