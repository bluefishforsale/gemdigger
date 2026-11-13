#!/usr/bin/env bash
# Hosted runners are thrown away after the job, but the keychain and the
# notarization key go anyway, so a self-hosted Mac could run this safely too.
set -euo pipefail

keychain="$RUNNER_TEMP/release.keychain-db"
if [[ -e "$keychain" ]]; then
  security delete-keychain "$keychain"
fi
rm -f "$RUNNER_TEMP/developer-id.p12" "$RUNNER_TEMP/notarization-key.p8"
