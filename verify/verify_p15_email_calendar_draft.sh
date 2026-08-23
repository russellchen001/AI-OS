#!/bin/bash

set -e

echo "P15-1 Mail Draft Creation"

if rg -q "create_mail_draft" src-tauri/src/email_calendar.rs; then
    echo "✓ draft command exists"
else
    echo "FAIL draft command missing"
    exit 1
fi

if rg -q "email_calendar::create_mail_draft" src-tauri/src/lib.rs; then
    echo "✓ draft command registered"
else
    echo "FAIL command registration missing"
    exit 1
fi

echo "PASS P15-1 Mail Draft Foundation"
