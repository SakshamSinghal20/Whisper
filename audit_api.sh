#!/bin/bash

# SECTION 3: API FUNCTIONALITY & SECURITY AUDIT
# Run this script against a running Whisper server

set -e

SERVER_URL="${WHISPER_SERVER:-http://localhost:3000}"
RESULTS_FILE="api_audit_results.txt"

echo "=== WHISPER API SECURITY AUDIT ===" | tee $RESULTS_FILE
echo "Server: $SERVER_URL" | tee -a $RESULTS_FILE
echo "Date: $(date)" | tee -a $RESULTS_FILE
echo "" | tee -a $RESULTS_FILE

# SECTION 3.1: Endpoint Testing
echo "=== SECTION 3.1: Endpoint Testing ===" | tee -a $RESULTS_FILE

# Test status endpoint
echo "Testing GET /api/v1/status..." | tee -a $RESULTS_FILE
STATUS_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" "$SERVER_URL/api/v1/status")
STATUS_CODE=$(echo "$STATUS_RESPONSE" | grep "HTTP_CODE" | cut -d: -f2)

if [ "$STATUS_CODE" = "200" ]; then
    echo "✓ Status endpoint returns 200" | tee -a $RESULTS_FILE
else
    echo "✗ FAIL: Status endpoint returned $STATUS_CODE" | tee -a $RESULTS_FILE
fi

# Test valid scan request
echo "" | tee -a $RESULTS_FILE
echo "Testing POST /api/v1/scan with valid request..." | tee -a $RESULTS_FILE
SCAN_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" -X POST "$SERVER_URL/api/v1/scan" \
  -H "Content-Type: application/json" \
  -d '{
    "scan_pubkey": "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    "start_height": 100,
    "end_height": 200,
    "prefixes": ["a1b2c3d4", "00000000"]
  }')

SCAN_CODE=$(echo "$SCAN_RESPONSE" | grep "HTTP_CODE" | cut -d: -f2)
if [ "$SCAN_CODE" = "200" ]; then
    echo "✓ Scan endpoint returns 200 for valid request" | tee -a $RESULTS_FILE
else
    echo "✗ FAIL: Scan endpoint returned $SCAN_CODE" | tee -a $RESULTS_FILE
fi

# Test invalid hex
echo "" | tee -a $RESULTS_FILE
echo "Testing invalid hex prefix..." | tee -a $RESULTS_FILE
INVALID_HEX=$(curl -s -w "%{http_code}" -X POST "$SERVER_URL/api/v1/scan" \
  -H "Content-Type: application/json" \
  -d '{
    "scan_pubkey": "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    "start_height": 100,
    "end_height": 200,
    "prefixes": ["INVALID_HEX"]
  }')

if [ "$INVALID_HEX" = "400" ]; then
    echo "✓ Returns 400 for invalid hex" | tee -a $RESULTS_FILE
else
    echo "✗ FAIL: Expected 400, got $INVALID_HEX" | tee -a $RESULTS_FILE
fi

# Test block range too large
echo "" | tee -a $RESULTS_FILE
echo "Testing block range > 1000..." | tee -a $RESULTS_FILE
LARGE_RANGE=$(curl -s -w "%{http_code}" -X POST "$SERVER_URL/api/v1/scan" \
  -H "Content-Type: application/json" \
  -d '{
    "scan_pubkey": "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    "start_height": 100,
    "end_height": 2000,
    "prefixes": ["a1b2c3d4"]
  }')

if [ "$LARGE_RANGE" = "400" ]; then
    echo "✓ Returns 400 for block range > 1000" | tee -a $RESULTS_FILE
else
    echo "✗ FAIL: Expected 400, got $LARGE_RANGE" | tee -a $RESULTS_FILE
fi

# Test too many prefixes
echo "" | tee -a $RESULTS_FILE
echo "Testing > 1000 prefixes..." | tee -a $RESULTS_FILE

# Generate 1001 prefixes
PREFIXES=$(for i in {1..1001}; do echo -n "\"$(printf '%08x' $i)\","; done | sed 's/,$//')
TOO_MANY=$(curl -s -w "%{http_code}" -X POST "$SERVER_URL/api/v1/scan" \
  -H "Content-Type: application/json" \
  -d "{
    \"scan_pubkey\": \"0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798\",
    \"start_height\": 100,
    \"end_height\": 200,
    \"prefixes\": [$PREFIXES]
  }")

if [ "$TOO_MANY" = "400" ]; then
    echo "✓ Returns 400 for > 1000 prefixes" | tee -a $RESULTS_FILE
else
    echo "✗ FAIL: Expected 400, got $TOO_MANY" | tee -a $RESULTS_FILE
fi

# SECTION 3.2: Privacy Leakage Audit
echo "" | tee -a $RESULTS_FILE
echo "=== SECTION 3.2: Privacy Leakage Audit ===" | tee -a $RESULTS_FILE

# Check if server logs contain sensitive data (requires log access)
if [ -f "/var/log/whisper/server.log" ]; then
    echo "Checking server logs for key leakage..." | tee -a $RESULTS_FILE
    
    # Look for 64-character hex strings (potential private keys)
    KEY_LEAKS=$(grep -E '[0-9a-f]{64}' /var/log/whisper/server.log | wc -l)
    
    if [ "$KEY_LEAKS" -eq 0 ]; then
        echo "✓ No 64-char hex strings in logs (no key leakage)" | tee -a $RESULTS_FILE
    else
        echo "⚠ WARNING: Found $KEY_LEAKS potential key leaks in logs" | tee -a $RESULTS_FILE
    fi
else
    echo "⚠ Log file not accessible, skipping privacy audit" | tee -a $RESULTS_FILE
fi

# SECTION 3.3: CORS & Security Headers
echo "" | tee -a $RESULTS_FILE
echo "=== SECTION 3.3: Security Headers ===" | tee -a $RESULTS_FILE

HEADERS=$(curl -sI "$SERVER_URL/api/v1/status")

echo "Response headers:" | tee -a $RESULTS_FILE
echo "$HEADERS" | tee -a $RESULTS_FILE

if echo "$HEADERS" | grep -qi "access-control-allow-origin"; then
    echo "✓ CORS headers present" | tee -a $RESULTS_FILE
else
    echo "⚠ No CORS headers found" | tee -a $RESULTS_FILE
fi

# Rate limiting test (optional - requires many requests)
echo "" | tee -a $RESULTS_FILE
echo "=== Rate Limiting Test (100 requests) ===" | tee -a $RESULTS_FILE
echo "This may take a minute..." | tee -a $RESULTS_FILE

SUCCESS_COUNT=0
RATE_LIMITED=0

for i in {1..100}; do
    CODE=$(curl -s -w "%{http_code}" -o /dev/null "$SERVER_URL/api/v1/status")
    if [ "$CODE" = "200" ]; then
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    elif [ "$CODE" = "429" ]; then
        RATE_LIMITED=$((RATE_LIMITED + 1))
    fi
done

echo "Successful requests: $SUCCESS_COUNT" | tee -a $RESULTS_FILE
echo "Rate limited (429): $RATE_LIMITED" | tee -a $RESULTS_FILE

if [ $RATE_LIMITED -gt 0 ]; then
    echo "✓ Rate limiting is active" | tee -a $RESULTS_FILE
else
    echo "⚠ No rate limiting detected (may not be configured)" | tee -a $RESULTS_FILE
fi

echo "" | tee -a $RESULTS_FILE
echo "=== API AUDIT COMPLETE ===" | tee -a $RESULTS_FILE
echo "Results saved to: $RESULTS_FILE" | tee -a $RESULTS_FILE
