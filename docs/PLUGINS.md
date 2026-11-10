# Plugin Platform

OpenVox WebUI can be extended with signed plugins. This page is the official overview of the platform for administrators and plugin authors. The full design and the contracts live in [architecture/PLUGIN_PLATFORM.md](architecture/PLUGIN_PLATFORM.md) and [architecture/PLUGIN_SYSTEM.md](architecture/PLUGIN_SYSTEM.md); when this page and those documents differ, they prevail.

## What plugins can do

| Extension point | What it adds | Reference |
|---|---|---|
| Menus and pages | Entries in the navigation and pages rendered declaratively, in a sandboxed frame, or (trusted plugins only) as host modules | PLUGIN_SYSTEM.md §4, PLUGIN_PLATFORM.md §8 |
| UI slots | Panels in named places of the UI, such as the profile security section, the Authentication tab of Settings and the user panel of the Users page | PLUGIN_PLATFORM.md §8, §17.18 |
| Authentication | Identity providers (SSO, LDAP/AD), user providers, directory providers, inbound SCIM and second factors | PLUGIN_PLATFORM.md §11, §17.10 to §17.15 |
| API routes | Routes served by the plugin under `/api/v1/plugins/<plugin-id>/...` | PLUGIN_PLATFORM.md §17.16 |
| Secret providers | External sources for secret references | PLUGIN_PLATFORM.md §17.9 |
| Events | Subscribers and restricted interceptors on the host event bus | PLUGIN_PLATFORM.md §9, §17.12 |
| Facts, reports and classification | Extra fact and report sources and classification extensions | PLUGIN_PLATFORM.md §12, §13 |

## Security model

- **Signed packages only.** A plugin is a `.ovxp` package signed with an ed25519 key. The signature is checked against the trusted keys before anything is shown, and the package hash is pinned at install time; a mismatch at load time blocks the plugin.
- **Isolation by default.** Backend code runs as WebAssembly with no file system, network, clock or environment access. The only channel out is the host API, and every call goes through a capability check. Frontend code runs declaratively or in a sandboxed frame. Host modules (same JavaScript context as the UI) are off by default (`plugins.allow_trusted_modules`) and need an explicit trust decision per plugin version.
- **Capabilities are deny by default.** A plugin declares what it needs in `plugin.yaml`. The effective set is what the manifest declares, the administrator approves and the configured policy ceiling allows. An update that adds a capability stays pending until it is approved. Critical capabilities such as `auth:factor` cannot be optional.
- **Limits and fail-closed behaviour.** Every call has a time, memory and quota limit and feeds a circuit breaker. A trap, timeout or refusal is an error, never a grant. A plugin outage does not lock out the local super-administrator break-glass path.
- **Plugins stay apart.** A plugin reads only its own records. It never sees another plugin's data, built-in credential records, secrets or the signing key.

## Administering plugins

Everything below needs the `plugins:admin` permission (module trust needs `plugins:trust`).

1. Open **Plugins** and install a package. The host verifies the signature and lists each requested capability with its risk level.
2. Approve the capabilities, then enable the plugin. Disabling keeps its data; uninstalling removes it.
3. Settings are saved per plugin; secret fields are write-only.
4. Disabling or uninstalling a plugin that provides second factors answers with a `users_without_usable_factor` warning when users would be left without a factor. See [Configuration](CONFIGURATION.md) and the [User Guide](USER_GUIDE.md#plugins-trusted-module-pages).

The admin API (`GET/POST /api/v1/plugins`, `POST /api/v1/plugins/preflight`, grants, trust keys) is listed in PLUGIN_SYSTEM.md §12 and PLUGIN_PLATFORM.md §4.

## Writing a plugin

- The manifest `api_version` is `2`. New manifest fields are optional and backward compatible.
- A package holds `plugin.yaml`, `MANIFEST.sha256`, `plugin.sig`, the WASM module, optional UI assets, migrations and i18n files (PLUGIN_PLATFORM.md §4.1).
- Plugin storage is versioned SQL migrations run by the host, plus a key-value store (PLUGIN_PLATFORM.md §7).
- The declarative UI format is described in PLUGIN_PLATFORM.md §17.18 and validated against [architecture/plugin-declarative-spec.schema.json](architecture/plugin-declarative-spec.schema.json).
- A worked example of a plugin that provides a second factor is the mfa-fido2 hand-off: [architecture/MFA_FIDO2_HOST_HANDOFF.md](architecture/MFA_FIDO2_HOST_HANDOFF.md), with a recorded end-to-end run in [architecture/e2e-mfa-fido2-transcript.md](architecture/e2e-mfa-fido2-transcript.md). That run used a software authenticator and Chrome's virtual authenticator, not a physical security key.

## Status

Implemented: plugin manager, signatures and trusted keys, admin API and UI, slot registry and declarative panels, sandboxed pages, event bus and restricted interceptors, secret and directory providers, the authentication pipeline, inbound SCIM, second factors, plugin API routes and enrollment grace windows. The delivery history is in PLUGIN_PLATFORM.md §17 and [ROADMAP.md](../ROADMAP.md). Multi-tenant isolation per organization and a plugin marketplace are not part of the platform yet (PLUGIN_PLATFORM.md §18).
