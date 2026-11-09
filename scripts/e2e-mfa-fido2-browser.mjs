#!/usr/bin/env node
// Browser end-to-end check of the mfa-fido2 stand-in plugin: the real server
// (target/debug/openvox-webui), the built web UI (frontend/dist) and Chrome
// driven over the DevTools protocol with a virtual WebAuthn authenticator.
// Software only: no physical security key is used or claimed.
// Needs Google Chrome, Node 22+, `npm run build` and `cargo build`.
// Env: OVX_E2E_CHROME, OVX_E2E_PORT (5099), OVX_E2E_OUT (transcript file).

import { spawn, spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PORT = Number(process.env.OVX_E2E_PORT || 5099);
const CDP_PORT = PORT + 1;
const BASE = `http://localhost:${PORT}`;
const CHROME =
  process.env.OVX_E2E_CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const FACTOR = 'mfa-fido2:security-key';
const ADMIN_PASSWORD = `admin-${crypto.randomBytes(6).toString('hex')}`;
const ALICE_PASSWORD = `alice-${crypto.randomBytes(6).toString('hex')}`;

const log = [];
const out = (line = '') => {
  log.push(line);
  console.log(line);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function fail(message) {
  throw new Error(message);
}

// ---- server ---------------------------------------------------------------------

/** argon2id PHC string with the parameters of the host (argon2 crate defaults). */
function argon2Hash(password) {
  const salt = crypto.randomBytes(16);
  const tag = crypto.argon2Sync('argon2id', {
    message: Buffer.from(password),
    nonce: salt,
    parallelism: 1,
    tagLength: 32,
    memory: 19456,
    passes: 2,
  });
  const b64 = (b) => b.toString('base64').replace(/=+$/, '');
  return `$argon2id$v=19$m=19456,t=2,p=1$${b64(salt)}$${b64(tag)}`;
}

function sqlite(db, sql) {
  const r = spawnSync('sqlite3', [db, sql], { encoding: 'utf8' });
  if (r.status !== 0) fail(`sqlite3 failed: ${r.stderr}`);
  return r.stdout.trim();
}

async function startServer(work) {
  const db = path.join(work, 'openvox.db');
  const config = [
    'server:',
    `  host: "127.0.0.1"`,
    `  port: ${PORT}`,
    '  serve_frontend: true',
    `  static_dir: "${path.join(ROOT, 'frontend/dist')}"`,
    'database:',
    `  url: "sqlite://${db}?mode=rwc"`,
    'auth:',
    `  jwt_secret: "${crypto.randomBytes(32).toString('hex')}"`,
    '  bcrypt_cost: 4',
    'inventory:',
    `  database_url: "sqlite://${path.join(work, 'inventory.db')}?mode=rwc"`,
    'plugins:',
    `  data_dir: "${path.join(work, 'plugins')}"`,
    'logging:',
    '  level: "info"',
    '',
  ].join('\n');
  fs.writeFileSync(path.join(work, 'config.yaml'), config);
  const logFile = fs.openSync(path.join(work, 'server.log'), 'w');
  const server = spawn(path.join(ROOT, 'target/debug/openvox-webui'), [], {
    cwd: work,
    env: { ...process.env, OPENVOX_CONFIG: path.join(work, 'config.yaml') },
    stdio: ['ignore', logFile, logFile],
  });
  for (let i = 0; i < 120; i++) {
    try {
      const r = await fetch(`${BASE}/api/v1/health`);
      if (r.ok) return { server, db };
    } catch {
      // not listening yet
    }
    await sleep(500);
  }
  server.kill();
  fail(`the server did not start; see ${path.join(work, 'server.log')}`);
}

// ---- HTTP API (administrator side) ------------------------------------------------

const SECRET_KEYS = ['access_token', 'refresh_token', 'password', 'codes', 'token'];

function redact(v) {
  if (Array.isArray(v)) return v.map(redact);
  if (v && typeof v === 'object') {
    return Object.fromEntries(
      Object.entries(v).map(([k, x]) => [
        k,
        SECRET_KEYS.includes(k) && x != null ? '<redacted>' : redact(x),
      ])
    );
  }
  if (typeof v === 'string' && v.length > 44 && !v.includes(' ')) {
    return `${v.slice(0, 12)}...(${v.length} chars)`;
  }
  return v;
}

function brief(v) {
  const keep = ['warnings', 'issues', 'factors', 'code', 'message', 'state', 'details'];
  if (!v || typeof v !== 'object' || JSON.stringify(v).length <= 600) return v;
  return Object.fromEntries(
    Object.entries(v)
      .filter(([k, x]) => keep.includes(k) || typeof x !== 'object' || x === null)
      .map(([k, x]) => [k, x])
  );
}

async function api(method, p, { token, body, raw, headers = {} } = {}) {
  const h = { ...headers };
  if (token) h.Authorization = `Bearer ${token}`;
  let payload;
  if (raw) {
    payload = raw;
    h['Content-Type'] = 'application/octet-stream';
  } else if (body !== undefined) {
    payload = JSON.stringify(body);
    h['Content-Type'] = 'application/json';
  }
  const r = await fetch(`${BASE}${p}`, { method, headers: h, body: payload });
  const text = await r.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    json = text;
  }
  const shownHeaders = Object.entries(headers)
    .map(([k, v]) => `> ${k}: ${k.includes('password') ? '<redacted>' : v}`)
    .join('\n');
  out('```text');
  out(`> ${method} ${p}${token ? ' (Bearer <redacted>)' : ''}`);
  if (shownHeaders) out(shownHeaders);
  if (raw) out(`> <.ovxp, ${raw.length} bytes>`);
  else if (body !== undefined) out(`> ${JSON.stringify(redact(body))}`);
  out(`< ${r.status}`);
  if (json !== null) out(`< ${JSON.stringify(brief(redact(json)))}`);
  out('```');
  return { status: r.status, body: json };
}

function expect(cond, message) {
  if (!cond) fail(message);
}

// ---- Chrome DevTools protocol --------------------------------------------------

class Cdp {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.next = 1;
    this.pending = new Map();
    this.api = [];
    this.requests = new Map();
    this.ws.onmessage = (m) => {
      const d = JSON.parse(m.data);
      if (d.id && this.pending.has(d.id)) {
        const { resolve, reject } = this.pending.get(d.id);
        this.pending.delete(d.id);
        if (d.error) reject(new Error(`${d.error.message} (${JSON.stringify(d.error)})`));
        else resolve(d.result);
      } else if (d.method === 'Network.requestWillBeSent') {
        this.requests.set(d.params.requestId, d.params.request.method);
      } else if (d.method === 'Network.responseReceived') {
        const u = new URL(d.params.response.url);
        if (u.pathname.startsWith('/api/v1/')) {
          const method = this.requests.get(d.params.requestId) || '?';
          this.api.push(`${method} ${u.pathname} -> ${d.params.response.status}`);
        }
      }
    };
  }
  open() {
    return new Promise((resolve, reject) => {
      this.ws.onopen = resolve;
      this.ws.onerror = reject;
    });
  }
  send(method, params = {}) {
    const id = this.next++;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
  }
  async eval(expression) {
    const r = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r.exceptionDetails) fail(`page script failed: ${r.exceptionDetails.text}`);
    return r.result.value;
  }
  async waitFor(expression, what, timeoutMs = 20000) {
    const until = Date.now() + timeoutMs;
    while (Date.now() < until) {
      if (await this.eval(`Boolean(${expression})`)) return;
      await sleep(200);
    }
    const text = await this.eval('document.body ? document.body.innerText.slice(0, 600) : ""');
    fail(`timed out waiting for ${what}; page text: ${text}`);
  }
  /** API calls the page made since the last flush (method, path, status). */
  flush(title) {
    if (this.api.length) {
      out(`Browser API calls (${title}):`);
      out('```text');
      for (const line of this.api) out(line);
      out('```');
    }
    this.api = [];
  }
}

async function startChrome(work) {
  const chrome = spawn(
    CHROME,
    [
      '--headless=new',
      `--remote-debugging-port=${CDP_PORT}`,
      `--user-data-dir=${path.join(work, 'chrome')}`,
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-gpu',
      'about:blank',
    ],
    { stdio: 'ignore' }
  );
  for (let i = 0; i < 60; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${CDP_PORT}/json/list`)).json();
      const page = list.find((t) => t.type === 'page');
      if (page) {
        const cdp = new Cdp(page.webSocketDebuggerUrl);
        await cdp.open();
        return { chrome, cdp };
      }
    } catch {
      // not ready yet
    }
    await sleep(250);
  }
  chrome.kill();
  fail('Chrome did not start');
}

// Page helpers: React reads the value through the native setter and an event.
const PAGE_HELPERS = `
window.__ovx = {
  set(el, value) {
    const proto = Object.getPrototypeOf(el);
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
    el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true }));
  },
  button(text, scope) {
    const root = scope ? document.querySelector(scope) : document;
    return [...(root ? root.querySelectorAll('button') : [])].find(
      (b) => b.textContent.trim() === text && !b.disabled
    );
  },
  click(text, scope) {
    const b = this.button(text, scope);
    if (!b) throw new Error('no enabled button ' + text);
    b.click();
    return true;
  },
  has(text) {
    return document.body.innerText.includes(text);
  },
  alerts() {
    return [...document.querySelectorAll('[role=alert],[role=status]')]
      .map((e) => e.innerText.trim())
      .filter(Boolean);
  },
};
true`;

async function goto(cdp, p) {
  await cdp.send('Page.navigate', { url: `${BASE}${p}` });
  await sleep(300);
  await cdp.waitFor('document.readyState === "complete"', `${p} to load`);
  await cdp.eval(PAGE_HELPERS);
}

// The server limits authentication endpoints per address (1/s, burst 5);
// a sign-in with a security key uses 3 (login, begin, verify).
const AUTH_REFILL_MS = 6000;

async function uiLogin(cdp, username, password) {
  await sleep(AUTH_REFILL_MS);
  await goto(cdp, '/login');
  await cdp.waitFor('document.querySelector("#username")', 'the login form');
  await cdp.eval(`__ovx.set(document.querySelector('#username'), ${JSON.stringify(username)})`);
  await cdp.eval(`__ovx.set(document.querySelector('#password'), ${JSON.stringify(password)})`);
  await cdp.eval('document.querySelector("button[type=submit]").click(), true');
}

/** On a challenge (page or dialog): pick the security key, run the ceremony. */
async function uiUseSecurityKey(cdp, scope = 'body') {
  const sc = JSON.stringify(scope);
  await cdp.waitFor(
    `document.querySelector(${sc} + ' select option[value="${FACTOR}"]') || __ovx.button('Use security key', ${sc})`,
    'the factor choice'
  );
  await cdp.eval(`(() => {
    const s = [...document.querySelectorAll(${sc} + ' select')].find((x) =>
      [...x.options].some((o) => o.value === ${JSON.stringify(FACTOR)}));
    if (s) __ovx.set(s, ${JSON.stringify(FACTOR)});
    return true;
  })()`);
  await cdp.waitFor(`__ovx.button('Use security key', ${sc})`, 'the security key button');
  await cdp.eval(`__ovx.click('Use security key', ${sc})`);
}

async function uiLogout(cdp, username) {
  await cdp.eval(`(() => {
    const menu = [...document.querySelectorAll('button')].find((b) =>
      b.textContent.trim() === ${JSON.stringify(username)});
    if (menu) menu.click();
    return true;
  })()`);
  await cdp.waitFor("__ovx.button('Logout')", 'the logout entry');
  await cdp.eval("__ovx.click('Logout')");
  await cdp.waitFor('location.pathname === "/login"', 'the login page after logout');
}

function claimsOfToken(token) {
  const c = JSON.parse(Buffer.from(token.split('.')[1], 'base64url').toString());
  return { amr: c.amr, acr: c.acr };
}

const AUTHENTICATOR = {
  protocol: 'ctap2',
  transport: 'usb',
  hasResidentKey: false,
  hasUserVerification: true,
  isUserVerified: true,
  automaticPresenceSimulation: true,
};

/** The credentials of a virtual authenticator, without their private keys. */
async function credentialsOf(cdp, authenticatorId) {
  const r = await cdp.send('WebAuthn.getCredentials', { authenticatorId });
  return r.credentials;
}

function describe(creds) {
  return creds
    .map((c) => `id ${c.credentialId.slice(0, 10)}... rpId ${c.rpId} signCount ${c.signCount}`)
    .join('; ');
}

// ---- the run ----------------------------------------------------------------------

async function main() {
  expect(
    fs.existsSync(path.join(ROOT, 'frontend/dist/index.html')),
    'run `npm run build` in frontend/ first'
  );
  expect(
    fs.existsSync(path.join(ROOT, 'target/debug/openvox-webui')),
    'run `cargo build` first'
  );
  const work = fs.mkdtempSync(path.join(os.tmpdir(), 'ovx-e2e-fido2-'));
  const pkgPath = path.join(work, 'mfa-fido2.ovxp');
  const exp = spawnSync(
    'cargo',
    ['test', '--test', 'integration_tests', 'export_the_standin_package', '--', '--ignored'],
    {
      cwd: ROOT,
      env: { ...process.env, OVX_E2E_EXPORT: pkgPath, CARGO_INCREMENTAL: '0' },
      encoding: 'utf8',
    }
  );
  expect(exp.status === 0 && fs.existsSync(pkgPath), `exporting the package failed: ${exp.stderr}`);
  const pkg = fs.readFileSync(pkgPath);

  const { server, db } = await startServer(work);
  let chrome;
  try {
    out('## Transcript (real server, built web UI, Chrome with a CDP virtual authenticator)');
    out();
    out(`Server: target/debug/openvox-webui on ${BASE}, web UI from frontend/dist, fresh database.`);
    out(`Package: mfa-fido2 0.1.0 stand-in, ${pkg.length} bytes, signed with a key made for this run.`);
    sqlite(
      db,
      `UPDATE users SET password_hash = '${argon2Hash(ADMIN_PASSWORD)}', ` +
        "force_password_change = 0 WHERE username = 'admin'"
    );
    out('The administrator password is set in the fresh database (seeded hash is a placeholder).');

    out('\n### B1. Administrator installs, approves, configures and enables the plugin (HTTP API)');
    let r = await api('POST', '/api/v1/auth/login', {
      body: { username: 'admin', password: ADMIN_PASSWORD },
    });
    expect(r.status === 200, 'admin login');
    const admin = r.body.access_token;
    r = await api('POST', '/api/v1/plugins/preflight', { token: admin, raw: pkg });
    expect(r.status === 200 && r.body.can_install, 'preflight');
    const keyId = r.body.signature.key_id;
    r = await api('POST', '/api/v1/plugins', {
      token: admin,
      raw: pkg,
      headers: {
        'x-trust-key-confirm': keyId.slice(0, 8),
        'x-trust-key-label': 'mfa-fido2 browser e2e',
        'x-reauth-password': ADMIN_PASSWORD,
      },
    });
    expect(r.status === 201, 'install');
    const caps = r.body.capabilities.map((c) => c.name);
    r = await api('PUT', '/api/v1/plugins/mfa-fido2/capabilities', {
      token: admin,
      body: { capabilities: caps },
    });
    expect(r.status === 200, 'approve');
    r = await api('PUT', '/api/v1/plugins/mfa-fido2/config', {
      token: admin,
      body: { rp: { id: 'localhost', name: 'OpenVox WebUI', origins: [BASE] } },
    });
    expect(r.status === 200, 'config');
    r = await api('POST', '/api/v1/plugins/mfa-fido2/enable', { token: admin });
    expect(r.status === 200 && r.body.state === 'enabled', 'enable');
    r = await api('POST', '/api/v1/users', {
      token: admin,
      body: { username: 'alice', email: 'alice@example.com', password: ALICE_PASSWORD, role: 'viewer' },
    });
    expect(r.status === 201, 'create alice');

    const started = await startChrome(work);
    chrome = started.chrome;
    const cdp = started.cdp;
    await cdp.send('Page.enable');
    await cdp.send('Runtime.enable');
    await cdp.send('Network.enable');
    await cdp.send('WebAuthn.enable', { enableUI: false });
    const { authenticatorId: keyA } = await cdp.send('WebAuthn.addVirtualAuthenticator', {
      options: AUTHENTICATOR,
    });
    out(
      `\nVirtual authenticator A: ${JSON.stringify(AUTHENTICATOR)} (CDP WebAuthn.addVirtualAuthenticator).`
    );

    out('\n### B2. alice signs in with her password in the web UI (no policy yet)');
    await uiLogin(cdp, 'alice', ALICE_PASSWORD);
    await cdp.waitFor('location.pathname !== "/login"', 'the signed-in page');
    out(`UI: signed in, now at ${await cdp.eval('location.pathname')}.`);
    cdp.flush('password sign-in');

    out('\n### B3. alice enrols a security key from her profile (navigator.credentials.create)');
    await goto(cdp, '/profile');
    await cdp.waitFor("__ovx.has('Security keys and passkeys')", 'the security keys section');
    await cdp.eval(
      `__ovx.set(document.querySelector('input[placeholder="For example: Blue key"]'), 'Browser key A')`
    );
    await cdp.eval("__ovx.click('Add security key')");
    await cdp.waitFor("__ovx.has('Save your recovery codes')", 'the recovery codes');
    const codes = await cdp.eval(
      "[...document.querySelectorAll('code, li')].filter((e) => /^[A-Z0-9]{5}-[A-Z0-9]{5}$/.test(e.innerText.trim())).length"
    );
    out(`UI: "Save your recovery codes" shown with ${codes} codes (values not recorded).`);
    await cdp.eval("__ovx.click('I have saved these codes')");
    await cdp.waitFor("__ovx.has('Browser key A')", 'the new key in the list');
    await cdp.waitFor("__ovx.has('Registered security keys')", 'the plugin panel');
    out('UI: "Browser key A" is listed by the host section and by the plugin panel (profile.security).');
    out(`Authenticator A: ${describe(await credentialsOf(cdp, keyA))}.`);
    cdp.flush('enrolment');

    out('\n### B4. Step-up: adding a second key opens the step-up dialog');
    await cdp.eval(
      `__ovx.set(document.querySelector('input[placeholder="For example: Blue key"]'), 'Browser key B')`
    );
    await cdp.eval("__ovx.click('Add security key')");
    await cdp.waitFor(`__ovx.has("Confirm it's you")`, 'the step-up dialog');
    out(`UI: step-up dialog "Confirm it's you" opened after 401 step_up_required.`);
    await uiUseSecurityKey(cdp, '[role=dialog]');
    await cdp.waitFor("!document.querySelector('[role=dialog]')", 'the dialog to close');
    await cdp.waitFor(
      "__ovx.has('already registered') || __ovx.has('Security key added.')",
      'the retried enrolment'
    );
    out(`UI after step-up, retried call with the same authenticator: ${JSON.stringify(await cdp.eval('__ovx.alerts()'))}`);
    const token1 = await cdp.eval("localStorage.getItem('auth_token')");
    out(`Session after step-up: ${JSON.stringify(claimsOfToken(token1))}.`);
    cdp.flush('step-up');
    // A second, empty authenticator stands for the second key; A is kept in memory.
    const savedA = (await credentialsOf(cdp, keyA))[0];
    await cdp.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId: keyA });
    const { authenticatorId: keyB } = await cdp.send('WebAuthn.addVirtualAuthenticator', {
      options: AUTHENTICATOR,
    });
    out('Authenticator A is unplugged (its credential kept in memory) and an empty authenticator B is plugged in.');
    await cdp.eval("__ovx.click('Try again') || true").catch(() => cdp.eval("__ovx.click('Add security key')"));
    await cdp.waitFor("__ovx.has('Browser key B')", 'the second key in the list');
    out('UI: "Browser key B" added without a new step-up (the session holds level 2).');
    out(`Authenticator B: ${describe(await credentialsOf(cdp, keyB))}.`);
    cdp.flush('second key');

    out('\n### B5. alice signs out from the user menu');
    await uiLogout(cdp, 'alice');
    out(`UI: back on /login; stored token: ${(await cdp.eval("localStorage.getItem('auth_token')")) === null ? 'none' : 'still present'}.`);
    r = await api('GET', '/api/v1/users/me/factors', { token: token1 });
    expect(r.status === 401, 'old token after logout');
    cdp.flush('logout');

    out('\n### B6. Administrator requires a second factor; alice signs in with her key');
    r = await api('PUT', '/api/v1/auth/policies', {
      token: admin,
      body: { policies: [{ name: 'everyone', conditions: {}, required_acr: 2 }] },
    });
    expect(r.status === 200, 'policy');
    await uiLogin(cdp, 'alice', ALICE_PASSWORD);
    await uiUseSecurityKey(cdp);
    await cdp.waitFor('location.pathname !== "/login"', 'the signed-in page');
    const token2 = await cdp.eval("localStorage.getItem('auth_token')");
    out(`UI: signed in with the security key; session ${JSON.stringify(claimsOfToken(token2))}.`);
    out(`Authenticator B: ${describe(await credentialsOf(cdp, keyB))}.`);
    cdp.flush('login challenge');

    out('\n### B7. Negative: the authenticator signs with the wrong private key');
    await uiLogout(cdp, 'alice');
    await uiLogin(cdp, 'alice', ALICE_PASSWORD);
    const good = (await credentialsOf(cdp, keyB))[0];
    const wrongKey = crypto
      .generateKeyPairSync('ec', { namedCurve: 'prime256v1' })
      .privateKey.export({ type: 'pkcs8', format: 'der' })
      .toString('base64');
    await cdp.send('WebAuthn.removeCredential', { authenticatorId: keyB, credentialId: good.credentialId });
    await cdp.send('WebAuthn.addCredential', {
      authenticatorId: keyB,
      credential: {
        credentialId: good.credentialId,
        isResidentCredential: false,
        rpId: good.rpId,
        privateKey: wrongKey,
        signCount: good.signCount,
      },
    });
    out('Credential B is replaced in the authenticator by one with the same id and another P-256 key (CDP WebAuthn.addCredential).');
    await uiUseSecurityKey(cdp);
    await cdp.waitFor("__ovx.has('was not accepted')", 'the rejection');
    out(`UI: ${JSON.stringify(await cdp.eval('__ovx.alerts()'))}`);
    const attempts = sqlite(
      db,
      "SELECT attempts FROM auth_challenges WHERE consumed_at IS NULL ORDER BY created_at DESC LIMIT 1"
    );
    const reason = sqlite(
      db,
      "SELECT json_extract(details, '$.reason') FROM audit_log WHERE action = 'plugin.mfa-fido2.credential.verification_failed' ORDER BY rowid DESC LIMIT 1"
    );
    out(`auth_challenges.attempts = ${attempts}; plugin audit reason = ${reason}.`);
    cdp.flush('wrong signature');
    await cdp.send('WebAuthn.removeCredential', { authenticatorId: keyB, credentialId: good.credentialId });
    await cdp.send('WebAuthn.addCredential', {
      authenticatorId: keyB,
      credential: { ...good, signCount: good.signCount },
    });
    out('The genuine credential B is put back; alice tries again on the same challenge.');
    await cdp.waitFor("__ovx.button('Try again') || __ovx.button('Use security key')", 'the retry');
    await sleep(AUTH_REFILL_MS);
    await cdp.eval("__ovx.button('Try again') ? __ovx.click('Try again') : __ovx.click('Use security key')");
    await cdp.waitFor('location.pathname !== "/login"', 'the signed-in page');
    out('UI: signed in after the retry.');
    cdp.flush('retry');

    out('\n### B8. Fail closed');
    const mine = await api('GET', '/api/v1/users/me/factors', {
      token: await cdp.eval("localStorage.getItem('auth_token')"),
    });
    const recovery = mine.body.factors.find((f) => f.kind === 'recovery');
    r = await api('DELETE', `/api/v1/users/me/factors/${recovery.id}`, {
      token: await cdp.eval("localStorage.getItem('auth_token')"),
    });
    expect(r.status === 200, 'remove recovery codes');
    out('alice removed her recovery codes (stepped-up session), so her security keys are her only factor.');
    await uiLogout(cdp, 'alice');

    out('\n#### B8a. Outage: auth:factor withdrawn while the plugin stays enabled');
    r = await api('PUT', '/api/v1/plugins/mfa-fido2/capabilities', {
      token: admin,
      body: { capabilities: caps.filter((c) => c !== 'auth:factor') },
    });
    expect(r.status === 200, 'withdraw');
    await uiLogin(cdp, 'alice', ALICE_PASSWORD);
    await uiUseSecurityKey(cdp);
    await cdp.waitFor("__ovx.has('unavailable')", 'the unavailable message');
    out(`UI: ${JSON.stringify(await cdp.eval('__ovx.alerts()'))}; still on ${await cdp.eval('location.pathname')}.`);
    cdp.flush('outage');
    r = await api('PUT', '/api/v1/plugins/mfa-fido2/capabilities', {
      token: admin,
      body: { capabilities: caps },
    });
    expect(r.status === 200, 'grant again');

    out('\n#### B8b. The administrator disables the plugin');
    r = await api('POST', '/api/v1/plugins/mfa-fido2/disable', { token: admin });
    expect(r.status === 200, 'disable');
    await uiLogin(cdp, 'alice', ALICE_PASSWORD);
    await cdp.waitFor('document.querySelector("[role=alert]")', 'the sign-in error');
    out(`UI: ${JSON.stringify(await cdp.eval('__ovx.alerts()'))}; still on ${await cdp.eval('location.pathname')}.`);
    cdp.flush('disabled plugin');

    out('\n#### B8c. Break-glass: the local super administrator still signs in');
    await sleep(AUTH_REFILL_MS);
    r = await api('POST', '/api/v1/auth/login', { body: { username: 'admin', password: ADMIN_PASSWORD } });
    expect(r.status === 200, 'break-glass');
    out(
      `auth.break_glass audit entries: ${sqlite(db, "SELECT COUNT(*) FROM audit_log WHERE action = 'auth.break_glass'")}.`
    );

    out('\n#### B8d. Re-enabled, the keys work again (records were kept)');
    r = await api('POST', '/api/v1/plugins/mfa-fido2/enable', { token: admin });
    expect(r.status === 200, 're-enable');
    await uiLogin(cdp, 'alice', ALICE_PASSWORD);
    await uiUseSecurityKey(cdp);
    await cdp.waitFor('location.pathname !== "/login"', 'the signed-in page');
    out('UI: signed in with the security key.');
    out(`Authenticator B: ${describe(await credentialsOf(cdp, keyB))}.`);
    cdp.flush('re-enabled');
    out(`\nCredential A (unplugged) was kept only in this process's memory: ${savedA ? 'yes' : 'no'}.`);
    out('\nResult: every step passed.');
  } finally {
    if (chrome) chrome.kill();
    server.kill();
    if (process.env.OVX_E2E_OUT) fs.writeFileSync(process.env.OVX_E2E_OUT, log.join('\n') + '\n');
    if (!process.env.OVX_E2E_KEEP) fs.rmSync(work, { recursive: true, force: true });
    else console.error(`work directory kept: ${work}`);
  }
}

main().catch((e) => {
  console.error(`FAILED: ${e.message}`);
  process.exitCode = 1;
});
