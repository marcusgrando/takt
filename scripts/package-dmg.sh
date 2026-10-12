#!/bin/bash
# scripts/package-dmg.sh — Build the Release app and package it as a DMG.
#
# Usage: scripts/package-dmg.sh [expected-version]
#
# The version comes from macos/Version.xcconfig (bumped by release-please).
# When an expected version is given (CI passes the release tag), the script
# fails if it does not match, so a DMG never ships with the wrong version.
#
# The app is ad-hoc signed. Without a Developer ID signature and
# notarization, users must approve the first launch in
# System Settings > Privacy & Security.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_DIR="$REPO_ROOT/build"
APP_NAME="Takt"

VERSION="$(sed -nE 's/^MARKETING_VERSION = ([0-9A-Za-z.+-]+).*/\1/p' "$REPO_ROOT/macos/Version.xcconfig")"
if [ -z "$VERSION" ]; then
    echo "error: MARKETING_VERSION not found in macos/Version.xcconfig" >&2
    exit 1
fi

EXPECTED_VERSION="${1:-}"
EXPECTED_VERSION="${EXPECTED_VERSION#v}"
if [ -n "$EXPECTED_VERSION" ] && [ "$EXPECTED_VERSION" != "$VERSION" ]; then
    echo "error: Version.xcconfig has $VERSION but release expects $EXPECTED_VERSION" >&2
    exit 1
fi

BUILD_NUMBER="${BUILD_NUMBER:-1}"
DMG_PATH="$BUILD_DIR/$APP_NAME-$VERSION-arm64.dmg"
APP_PATH="$BUILD_DIR/Build/Products/Release/$APP_NAME.app"

echo "==> Building $APP_NAME $VERSION ($BUILD_NUMBER)..."
# Xcode collects the synchronized source folder before its build phases run,
# so the generated Swift bindings must exist before xcodebuild starts.
"$REPO_ROOT/scripts/build-rust.sh" release
xcodebuild \
    -project "$REPO_ROOT/macos/Takt.xcodeproj" \
    -scheme "$APP_NAME" \
    -configuration Release \
    -derivedDataPath "$BUILD_DIR" \
    -destination 'generic/platform=macOS' \
    CURRENT_PROJECT_VERSION="$BUILD_NUMBER" \
    CODE_SIGN_STYLE=Manual \
    CODE_SIGN_IDENTITY=- \
    DEVELOPMENT_TEAM= \
    CODE_SIGN_INJECT_BASE_ENTITLEMENTS=NO \
    build

codesign --verify --deep --strict --verbose=2 "$APP_PATH"

echo "==> Packaging $DMG_PATH..."
STAGING="$(mktemp -d "${TMPDIR:-/tmp}/takt-dmg.XXXXXX")"
trap 'rm -rf "$STAGING"' EXIT
cp -R "$APP_PATH" "$STAGING/"
ln -s /Applications "$STAGING/Applications"
rm -f "$DMG_PATH" "$DMG_PATH.sha256"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGING" -ov -format UDZO "$DMG_PATH"
hdiutil verify "$DMG_PATH"

(cd "$BUILD_DIR" && shasum -a 256 "$(basename "$DMG_PATH")" > "$(basename "$DMG_PATH").sha256")

echo "==> Done: $DMG_PATH"
