#!/usr/bin/env bash
# Shared macOS code-signing helpers. Source this file; do not execute it.
#
# Signing identity, in order of preference:
#   1. REVFLY_SIGN_IDENTITY (certificate name or SHA-1), optionally with REVFLY_SIGN_KEYCHAIN.
#   2. A "Developer ID Application" certificate, if installed: the only one Gatekeeper trusts.
#   3. The "RevFly Self-Signed" certificate from scripts/create_macos_signing_cert.sh, if installed.
#   4. Ad-hoc ("-"): builds still work, but macOS asks for Microphone/Accessibility again after every update.
#
# A stable identity (1-3) keeps permissions across updates because macOS pins them to the certificate.
# Only a Developer ID build, notarized with revfly_notarize, opens without the Gatekeeper warning.

REVFLY_SELF_SIGNED_NAME="RevFly Self-Signed"
REVFLY_BUNDLE_ID="com.revfly.desktop"
REVFLY_ENTITLEMENTS="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/src-tauri/Entitlements.plist"

revfly_resolve_identity() {
  if [ -n "${REVFLY_SIGN_IDENTITY:-}" ]; then
    echo "$REVFLY_SIGN_IDENTITY"
    return
  fi
  local identities sha
  # No -v: a self-signed certificate is "not trusted", and -v would hide it.
  identities="$(security find-identity -p codesigning ${REVFLY_SIGN_KEYCHAIN:+"$REVFLY_SIGN_KEYCHAIN"} 2>/dev/null)"
  sha="$(awk '/"Developer ID Application: / {print $2; exit}' <<<"$identities")"
  [ -n "$sha" ] || sha="$(awk -v name="\"$REVFLY_SELF_SIGNED_NAME\"" 'index($0, name) {print $2; exit}' <<<"$identities")"
  echo "${sha:--}"
}

REVFLY_IDENTITY="$(revfly_resolve_identity)"
REVFLY_DEVELOPER_ID=0
if [ "$REVFLY_IDENTITY" != "-" ] && security find-identity -p codesigning ${REVFLY_SIGN_KEYCHAIN:+"$REVFLY_SIGN_KEYCHAIN"} 2>/dev/null \
  | grep -F "$REVFLY_IDENTITY" | grep -q '"Developer ID Application: '; then
  REVFLY_DEVELOPER_ID=1
fi

revfly_codesign() {
  # Notarization requires the hardened runtime and a secure timestamp on every binary.
  local extra=()
  [ "$REVFLY_DEVELOPER_ID" = 1 ] && extra=(--options runtime --timestamp)
  codesign --force --sign "$REVFLY_IDENTITY" ${REVFLY_SIGN_KEYCHAIN:+--keychain "$REVFLY_SIGN_KEYCHAIN"} "${extra[@]}" "$@"
}

revfly_print_identity() {
  if [ "$REVFLY_IDENTITY" = "-" ]; then
    echo "--> Signing ad-hoc (no stable identity found; permissions will reset on update)."
    echo "    Run scripts/create_macos_signing_cert.sh once to fix this."
  elif [ "$REVFLY_DEVELOPER_ID" = 1 ]; then
    echo "--> Signing with Developer ID: $REVFLY_IDENTITY"
  else
    echo "--> Signing with stable identity: $REVFLY_IDENTITY"
  fi
}

# Sends a signed .app or .dmg to Apple's notary service and staples the ticket to it, so it opens
# without a Gatekeeper warning, even offline. Needs a Developer ID identity plus APPLE_ID,
# APPLE_PASSWORD (an app-specific password) and APPLE_TEAM_ID; skipped otherwise.
revfly_notarize() {
  local target="$1"
  if [ "$REVFLY_DEVELOPER_ID" != 1 ] || [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_PASSWORD:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ]; then
    echo "--> Skipping notarization of $(basename "$target") (needs Developer ID + APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID)."
    return 0
  fi
  local upload="$target"
  if [ -d "$target" ]; then
    # The notary service takes a zip, not a bare .app.
    upload="$(mktemp -d)/$(basename "$target").zip"
    ditto -c -k --keepParent "$target" "$upload"
  fi
  local auth=(--apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID")
  echo "--> Notarizing $(basename "$target") (usually a few minutes)..."
  local out
  out="$(xcrun notarytool submit "$upload" "${auth[@]}" --wait 2>&1)" || true
  echo "$out"
  if ! grep -q "status: Accepted" <<<"$out"; then
    local id
    id="$(awk '$1 == "id:" {print $2; exit}' <<<"$out")"
    [ -n "$id" ] && xcrun notarytool log "$id" "${auth[@]}"
    echo "❌ Notarization failed for $(basename "$target")" >&2
    return 1
  fi
  xcrun stapler staple "$target"
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

  revfly_sign_bundle --deep "$app"

  # The Apple Translation helper: --deep above gave it the app's identifier and no hardened
  # runtime. Sign it on its own, then seal the bundle again around it.
  local helper="$app/Contents/MacOS/revfly-translate"
  if [ -f "$helper" ]; then
    revfly_codesign --options runtime --identifier "$REVFLY_BUNDLE_ID.translate" "$helper"
    revfly_sign_bundle "$app"
  fi
}

# Signs the app bundle itself; extra arguments (e.g. --deep) go before the path.
revfly_sign_bundle() {
  if [ "$REVFLY_IDENTITY" = "-" ]; then
    # Ad-hoc signatures change every build; an identifier-only requirement is the best that is possible.
    revfly_codesign "${@:1:$#-1}" --identifier "$REVFLY_BUNDLE_ID" --entitlements "$REVFLY_ENTITLEMENTS" \
      -r="designated => identifier \"$REVFLY_BUNDLE_ID\"" "${@: -1}"
  else
    # Default designated requirement pins the certificate, so permissions survive updates.
    revfly_codesign "${@:1:$#-1}" --identifier "$REVFLY_BUNDLE_ID" --entitlements "$REVFLY_ENTITLEMENTS" "${@: -1}"
  fi
}

revfly_sign_dmg() {
  local dmg="$1"
  xattr -cr "$dmg" || true
  revfly_codesign "$dmg"
}
