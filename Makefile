# Takt Makefile
# SwiftUI + Rust (libtakt via UniFFI) — macOS menu bar app

APP_NAME := Takt

.DEFAULT_GOAL := help

.PHONY: help
help: ## Show this help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n\nTargets:\n"} \
	  /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@echo ""

# ─── Development ───────────────────────────────────────────────────────────

.PHONY: test
test: ## Run Rust unit tests
	cargo test --package libtakt

.PHONY: build-rust
build-rust: ## Build Rust static lib + generate Swift bindings
	./scripts/build-rust.sh release

.PHONY: build
build: ## Build the app via Xcode (Release)
	cd macos && xcodebuild -project Takt.xcodeproj -scheme Takt -configuration Release build

.PHONY: build-debug
build-debug: ## Build the app via Xcode (Debug)
	cd macos && xcodebuild -project Takt.xcodeproj -scheme Takt -configuration Debug build

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
	cd macos && xcodebuild -project Takt.xcodeproj -scheme Takt clean

.PHONY: clean-all
clean-all: clean clean-xcode ## Remove all build artifacts
