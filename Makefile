# OpenVox WebUI Makefile
# Development convenience targets

.PHONY: help build build-release run dev test test-unit test-bdd lint fmt check clean install-deps setup

# Default target
help:
	@echo "OpenVox WebUI Development Commands"
	@echo ""
	@echo "Setup:"
	@echo "  make setup          - Install all dependencies and set up environment"
	@echo "  make install-deps   - Install Rust and Node.js dependencies"
	@echo ""
	@echo "Development:"
	@echo "  make dev            - Run backend and frontend in development mode"
	@echo "  make run            - Run backend only"
	@echo "  make run-frontend   - Run frontend only"
	@echo ""
	@echo "Building:"
	@echo "  make build          - Build backend (debug)"
	@echo "  make build-release  - Build backend (release)"
	@echo "  make build-frontend - Build frontend for production"
	@echo "  make build-all      - Build everything for production"
	@echo ""
	@echo "Testing:"
	@echo "  make test           - Run all tests"
	@echo "  make test-unit      - Run unit tests only"
	@echo "  make test-bdd       - Run BDD/Cucumber tests"
	@echo "  make test-frontend  - Run frontend tests"
	@echo ""
	@echo "Code Quality:"
	@echo "  make lint           - Run all linters"
	@echo "  make fmt            - Format all code"
	@echo "  make check          - Run all checks (lint, fmt, typecheck)"
	@echo ""
	@echo "Packaging:"
	@echo "  make package        - Build RPM and DEB packages"
	@echo "  make package-rpm    - Build RPM package only"
	@echo "  make package-deb    - Build DEB package only"
	@echo ""
	@echo "Cleanup:"
	@echo "  make clean          - Remove build artifacts"

# =============================================================================
# Setup
# =============================================================================

setup: install-deps
	@echo "Creating data directory..."
	@mkdir -p data
	@echo "Copying example config..."
	@cp -n .env.example .env 2>/dev/null || true
	@echo "Setup complete!"

install-deps:
	@echo "Installing Rust dependencies..."
	cargo fetch
	@echo "Installing frontend dependencies..."
	cd frontend && npm install

# =============================================================================
# Development
# =============================================================================

dev:
	@echo "Starting development servers..."
	@make -j2 run run-frontend

run:
	cargo run

run-frontend:
	cd frontend && npm run dev

# =============================================================================
# Building
# =============================================================================

build:
	cargo build

build-release:
	cargo build --release

build-frontend:
	cd frontend && npm run build

build-all: build-release build-frontend

# =============================================================================
# Testing
# =============================================================================

test: test-unit test-bdd test-frontend

test-unit:
	cargo test --lib

test-bdd:
	cargo test --test cucumber

test-frontend:
	cd frontend && npm test -- --run

test-coverage:
	cargo llvm-cov --html
	cd frontend && npm run test:coverage

# =============================================================================
# Code Quality
# =============================================================================

lint: lint-rust lint-frontend

lint-rust:
	cargo clippy --all-targets --all-features -- -D warnings

lint-frontend:
	cd frontend && npm run lint

fmt: fmt-rust fmt-frontend

fmt-rust:
	cargo fmt

fmt-frontend:
	cd frontend && npm run format

fmt-check: fmt-check-rust fmt-check-frontend

fmt-check-rust:
	cargo fmt -- --check

fmt-check-frontend:
	cd frontend && npm run format:check

check: fmt-check lint
	cargo check
	cd frontend && npm run typecheck

# =============================================================================
# Packaging
# =============================================================================

package:
	./scripts/build-packages.sh

package-rpm:
	./scripts/build-packages.sh rpm

package-deb:
	./scripts/build-packages.sh deb

# =============================================================================
# Database
# =============================================================================

db-migrate:
	@echo "Running database migrations..."
	cargo sqlx migrate run

db-reset:
	@echo "Resetting database..."
	rm -f data/openvox.db
	cargo sqlx migrate run

# =============================================================================
# Cleanup
# =============================================================================

clean:
	cargo clean
	rm -rf frontend/dist frontend/node_modules/.cache
	rm -rf build/

clean-all: clean
	rm -rf frontend/node_modules
	rm -rf data/
