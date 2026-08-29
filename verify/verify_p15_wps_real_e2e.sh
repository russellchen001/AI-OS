#!/usr/bin/env bash
set -u

if [ -z "${AI_OS_WPS_BROKER_URL:-}" ]; then
  echo "SKIP WPS real E2E: BACKEND_BROKER_REQUIRED"
  exit 0
fi
echo "SKIP WPS real E2E: SERVER_APPKEY_PROVISIONING_OR_USER_CONSENT_REQUIRED"
exit 0
