# Plugin Platform: Roadmap Notes (Phase 11)

> Moved verbatim from the pre-PTF `ROADMAP.md` (Phase 11) during the migration to the Project Tracking Format. Delivery status is now tracked in `ROADMAP.md` milestones M18-M31.

Status: Proposed (design only, no implementation yet). Design documents: [PLUGIN_PLATFORM.md](PLUGIN_PLATFORM.md) (platform, sandbox, extension domains, phased delivery) and [PLUGIN_SYSTEM.md](PLUGIN_SYSTEM.md) (typed extension points: auth, user and secret providers, menus, pages, API proxy).

## Goal
- Let administrators extend OpenVox WebUI with third-party and first-party plugins without rebuilding the application and without putting the core at risk: untrusted code runs sandboxed, every capability is declared and approved, and every plugin is signed.

## Scope Summary
- Sandboxed runtimes: WASM (wasmtime) by default for backend logic, remote HTTPS plugins for external services, built-in Rust plugins for first-party providers
- Mandatory plugin signatures (ed25519) with a UI and API to add, review and revoke trusted keys, under a dedicated `plugins:trust` permission
- Capability-based permissions declared in a manifest and approved by an administrator, with re-approval when an update adds capabilities
- Plugin lifecycle: install, configure, enable, disable, upgrade (batch or scheduled), rollback, uninstall with purge; declarative migrations for plugin-owned storage
- UI extension through named slots (menus, dashboard, node, group and report panels), declarative pages, sandboxed iframes, and theme tokens; Module Federation only for trusted plugins
- Event bus with a durable outbox, signed inbound and outbound webhooks, and a plugin SDK (Rust and TypeScript)
- Authentication extensions: MFA engines (TOTP, WebAuthn, push), SSO (OIDC, SAML), step-up authentication, and LDAP/Active Directory as an authentication and user directory provider
- Report extensions (new report types, export formats, annotations, datasets) and plugin-provided facts (including opt-in export to agents)
- Advanced classification: expression-based rules, dynamic and generated groups, configurable group behavior, merge strategies, and explainable results with provenance
- Governance: per-plugin telemetry, quotas, circuit breakers, kill switch, audit trail, and an administrator dashboard

## Out of Scope (deferred)
- Multi-tenant plugin scoping (per-organization installation, data and branding)
- Marketplace (catalog, signed index, public and private registries, central revocation)
- The design keeps both reachable; see "Deferred" in the design document.

## Planned Sub-phases
The numbering follows the phased delivery table in PLUGIN_PLATFORM.md, section 17.

### Phase 11.0: Proofs of Concept
- Per-plugin SQLite storage with enforced limits (sqlx 0.9), wasmtime fuel and memory limits inside Axum, build and binary size impact on the RPM and DEB packages

### Phase 11.1: Plugin Manager and Signatures
- Manifest, `.ovxp` package, mandatory signature verification, trusted key management (API and UI), plugin states, admin API, Plugins page, built-in runtime, slot registry for menus, declarative pages

### Phase 11.2: WASM Runtime and Host API
- WASM runtime, capability gate and approval flow, `kv` and `db:own` storage, migrations, lifecycle hooks, resource limits

### Phase 11.3: Events, Jobs and Webhooks
- Event bus with outbox, dispatcher and filters, scheduled jobs, inbound and outbound webhooks

### Phase 11.4: Telemetry and Governance
- Metrics, quotas, circuit breaker, administrator dashboard, Prometheus `/metrics`

### Phase 11.5: Plugin UI and SDK
- Sandboxed iframe pages, TypeScript SDK, theme tokens, complete slot catalog

### Phase 11.6: Updates and Rollback
- Batch and scheduled updates, stored versions, rollback, local revocation

### Phase 11.7: Facts and Reports
- Plugin facts with merged view, opt-in rule usage and agent export, plugin report types, export formats, annotations, datasets

### Phase 11.8: Advanced Classification (core)
- Expression engine, group behavior policies, explain and provenance, impact preview. Independent of the plugin runtime and can start earlier; existing BDD classification scenarios must keep passing unchanged

### Phase 11.9: Classification Extensions for Plugins
- Pure operators and functions, membership providers and dynamic groups with churn guard, generated groups, merge strategies, safe degradation

### Phase 11.10: Secret Providers
- `SecretResolver`, `env` and `file` providers, `secret://` references in configuration

### Phase 11.11: Authentication Pipeline
- Challenge and step-up pipeline, built-in TOTP, recovery codes and WebAuthn, OIDC, local and SAML refactored as providers, MFA policies

### Phase 11.12: Directory Providers
- Built-in LDAP/Active Directory provider (bind authentication, lookup, JIT provisioning, scheduled sync, group-to-role mapping)

### Phase 11.13: Advanced Hooks
- Restricted interceptors, trusted `module` pages (if approved), inbound SCIM

## Open Questions
- wasmtime versus Extism, and the impact on the packaging pipeline
- Languages supported for plugin authors in the first release
- Whether Module Federation is acceptable in the target deployment, or should be compiled out
- Where plugin facts exported to agents should be delivered from, and whether plugins may write facts to PuppetDB
- Expression language for classification (CEL subset or a custom AST) and realistic latency budgets for the classification hot path
- Who owns the project signing key for first-party plugins and how it is rotated

The full list is in PLUGIN_PLATFORM.md, section 19.

## Success Criteria
- An unsigned plugin, or one signed by an untrusted key, can never be installed or loaded
- A plugin that exceeds its limits or fails is isolated without affecting the core or other plugins
- Administrators can see and approve exactly what each plugin may access before it runs
- Existing classification results are unchanged for existing groups after the engine changes
- MFA and SSO can be enforced by policy, and the local break-glass administrator remains available
