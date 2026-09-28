#!/usr/bin/env bash
# Shared macOS code-signing helpers. Source this file; do not execute it.
#
# Signing identity, in order of preference:
#   1. REVFLY_SIGN_IDENTITY (certificate name or SHA-1), optionally with REVFLY_SIGN_KEYCHAIN.
#   2. The "RevFly Self-Signed" certificate from scripts/create_macos_signing_cert.sh, if installed.
#   3. Ad-hoc ("-"): builds still work, but macOS asks for Microphone/Accessibility again after every update.
#
# A stable identity (1 or 2) keeps permissions across updates because macOS pins them to the certificate.
# It does not remove the Gatekeeper warning; only an Apple Developer ID certificate does that.

REVFLY_SELF_SIGNED_NAME="RevFly Self-Signed"
REVFLY_BUNDLE_ID="com.revfly.desktop"

revfly_resolve_identity() {
  if [ -n "${REVFLY_SIGN_IDENTITY:-}" ]; then
    echo "$REVFLY_SIGN_IDENTITY"
    return
  fi
  local sha
  # No -v: a self-signed certificate is "not trusted", and -v would hide it.
  sha="$(security find-identity -p codesigning ${REVFLY_SIGN_KEYCHAIN:+"$REVFLY_SIGN_KEYCHAIN"} 2>/dev/null \
    | awk -v name="\"$REVFLY_SELF_SIGNED_NAME\"" 'index($0, name) {print $2; exit}')"
  echo "${sha:--}"
}

REVFLY_IDENTITY="$(revfly_resolve_identity)"

revfly_codesign() {
  codesign --force --sign "$REVFLY_IDENTITY" ${REVFLY_SIGN_KEYCHAIN:+--keychain "$REVFLY_SIGN_KEYCHAIN"} "$@"
}

revfly_print_identity() {
  if [ "$REVFLY_IDENTITY" = "-" ]; then
    echo "--> Signing ad-hoc (no stable identity found; permissions will reset on update)."
    echo "    Run scripts/create_macos_signing_cert.sh once to fix this."
  else
    echo "--> Signing with stable identity: $REVFLY_IDENTITY"
  fi
}

# Signs bundled dylibs first, then the app bundle itself.
revfly_sign_app() {
  local app="$1"
  [ -d "$app" ] || return 0
  xattr -cr "$app" || true

  if [ -d "$app/Contents/Frameworks" ]; then
    for f in "$app/Contents/Frameworks"/*.dylib; do
      if [ -f "$f" ]; then
        revfly_codesign "$f"
      fi
    done
  fi

  if [ "$REVFLY_IDENTITY" = "-" ]; then
    # Ad-hoc signatures change every build; an identifier-only requirement is the best that is possible.
    revfly_codesign --deep --identifier "$REVFLY_BUNDLE_ID" \
      -r="designated => identifier \"$REVFLY_BUNDLE_ID\"" "$app"
  else
    # Default designated requirement pins the certificate, so permissions survive updates.
    revfly_codesign --deep --identifier "$REVFLY_BUNDLE_ID" "$app"
  fi
}

revfly_sign_dmg() {
  local dmg="$1"
  xattr -cr "$dmg" || true
  revfly_codesign "$dmg"
}
