#!/bin/bash

set -e

echo "P15-2 Document Skill Foundation"

if grep -q "document::list_document_capabilities" src-tauri/src/lib.rs; then
    echo "✓ document command registered"
else
    echo "FAIL document command missing"
    exit 1
fi

if grep -q "document.read" src-tauri/src/runtime/skills/registry.rs; then
    echo "✓ document skill capability exists"
else
    echo "FAIL document capability missing"
    exit 1
fi

if test -f src-tauri/src/document.rs; then
    echo "✓ document module exists"
else
    echo "FAIL document module missing"
    exit 1
fi

echo "PASS P15-2 Document Skill Foundation"
