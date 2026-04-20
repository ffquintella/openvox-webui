# OpenVox WebUI Development Guide

Practical notes for contributors on how to build, test, and package both backend and frontend, plus Puppet module packaging instructions.

## 1. Prerequisites
- Rust toolchain (stable) with `rustup` installed; `cargo` on PATH.
- Node.js ≥ 18 with npm.
- SQLite dev tools (for local builds).
- PDK or Puppet installed if building the Puppet module.
- Optional: `make`, `docker` if building packages inside containers.

## 2. Repository Layout
- **Backend**: Rust/Axum under `src/`, tests under `tests/`.
- **Frontend**: React/TypeScript/Tailwind under `frontend/`.
- **Puppet module**: `puppet/` (metadata, manifests, templates, pkg/).
- **Docs**: `docs/` (architecture, user-guide, install/config/upgrade).
- **Scripts**: helper scripts under `scripts/`.

## 3. Common Commands
Backend:
```bash
cargo fmt
cargo clippy
cargo test
cargo run
```

Frontend (from `frontend/`):
```bash
npm install
npm run lint
npm run test
npm run build
```

Full test sweep:
```bash
make test        # all
make test-unit   # Rust unit
make test-bdd    # Cucumber BDD
make test-frontend
```

## 4. Building the Application
Backend release build:
```bash
cargo build --release
```

Frontend production build (outputs to `frontend/dist`):
```bash
cd frontend
npm ci
npm run build
```

Serve frontend via backend (default) by pointing `static_dir` to the built `dist` path in `config.yaml`.

## 5. Packaging (RPM/DEB)
We provide a helper script to build packages (requires Docker).

```bash
./scripts/build-packages.sh          # build RPM + DEB
./scripts/build-packages.sh rpm      # RPM only
./scripts/build-packages.sh deb      # DEB only
./scripts/build-packages.sh -v 0.9.1 # override version
```

Artifacts are written under `packaging/` and `target/` as configured by the script. Inspect logs for the exact output paths.

## 6. Puppet Module Packaging
The Puppet module lives in `puppet/`. Use the publish helper (supports dry-run and falls back to Forge API).

### Build only (dry run)
```bash
./scripts/publish-puppet-module.sh --dry-run
```
This validates metadata, runs syntax/epp validation, and produces `puppet/pkg/ffquintella-openvox_webui-<version>.tar.gz`.

### Publish to Puppet Forge
1. Set or create a Forge API token (recommended):
   - Env: `export FORGE_TOKEN=<token>`
   - Or file: `~/.puppetlabs/token` containing the token
2. Run:
   ```bash
   ./scripts/publish-puppet-module.sh --version <x.y.z>
   ```
   The script uploads via `curl` to `https://forgeapi.puppet.com/v3/releases`. It prompts if no token is available.

Notes:
- Module name: `ffquintella-openvox_webui`
- Tarball output: `puppet/pkg/ffquintella-openvox_webui-<version>.tar.gz`
- Only the API upload path is used; `puppet module upload` is intentionally not called.

## 7. Versioning
- Bump versions in both `Cargo.toml` and `frontend/package.json` together (`make version-patch`/`version-minor`/`version-major` can help).
- Update `CHANGELOG.md` before committing (see CLAUDE.md rules).

## 8. Coding Standards
- Rust: `cargo fmt`, `cargo clippy`.
- TypeScript: `npm run lint`, `npm run format` (if configured).
- Keep source files < 1000 lines (see `CLAUDE.md` guidance).
- Prefer `apply_patch` for small edits; avoid reverting user changes.

## 9. Troubleshooting
- Puppet publish 401/403: ensure Forge token is valid and matches the module namespace (`ffquintella-openvox_webui`).
- If `pdk` isn’t available, the publish script falls back to Puppet CLI tooling where possible; upload is always via Forge API.
- Frontend build errors: clear `frontend/node_modules` and `npm ci` again.
- Database issues: remove local SQLite db (`data/` or `/tmp/openvox_test_*.db`) for a clean slate in dev.

