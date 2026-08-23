#!/bin/bash

set -e

echo "P15-1 Mail Send Confirmation"

if rg -q "prepare_native_mail_send_confirmation" src-tauri/src/email_calendar.rs; then
    echo "✓ confirmation preparation exists"
else
    echo "FAIL confirmation preparation missing"
    exit 1
fi

if rg -q "send_native_mail_after_confirmation" src-tauri/src/email_calendar.rs; then
    echo "✓ confirmed send command exists"
else
    echo "FAIL send command missing"
    exit 1
fi

if rg -q "send_native_mail_after_confirmation" src-tauri/src/lib.rs; then
    echo "✓ send command registered"
else
    echo "FAIL command registration missing"
    exit 1
fi

echo "PASS P15-1 Mail Send Confirmation"
