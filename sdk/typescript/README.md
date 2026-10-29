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
