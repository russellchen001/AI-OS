#!/usr/bin/env bash
set -u

if [ -d "/Applications/Keynote.app" ]; then
  echo "SKIP Presentation real E2E: NATIVE_AUTOMATION_PERMISSION_REQUIRED"
else
  echo "SKIP Presentation real E2E: KEYNOTE_APP_NOT_INSTALLED_AND_GOOGLE_OAUTH_REQUIRED"
fi
exit 0
