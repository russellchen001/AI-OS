#!/bin/bash

set -e

echo "P15-1 Email Calendar Read"

if test -f src-tauri/src/email_calendar.rs; then
    echo "✓ native module exists"
else
    echo "FAIL native module missing"
    exit 1
fi

if rg -q "list_native_mail" src-tauri/src/lib.rs; then
    echo "✓ mail command registered"
else
    echo "FAIL mail command missing"
    exit 1
fi

if rg -q "list_native_calendar" src-tauri/src/lib.rs; then
    echo "✓ calendar command registered"
else
    echo "FAIL calendar command missing"
    exit 1
fi

echo "PASS P15-1 Email Calendar Read Foundation"
