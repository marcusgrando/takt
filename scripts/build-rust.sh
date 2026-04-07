#!/bin/bash
# scripts/build-rust.sh — Build libtakt and generate Swift bindings
#
# Runs on every Xcode build (alwaysOutOfDate=1) but is fast when nothing
# changed: cargo build is a no-op (~0.5s), and bindgen is skipped if the
# .a file hasn't been modified since the last generation.
set -euo pipefail

# Xcode doesn't inherit shell PATH — add cargo explicitly
export PATH="$HOME/.cargo/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="aarch64-apple-darwin"

# Resolve profile: Xcode sets CONFIGURATION, CLI passes as $1
PROFILE="${1:-release}"
[ "${CONFIGURATION:-}" = "Debug" ] && PROFILE="debug"

# cargo uses --release flag; debug is the default (no flag)
CARGO_ARGS=(--manifest-path "$REPO_ROOT/Cargo.toml" --package libtakt --target "$TARGET")
[ "$PROFILE" = "release" ] && CARGO_ARGS+=(--release)

LIB="$REPO_ROOT/target/$TARGET/$PROFILE/liblibtakt.a"
OUT="$REPO_ROOT/macos/Takt/Generated"
STAMP="$OUT/.bindgen-stamp"

mkdir -p "$OUT" "$OUT/Headers" "$OUT/Modules" "$OUT/LibTaktFFI"

# 1. Build — cargo's own incremental check makes this ~0.5s when nothing changed
echo "==> Building libtakt ($PROFILE, $TARGET)..."
cargo build "${CARGO_ARGS[@]}"

# Also build the bindgen binary (cargo caches this too)
BINDGEN="$REPO_ROOT/target/debug/uniffi-bindgen-swift"
cargo build --manifest-path "$REPO_ROOT/Cargo.toml" \
    --package libtakt --bin uniffi-bindgen-swift

# 2. Regenerate bindings if lib OR bindgen binary is newer than stamp
if [ "$LIB" -nt "$STAMP" ] || [ "$BINDGEN" -nt "$STAMP" ] 2>/dev/null; then
    echo "==> Generating Swift bindings..."
    cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
        --package libtakt --bin uniffi-bindgen-swift -- \
        "$LIB" "$OUT" --swift-sources

    cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
        --package libtakt --bin uniffi-bindgen-swift -- \
        "$LIB" "$OUT/Headers" --headers

    cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
        --package libtakt --bin uniffi-bindgen-swift -- \
        "$LIB" "$OUT/Modules" --modulemap --modulemap-filename LibTaktFFI.modulemap

    # Clang module directory with module.modulemap + header
    cp "$OUT/Headers/LibTaktFFI.h" "$OUT/LibTaktFFI/LibTaktFFI.h"
    cat > "$OUT/LibTaktFFI/module.modulemap" << 'MODULEMAP'
module LibTaktFFI {
    header "LibTaktFFI.h"
    export *
}
MODULEMAP

    touch "$STAMP"
    echo "==> Bindings regenerated."
else
    echo "==> Bindings up to date, skipping."
fi
