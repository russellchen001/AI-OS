#!/bin/bash

set -e

echo "P15-1 Email Calendar Permission Detection"

if test -f src-tauri/src/macos_permissions.rs; then
    echo "✓ permission module exists"
else
    echo "FAIL permission module missing"
    exit 1
fi

if rg -q "check_macos_mail_calendar_permissions" src-tauri/src/lib.rs; then
    echo "✓ command registered"
else
    echo "FAIL command registration missing"
    exit 1
fi

echo "✓ permission foundation ready"
echo "PASS P15-1 Permission Detection"
