#!/bin/bash
set -e

cd /mnt/c/Users/info/repos/bisque-io/bop

echo "Running preemption tests 100 times..."
PASS_COUNT=0
FAIL_COUNT=0

for i in $(seq 1 100); do
    echo "=== Run $i/100 ==="
    if cargo test -p maniac-runtime --lib runtime::preemption_tests -- --test-threads=1 2>&1 | tee /tmp/test_output_$i.log | tail -5; then
        PASS_COUNT=$((PASS_COUNT + 1))
        echo "Run $i: PASS"
    else
        FAIL_COUNT=$((FAIL_COUNT + 1))
        echo "Run $i: FAIL - Check /tmp/test_output_$i.log for details"
        echo "First failure occurred on run $i"
        exit 1
    fi
done

echo ""
echo "=========================================="
echo "SUCCESS: All 100 runs passed!"
echo "Pass: $PASS_COUNT, Fail: $FAIL_COUNT"
echo "=========================================="

