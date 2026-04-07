#!/bin/bash
# scripts/build-rust.sh — Build libtakt and generate Swift bindings
set -euo pipefail

# Xcode doesn't inherit shell PATH — add cargo explicitly
export PATH="$HOME/.cargo/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="aarch64-apple-darwin"
PROFILE="${1:-release}"
LIB="$REPO_ROOT/target/$TARGET/$PROFILE/liblibtakt.a"
OUT="$REPO_ROOT/macos/Takt/Generated"

mkdir -p "$OUT" "$OUT/Headers" "$OUT/Modules" "$OUT/LibTaktFFI"

echo "==> Building libtakt ($PROFILE, $TARGET)..."
cargo build --manifest-path "$REPO_ROOT/Cargo.toml" \
    --package libtakt "--$PROFILE" --target "$TARGET"

echo "==> Generating Swift sources..."
cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
    --package libtakt --bin uniffi-bindgen-swift -- \
    "$LIB" "$OUT" --swift-sources

echo "==> Generating C headers..."
cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
    --package libtakt --bin uniffi-bindgen-swift -- \
    "$LIB" "$OUT/Headers" --headers

echo "==> Generating modulemap..."
cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
    --package libtakt --bin uniffi-bindgen-swift -- \
    "$LIB" "$OUT/Modules" --modulemap --modulemap-filename LibTaktFFI.modulemap

# Fix: Create a proper Clang module directory at Generated/LibTaktFFI/
# with module.modulemap + header. Clang discovers modules by scanning
# SWIFT_INCLUDE_PATHS for subdirectories containing module.modulemap.
cp "$OUT/Headers/LibTaktFFI.h" "$OUT/LibTaktFFI/LibTaktFFI.h"
cat > "$OUT/LibTaktFFI/module.modulemap" << 'MODULEMAP'
module LibTaktFFI {
    header "LibTaktFFI.h"
    export *
}
MODULEMAP

echo "==> Done. Output in $OUT"
ls "$OUT"/ "$OUT/Headers"/ "$OUT/Modules"/
