# @openvox/plugin-sdk

Client SDK for OpenVox WebUI plugin pages. A page declared with `render: iframe`
runs in a sandboxed iframe (no same-origin access, no cookies, CSP limited to the
approved `http:egress` hosts) and talks to the host through this package.

```ts
import { connect } from '@openvox/plugin-sdk';

const app = await connect();
const { params, theme } = app.getContext();           // e.g. { certname: "web01" }
const nodes = await app.api.fetch('/nodes?limit=20');  // needs the nodes:read capability
await app.showNotification({ level: 'success', text: 'Synced' });
await app.navigate('/nodes/web01');
app.on('theme.changed', () => console.log('theme changed'));
```

The host sends design tokens (`--ov-color-primary`, `--ov-font-body`, `--ov-radius`, ...)
in the handshake and on every theme change; the SDK sets them on `:root`, so plain
CSS such as `color: var(--ov-color-text)` follows the host theme.

- `app.api.fetch(path)` is a read-only (GET) call as the signed-in user: the host
  checks the plugin's approved capability for the area (`nodes:read`, `facts:read`,
  `reports:read`, `groups:read`, `users:read`) and the user's own RBAC still applies.
  A failure rejects with an `OpenVoxError` (`code` and `message`).
- `app.resize(px)` sizes the frame (ignored for full pages).
- `app.navigate(path)` moves the host to one of its own routes.

Build with `npm install && npm run build`.

## Trusted module pages

A contribution with `render: module` does not use this SDK: it runs in the host
page without a sandbox, so it is served only when `plugins.allow_trusted_modules`
is on and an administrator trusted the exact plugin version. Declare the entry
file and its Subresource Integrity:

```yaml
contributes:
  slots:
    - { slot: page, id: console, render: module, src: ui/remoteEntry.js,
        integrity: "sha384-...", expose: ./Page, path: /p/acme/console }
```

The entry is a Module Federation container (an ES module exporting `init` and
`get`) or a plain ES module with a default export. The host shares its own
`react`, `react-dom`, `react-dom/client` and `react/jsx-runtime` as singletons
and renders the exposed module's default export with `{context, host}`
(`host.navigate(path)`, `host.notify(level, text)`). Build the entry as one
self-contained file: only it is covered by the SRI and served.
Compute the value with `openssl dgst -sha384 -binary remoteEntry.js | openssl base64 -A`.
