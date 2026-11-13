#!/usr/bin/env bash
# Release gate: a tag build with any signing input missing fails here, before
# anything is built, signed or published, so nothing unsigned ever ships. It
# checks presence only and never prints a value.
set -euo pipefail

# NOTE: release.yml must map each of these into this step's env; a name it
# forgets to map reads as missing, which fails closed.
required=(
  APPLE_CERTIFICATE
  APPLE_CERTIFICATE_PASSWORD
  APPLE_SIGNING_IDENTITY
  APPLE_TEAM_ID
  APPLE_API_ISSUER
  APPLE_API_KEY
  APPLE_API_PRIVATE_KEY
  KEYCHAIN_PASSWORD
  AZURE_TENANT_ID
  AZURE_CLIENT_ID
  AZURE_SIGNING_ENDPOINT
  AZURE_SIGNING_ACCOUNT
  AZURE_CERT_PROFILE
)

missing=()
for name in "${required[@]}"; do
  [[ -n "${!name:-}" ]] || missing+=("$name")
done

if ((${#missing[@]} == 0)); then
  echo "All ${#required[@]} signing inputs are present in the release environment."
  exit 0
fi

{
  echo "### Release refused: ${#missing[@]} of ${#required[@]} signing inputs missing"
  echo
  printf -- "- \`%s\`\n" "${missing[@]}"
  echo
  echo "Add them to the \`release\` environment as described under "Signing" in README.md. Nothing was built or published."
} >>"${GITHUB_STEP_SUMMARY:-/dev/stderr}"
echo "::error title=Release refused::${#missing[@]} signing input(s) missing: ${missing[*]}"
exit 1
