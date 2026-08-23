#!/bin/bash

TEST_LOG="/tmp/ac-exec-step3b2-tests.verified.log"
CHECK_LOG="/tmp/ac-exec-step3b2-check.verified.log"

echo "AC-EXEC-MODEL Step 3B-2"

if [ ! -s "$TEST_LOG" ]; then
    echo "✗ Adapter test result unavailable"
    echo "FAIL AC-EXEC-MODEL Step 3B-2: missing test result"
    exit 1
fi

if tail -20 "$TEST_LOG" | grep -q "29 passed; 0 failed"; then
    echo "✓ OpenClaw adapter behavior passed"
else
    echo "✗ OpenClaw adapter tests did not pass"
    echo "FAIL AC-EXEC-MODEL Step 3B-2: adapter tests"
    exit 1
fi

if [ ! -s "$CHECK_LOG" ]; then
    echo "✗ Rust compile result unavailable"
    echo "FAIL AC-EXEC-MODEL Step 3B-2: missing compile result"
    exit 1
fi

if tail -20 "$CHECK_LOG" | grep -q "Finished .*dev.* profile"; then
    echo "✓ Rust compile passed"
else
    echo "✗ Rust compile did not pass"
    echo "FAIL AC-EXEC-MODEL Step 3B-2: compile"
    exit 1
fi

echo "✓ Sequential AI Center execution-agent fallback is build-valid"
echo "PASS AC-EXEC-MODEL Step 3B-2"
exit 0
