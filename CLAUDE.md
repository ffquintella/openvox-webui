# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Git Commit Guidelines

**IMPORTANT:** When creating git commits, do NOT include:
Keep commit messages clean and focused only on describing the changes made.

## Project Overview

OpenVox WebUI is a web interface for managing OpenVox infrastructure. It provides PuppetDB integration, node classification (similar to Puppet Enterprise), facter generation, and dashboard visualization.

**Tech Stack:**
- Backend: Rust + Axum
- Frontend: React + TypeScript + Tailwind CSS
- Database: SQLite (via SQLx)
- Testing: Cucumber BDD + unit tests

## Build Commands

```bash
# Backend
cargo build                    # Build
cargo build --release         # Build for release
cargo run                      # Run development server
cargo check                    # Check without building
cargo fmt                      # Format code
cargo clippy                   # Run linter

# Run tests
cargo test                     # All tests
cargo test --test cucumber    # BDD tests only
cargo test test_name          # Specific test

# Frontend (from frontend/ directory)
npm install                    # Install dependencies
npm run dev                    # Development server (port 3000)
npm run build                  # Production build
npm run lint                   # ESLint
npm run format                 # Prettier
npm test                       # Vitest tests

# Package building
./scripts/build-packages.sh          # Build all packages (RPM + DEB)
./scripts/build-packages.sh rpm      # Build RPM only
./scripts/build-packages.sh deb      # Build DEB only
./scripts/build-packages.sh -v 0.2.0 # Build with specific version
```

## Architecture

### Backend Structure (src/)
- `api/` - Axum route handlers (health, nodes, groups, facts, reports)
- `config/` - YAML configuration management
- `db/` - SQLite database layer with SQLx
- `models/` - Data models (Node, NodeGroup, Report, Fact, Classification)
- `services/` - Business logic:
  - `puppetdb.rs` - PuppetDB API client
  - `classification.rs` - Rule-based node classification engine
  - `facter.rs` - External fact generation
- `utils/` - Error handling, validation

### Frontend Structure (frontend/src/)
- `components/` - Reusable UI components (Layout)
- `pages/` - Route pages (Dashboard, Nodes, Groups, Reports, Facts, Settings)
- `services/api.ts` - API client with axios
- `stores/` - Zustand state management
- `hooks/` - React Query hooks (useNodes, useGroups)
- `types/` - TypeScript type definitions

### Key Patterns
- API routes: `/api/v1/{resource}`
- State management: Zustand for auth, React Query for server state
- Classification: Fact-based rules with operators (=, !=, ~, >, <, in)
- Error handling: Custom `AppError` enum implementing `IntoResponse`

## Configuration

YAML files in `config/`:
- `config.yaml` - Server, PuppetDB, auth, database settings
- `groups.yaml` - Node group definitions with rules
- `facter_templates.yaml` - Templates for generating external facts

## Testing

BDD features in `tests/features/*.feature`:
- `node_classification.feature` - Group rules and classification
- `nodes.feature` - Node CRUD operations
- `reports.feature` - Report queries
- `authentication.feature` - Auth flows
- `facter_generation.feature` - Fact generation

Step definitions in `tests/features/step_definitions/`.

## Database

SQLite with migrations in `migrations/`. Key tables:
- `users` - Authentication
- `node_groups` - Classification groups
- `classification_rules` - Rules per group
- `pinned_nodes` - Static node assignments
- `fact_templates` - Facter templates

## API Endpoints

```
# Health
GET  /api/v1/health

# Nodes (PuppetDB integration)
GET  /api/v1/nodes
GET  /api/v1/nodes/:certname
GET  /api/v1/nodes/:certname/facts
GET  /api/v1/nodes/:certname/reports

# Groups (full CRUD)
GET    /api/v1/groups
POST   /api/v1/groups
GET    /api/v1/groups/:id
PUT    /api/v1/groups/:id
DELETE /api/v1/groups/:id
GET    /api/v1/groups/:id/nodes
GET    /api/v1/groups/:id/rules
POST   /api/v1/groups/:id/rules
DELETE /api/v1/groups/:id/rules/:ruleId
POST   /api/v1/groups/:id/pinned
DELETE /api/v1/groups/:id/pinned/:certname

# Facts
GET  /api/v1/facts
GET  /api/v1/facts/names

# Reports
GET  /api/v1/reports
GET  /api/v1/reports/:hash

# Facter Templates (full CRUD)
GET    /api/v1/facter/templates
POST   /api/v1/facter/templates
GET    /api/v1/facter/templates/:id
PUT    /api/v1/facter/templates/:id
DELETE /api/v1/facter/templates/:id
POST   /api/v1/facter/generate
GET    /api/v1/facter/export/:certname
```
