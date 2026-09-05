# Takt Makefile
# SwiftUI + Rust (libtakt via UniFFI) — macOS menu bar app

APP_NAME := Takt
BUILD_DIR := $(CURDIR)/build

.DEFAULT_GOAL := help

.PHONY: help
help: ## Show this help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n\nTargets:\n"} \
	  /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@echo ""

# ─── Development ───────────────────────────────────────────────────────────

.PHONY: test
test: test-rust test-swift ## Run Rust and Swift tests

.PHONY: test-rust
test-rust: ## Run Rust unit tests
	cargo test --package libtakt

.PHONY: test-swift
test-swift: ## Run standalone Swift regression tests
	bash scripts/test-swift.sh

.PHONY: build-ci
build-ci: ## Build the arm64 macOS app without code signing
	cd macos && xcodebuild -project Takt.xcodeproj -scheme Takt -configuration Debug -derivedDataPath "$(BUILD_DIR)" -destination 'generic/platform=macOS' ARCHS=arm64 CODE_SIGNING_ALLOWED=NO build

.PHONY: build-rust
build-rust: ## Build Rust static lib + generate Swift bindings
	./scripts/build-rust.sh release

.PHONY: build
build: ## Build the app via Xcode (Release) → build/Takt.app
	cd macos && xcodebuild -project Takt.xcodeproj -scheme Takt -configuration Release -derivedDataPath "$(BUILD_DIR)" build

.PHONY: build-debug
build-debug: ## Build the app via Xcode (Debug) → build/Takt.app
	cd macos && xcodebuild -project Takt.xcodeproj -scheme Takt -configuration Debug -derivedDataPath "$(BUILD_DIR)" build

# ─── Code Quality ──────────────────────────────────────────────────────────

.PHONY: lint
lint: ## Run Rust clippy linter
	cargo clippy --package libtakt -- -D warnings

.PHONY: fmt
fmt: ## Format Rust code
	cargo fmt --package libtakt

.PHONY: fmt-check
fmt-check: ## Check Rust formatting
	cargo fmt --package libtakt -- --check

.PHONY: check
check: fmt-check lint test ## Run all checks (format, lint, tests)
	@echo ""
	@echo "  All checks passed."

# ─── Clean ─────────────────────────────────────────────────────────────────

.PHONY: clean
clean: ## Remove Rust build artifacts
	cargo clean

.PHONY: clean-xcode
clean-xcode: ## Clean Xcode derived data
	rm -rf "$(BUILD_DIR)"

.PHONY: clean-all
clean-all: clean clean-xcode ## Remove all build artifacts
