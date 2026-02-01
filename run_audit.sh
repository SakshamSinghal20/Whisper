#!/bin/bash

# WHISPER COMPREHENSIVE AUDIT RUNNER
# Executes all audit checks and generates report

set -e

AUDIT_DIR="audit_results_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$AUDIT_DIR"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║   WHISPER SILENT PAYMENTS INDEXER - AUDIT PROTOCOL        ║"
echo "║   Complete Verification for Production Readiness           ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Check prerequisites
echo "Checking prerequisites..."
command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found"; exit 1; }
command -v psql >/dev/null 2>&1 || { echo "✗ psql not found"; exit 1; }
echo "✓ Prerequisites OK"
echo ""

# SECTION 1: CRYPTOGRAPHIC VERIFICATION
echo "═══════════════════════════════════════════════════════════"
echo "SECTION 1: CRYPTOGRAPHIC VERIFICATION (CRITICAL)"
echo "═══════════════════════════════════════════════════════════"
echo ""

echo "Running BIP-352 compliance tests..."
cargo test -p whisper-core --release -- --nocapture 2>&1 | tee "$AUDIT_DIR/section1_crypto.txt"

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo "✓ SECTION 1: PASS" | tee -a "$AUDIT_DIR/summary.txt"
else
    echo "✗ SECTION 1: FAIL - BLOCKER FOR MAINNET" | tee -a "$AUDIT_DIR/summary.txt"
fi

echo ""

# SECTION 2: DATABASE INTEGRITY
echo "═══════════════════════════════════════════════════════════"
echo "SECTION 2: DATABASE INTEGRITY"
echo "═══════════════════════════════════════════════════════════"
echo ""

if [ -n "$DATABASE_URL" ]; then
    echo "Running database audit..."
    psql "$DATABASE_URL" -f whisper-server/audit_database.sql 2>&1 | tee "$AUDIT_DIR/section2_database.txt"
    
    # Check for failures
    if grep -q "MUST BE 0" "$AUDIT_DIR/section2_database.txt"; then
        INVALID_COUNT=$(grep "MUST BE 0" "$AUDIT_DIR/section2_database.txt" | grep -v "| 0 |" | wc -l)
        if [ "$INVALID_COUNT" -eq 0 ]; then
            echo "✓ SECTION 2: PASS" | tee -a "$AUDIT_DIR/summary.txt"
        else
            echo "✗ SECTION 2: FAIL - $INVALID_COUNT integrity checks failed" | tee -a "$AUDIT_DIR/summary.txt"
        fi
    else
        echo "⚠ SECTION 2: INCOMPLETE - Database not populated" | tee -a "$AUDIT_DIR/summary.txt"
    fi
else
    echo "⚠ DATABASE_URL not set, skipping database audit" | tee -a "$AUDIT_DIR/summary.txt"
fi

echo ""

# SECTION 3: API SECURITY
echo "═══════════════════════════════════════════════════════════"
echo "SECTION 3: API FUNCTIONALITY & SECURITY"
echo "═══════════════════════════════════════════════════════════"
echo ""

if curl -s --max-time 2 http://localhost:3000/api/v1/status >/dev/null 2>&1; then
    echo "Server detected, running API audit..."
    bash audit_api.sh 2>&1 | tee "$AUDIT_DIR/section3_api.txt"
    
    FAIL_COUNT=$(grep -c "✗ FAIL" "$AUDIT_DIR/section3_api.txt" || true)
    if [ "$FAIL_COUNT" -eq 0 ]; then
        echo "✓ SECTION 3: PASS" | tee -a "$AUDIT_DIR/summary.txt"
    else
        echo "✗ SECTION 3: FAIL - $FAIL_COUNT API checks failed" | tee -a "$AUDIT_DIR/summary.txt"
    fi
else
    echo "⚠ Server not running at localhost:3000, skipping API audit" | tee -a "$AUDIT_DIR/summary.txt"
fi

echo ""

# SECTION 4: CLIENT VERIFICATION
echo "═══════════════════════════════════════════════════════════"
echo "SECTION 4: CLIENT VERIFICATION LOGIC"
echo "═══════════════════════════════════════════════════════════"
echo ""

echo "Running client tests..."
cargo test -p whisper-client --release 2>&1 | tee "$AUDIT_DIR/section4_client.txt"

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo "✓ SECTION 4: PASS" | tee -a "$AUDIT_DIR/summary.txt"
else
    echo "✗ SECTION 4: FAIL" | tee -a "$AUDIT_DIR/summary.txt"
fi

echo ""

# SECTION 5: INTEGRATION TESTS
echo "═══════════════════════════════════════════════════════════"
echo "SECTION 5: INTEGRATION & SYNCHRONIZATION"
echo "═══════════════════════════════════════════════════════════"
echo ""

echo "Running integration tests..."
cargo test --workspace --release 2>&1 | tee "$AUDIT_DIR/section5_integration.txt"

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo "✓ SECTION 5: PASS" | tee -a "$AUDIT_DIR/summary.txt"
else
    echo "✗ SECTION 5: FAIL" | tee -a "$AUDIT_DIR/summary.txt"
fi

echo ""

# SECTION 6: PERFORMANCE BENCHMARKS
echo "═══════════════════════════════════════════════════════════"
echo "SECTION 6: PERFORMANCE BENCHMARKS"
echo "═══════════════════════════════════════════════════════════"
echo ""

echo "Building release binaries..."
cargo build --release 2>&1 | tee "$AUDIT_DIR/section6_build.txt"

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo "✓ Release build successful" | tee -a "$AUDIT_DIR/summary.txt"
    
    # Binary size check
    SERVER_SIZE=$(du -h target/release/whisper-server 2>/dev/null | cut -f1 || echo "N/A")
    echo "Server binary size: $SERVER_SIZE" | tee -a "$AUDIT_DIR/summary.txt"
else
    echo "✗ Release build failed" | tee -a "$AUDIT_DIR/summary.txt"
fi

echo ""

# SECTION 8: SECURITY AUDIT
echo "═══════════════════════════════════════════════════════════"
echo "SECTION 8: SECURITY AUDIT CHECKLIST"
echo "═══════════════════════════════════════════════════════════"
echo ""

echo "Checking for security issues..."

# Check for println! with secrets
echo "Checking for debug prints..." | tee -a "$AUDIT_DIR/section8_security.txt"
SECRET_PRINTS=$(grep -r "println.*secret" whisper-*/src/ || true)
if [ -z "$SECRET_PRINTS" ]; then
    echo "✓ No secret key debug prints found" | tee -a "$AUDIT_DIR/section8_security.txt"
else
    echo "⚠ WARNING: Found potential secret key prints:" | tee -a "$AUDIT_DIR/section8_security.txt"
    echo "$SECRET_PRINTS" | tee -a "$AUDIT_DIR/section8_security.txt"
fi

# Check for hardcoded credentials
echo "Checking for hardcoded credentials..." | tee -a "$AUDIT_DIR/section8_security.txt"
HARDCODED=$(grep -r "password.*=.*\"" whisper-*/src/ | grep -v "example" || true)
if [ -z "$HARDCODED" ]; then
    echo "✓ No hardcoded credentials found" | tee -a "$AUDIT_DIR/section8_security.txt"
else
    echo "⚠ WARNING: Potential hardcoded credentials:" | tee -a "$AUDIT_DIR/section8_security.txt"
    echo "$HARDCODED" | tee -a "$AUDIT_DIR/section8_security.txt"
fi

# Run clippy for security lints
echo "Running clippy security checks..." | tee -a "$AUDIT_DIR/section8_security.txt"
cargo clippy --all -- -D warnings 2>&1 | tee -a "$AUDIT_DIR/section8_clippy.txt"

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo "✓ SECTION 8: PASS - No clippy warnings" | tee -a "$AUDIT_DIR/summary.txt"
else
    echo "⚠ SECTION 8: WARNINGS - Review clippy output" | tee -a "$AUDIT_DIR/summary.txt"
fi

echo ""

# FINAL SUMMARY
echo "═══════════════════════════════════════════════════════════"
echo "AUDIT SUMMARY"
echo "═══════════════════════════════════════════════════════════"
echo ""

cat "$AUDIT_DIR/summary.txt"

echo ""
echo "Detailed results saved to: $AUDIT_DIR/"
echo ""

# Count failures
FAIL_COUNT=$(grep -c "✗.*FAIL" "$AUDIT_DIR/summary.txt" || true)
WARN_COUNT=$(grep -c "⚠" "$AUDIT_DIR/summary.txt" || true)

if [ "$FAIL_COUNT" -eq 0 ]; then
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║  ✓ AUDIT PASSED - Ready for further testing               ║"
    if [ "$WARN_COUNT" -gt 0 ]; then
        echo "║  ⚠ $WARN_COUNT warnings - review before mainnet deployment    ║"
    fi
    echo "╚════════════════════════════════════════════════════════════╝"
    exit 0
else
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║  ✗ AUDIT FAILED - $FAIL_COUNT critical issues found           ║"
    echo "║  DO NOT DEPLOY TO MAINNET                                  ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    exit 1
fi
