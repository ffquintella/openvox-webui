# OpenVox WebUI Makefile
# Development convenience targets

.PHONY: help build build-release run dev test test-unit test-bdd lint fmt check clean install-deps setup version version-patch version-minor version-major package package-rpm package-deb package-clean-cache publish-puppet-module release release-dry-run

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
	@echo "Packaging (incremental builds with Docker cache):"
	@echo "  make package            - Build RPM and DEB packages"
	@echo "  make package-rpm        - Build RPM package only (incremental)"
	@echo "  make package-deb        - Build DEB package only (incremental)"
	@echo "  make package-clean-cache - Clear build cache for full rebuild"
	@echo ""
	@echo "Versioning:"
	@echo "  make version        - Show current version"
	@echo "  make version-patch  - Bump patch version (0.1.0 -> 0.1.1)"
	@echo "  make version-minor  - Bump minor version, commit, and tag (0.1.0 -> 0.2.0)"
	@echo "  make version-major  - Bump major version, commit, and tag (0.1.0 -> 1.0.0)"
	@echo ""
	@echo "Releasing:"
	@echo "  make release        - Create and push release tag (triggers GitHub Actions build)"
	@echo "  make release-dry-run - Show what release would be created (no changes)"
	@echo ""
	@echo "Puppet Module:"
	@echo "  make publish-puppet-module - Publish Puppet module to Puppet Forge"
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
	@if [ -d "frontend/node_modules" ]; then \
		cd frontend && npm test -- --run; \
	else \
		echo "Skipping frontend tests (run 'make install-deps' first)"; \
	fi

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
# Packaging (uses Docker with incremental build cache)
# Cache location: ~/.cache/openvox-webui-build
# First build takes ~10-15 min, subsequent builds are much faster
# =============================================================================

# Build cache directory (can be overridden)
CACHE_DIR ?= $(HOME)/.cache/openvox-webui-build

package:
	./scripts/build-packages.sh

package-rpm:
	./scripts/build-packages.sh rpm

package-deb:
	./scripts/build-packages.sh deb

package-clean-cache:
	@echo "Cleaning package build cache..."
	rm -rf $(CACHE_DIR)
	@echo "Cache cleared. Next build will be a full rebuild."

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

# =============================================================================
# Versioning
# =============================================================================

# Get current version from Cargo.toml
CURRENT_VERSION := $(shell grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

version:
	@echo "Current version: $(CURRENT_VERSION)"
	@echo "  Cargo.toml:            $$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')"
	@echo "  frontend/package.json: $$(grep '"version"' frontend/package.json | head -1 | sed 's/.*"version": "\(.*\)".*/\1/')"
	@echo "  puppet/metadata.json:  $$(grep '"version"' puppet/metadata.json | head -1 | sed 's/.*"version": "\(.*\)".*/\1/')"

version-patch:
	@./scripts/bump-version.sh patch

version-minor:
	@./scripts/bump-version.sh minor
	@NEW_VERSION=$$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'); \
	git add Cargo.toml Cargo.lock frontend/package.json puppet/metadata.json; \
	git commit -m "chore: bump version to v$$NEW_VERSION"; \
	git tag -a "v$$NEW_VERSION" -m "Release v$$NEW_VERSION"; \
	echo ""; \
	echo "Created git tag: v$$NEW_VERSION"; \
	echo "Don't forget to push: git push && git push --tags"

version-major:
	@./scripts/bump-version.sh major
	@NEW_VERSION=$$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'); \
	git add Cargo.toml Cargo.lock frontend/package.json puppet/metadata.json; \
	git commit -m "chore: bump version to v$$NEW_VERSION"; \
	git tag -a "v$$NEW_VERSION" -m "Release v$$NEW_VERSION"; \
	echo ""; \
	echo "Created git tag: v$$NEW_VERSION"; \
	echo "Don't forget to push: git push && git push --tags"

# =============================================================================
# Releasing
# =============================================================================

# Create a release tag that triggers GitHub Actions to build packages
# This creates a tag under Releases/v<version> which triggers the release workflow
release:
	@CURRENT_VERSION=$$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'); \
	RELEASE_TAG="Releases/v$$CURRENT_VERSION"; \
	echo ""; \
	echo "Creating release for version $$CURRENT_VERSION"; \
	echo "========================================"; \
	echo ""; \
	if git rev-parse "$$RELEASE_TAG" >/dev/null 2>&1; then \
		echo "ERROR: Release tag $$RELEASE_TAG already exists!"; \
		echo ""; \
		echo "If you want to re-release this version:"; \
		echo "  git tag -d $$RELEASE_TAG"; \
		echo "  git push origin :refs/tags/$$RELEASE_TAG"; \
		echo ""; \
		echo "Or bump the version first:"; \
		echo "  make version-patch  # for bug fixes"; \
		echo "  make version-minor  # for new features"; \
		exit 1; \
	fi; \
	echo "Creating tag: $$RELEASE_TAG"; \
	git tag -a "$$RELEASE_TAG" -m "Release v$$CURRENT_VERSION"; \
	echo "Pushing tag to origin..."; \
	git push origin "$$RELEASE_TAG"; \
	echo ""; \
	echo "========================================"; \
	echo "Release tag created and pushed!"; \
	echo ""; \
	echo "GitHub Actions will now build packages for v$$CURRENT_VERSION"; \
	echo "Monitor progress at: https://github.com/$$(git remote get-url origin | sed 's/.*github.com[:/]\(.*\)\.git/\1/' | sed 's/.*github.com[:/]\(.*\)/\1/')/actions"; \
	echo ""; \
	echo "Once complete, packages will be available at:"; \
	echo "https://github.com/$$(git remote get-url origin | sed 's/.*github.com[:/]\(.*\)\.git/\1/' | sed 's/.*github.com[:/]\(.*\)/\1/')/releases/tag/v$$CURRENT_VERSION"

# Show what release would be created without making any changes
release-dry-run:
	@CURRENT_VERSION=$$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'); \
	RELEASE_TAG="Releases/v$$CURRENT_VERSION"; \
	echo ""; \
	echo "DRY RUN - No changes will be made"; \
	echo "========================================"; \
	echo ""; \
	echo "Current version: $$CURRENT_VERSION"; \
	echo "Release tag:     $$RELEASE_TAG"; \
	echo ""; \
	if git rev-parse "$$RELEASE_TAG" >/dev/null 2>&1; then \
		echo "WARNING: Tag $$RELEASE_TAG already exists!"; \
		echo "         You would need to delete it first or bump the version."; \
	else \
		echo "Tag $$RELEASE_TAG does not exist yet (ready to create)"; \
	fi; \
	echo ""; \
	echo "This would:"; \
	echo "  1. Create annotated tag: $$RELEASE_TAG"; \
	echo "  2. Push tag to origin"; \
	echo "  3. Trigger GitHub Actions release workflow"; \
	echo "  4. Build RPM, DEB, and binary packages"; \
	echo "  5. Create GitHub Release with all artifacts"; \
	echo ""; \
	echo "To proceed, run: make release"

# =============================================================================
# Puppet Module
# =============================================================================

publish-puppet-module:
	@./scripts/publish-puppet-module.sh
