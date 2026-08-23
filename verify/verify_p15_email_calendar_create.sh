#!/bin/bash

set -e

echo "P15-1 Calendar Create"

if test -f src-tauri/src/email_calendar.rs; then
    echo "✓ calendar module exists"
else
    echo "FAIL calendar module missing"
    exit 1
fi


if rg -q "create_native_calendar_event" src-tauri/src/lib.rs; then
    echo "✓ create command registered"
else
    echo "FAIL create command missing"
    exit 1
fi


echo "PASS P15-1 Calendar Create Foundation"
