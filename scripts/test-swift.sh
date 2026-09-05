#!/bin/bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEST_BUILD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/takt-swift-tests.XXXXXX")"
trap 'rm -rf "$TEST_BUILD_DIR"' EXIT

run_test() {
    local name="$1"
    local source="$2"
    xcrun swiftc -warnings-as-errors "$REPO_ROOT/$source" \
        "$REPO_ROOT/macos/TaktTests/$name.swift" -o "$TEST_BUILD_DIR/$name"
    "$TEST_BUILD_DIR/$name"
    echo "$name passed"
}

run_test CronUtilsTests macos/Takt/Helpers/CronUtils.swift
run_test UserActivityEligibilityStateTests macos/Takt/UserActivityEligibilityState.swift
