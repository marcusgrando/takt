# Takt Makefile
# Tauri 2 + React + Rust — macOS menu bar app

APP_NAME     := takt
VERSION      := 0.1.0
BUNDLE_DIR   := src-tauri/target/release/bundle
APP_PATH     := $(BUNDLE_DIR)/macos/$(APP_NAME).app
DMG_PATH     := $(BUNDLE_DIR)/dmg/$(APP_NAME)_$(VERSION)_aarch64.dmg
TAURI_DIR    := src-tauri

.DEFAULT_GOAL := help

# ─── Help ──────────────────────────────────────────────────────────────────────

.PHONY: help
help: ## Show this help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n\nTargets:\n"} \
	  /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@echo ""

# ─── Development ───────────────────────────────────────────────────────────────

.PHONY: dev
dev: ## Start Tauri dev mode (hot reload)
	bunx tauri dev

.PHONY: frontend
frontend: ## Start Vite frontend dev server only
	bun run dev

# ─── Build ─────────────────────────────────────────────────────────────────────

.PHONY: build
build: ## Build frontend (TypeScript + Vite)
	bun run build

.PHONY: app
app: ## Build release .app bundle only
	bunx tauri build --bundles app
	@echo ""
	@echo "  App bundle: $(APP_PATH)"

.PHONY: app-debug
app-debug: ## Build app in debug mode (faster, no optimizations)
	bunx tauri build --debug --bundles app
	@echo ""
	@echo "  Debug app: src-tauri/target/debug/bundle/macos/$(APP_NAME).app"

.PHONY: dmg
dmg: ## Build signed release DMG (requires Developer ID certificate)
	bunx tauri build --bundles dmg
	@echo ""
	@echo "  DMG: $(DMG_PATH)"

# ─── Testing ───────────────────────────────────────────────────────────────────

.PHONY: test
test: ## Run all Rust unit tests
	cargo test --manifest-path $(TAURI_DIR)/Cargo.toml

.PHONY: test-verbose
test-verbose: ## Run Rust tests with output
	cargo test --manifest-path $(TAURI_DIR)/Cargo.toml -- --nocapture

.PHONY: typecheck
typecheck: ## Run TypeScript type-check only (no emit)
	bunx tsc --noEmit

# ─── Code Quality ──────────────────────────────────────────────────────────────

.PHONY: lint
lint: ## Run Rust clippy linter
	cargo clippy --manifest-path $(TAURI_DIR)/Cargo.toml -- -D warnings

.PHONY: fmt
fmt: ## Format Rust code
	cargo fmt --manifest-path $(TAURI_DIR)/Cargo.toml

.PHONY: fmt-check
fmt-check: ## Check Rust formatting without modifying files
	cargo fmt --manifest-path $(TAURI_DIR)/Cargo.toml -- --check

# ─── CI / Full Verification ────────────────────────────────────────────────────

.PHONY: check
check: fmt-check lint typecheck test ## Run all checks (format, lint, typecheck, tests)
	@echo ""
	@echo "  All checks passed."

# ─── Clean ─────────────────────────────────────────────────────────────────────

.PHONY: clean
clean: ## Remove frontend build artifacts
	rm -rf dist

.PHONY: clean-rust
clean-rust: ## Remove Rust build artifacts
	cargo clean --manifest-path $(TAURI_DIR)/Cargo.toml

.PHONY: clean-all
clean-all: clean clean-rust ## Remove all build artifacts (frontend + Rust)

# ─── Install ───────────────────────────────────────────────────────────────────

.PHONY: install
install: ## Install frontend dependencies
	bun install

.PHONY: open
open: ## Open the built .app bundle
	open $(APP_PATH)
