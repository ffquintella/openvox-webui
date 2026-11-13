# OpenBolt Integration — Design

Status: implemented (2026-11-13, M32, T113 and T123 to T128); section 15 lists where the implementation differs from the first draft

## 1. Intent

Operators of an OpenVox fleet often need to act on nodes outside the Puppet run cycle: restart a service, apply a one-off fix, run a vendor task, or run a multi-step plan across a group. [OpenBolt](https://github.com/OpenVoxProject/openbolt) does this agentlessly over SSH and WinRM, using the same modules and the same node inventory the fleet already has in PuppetDB.

This design adds an **Orchestration** area to the WebUI. It lets authorized users run Bolt tasks and plans against nodes or node groups, with approval, limits, audit and result history, and it keeps the same safety posture as the update orchestration of M17 (S20).

## 2. Current state

- Node inventory and groups come from PuppetDB and the classification engine (`src/services/puppetdb.rs`, `classification.rs`).
- External tools are driven as subprocesses with timeouts and captured output; `src/services/r10k.rs` is the pattern to follow.
- Update jobs (migration `20260319000003_phase10_update_orchestration.sql`) already model request, approval, per-node execution, results, maintenance windows and concurrency limits (T58 to T63), and write entries through `update_audit.rs`.
- `secrets.rs` resolves `secret://provider/path` references and redacts values.
- RBAC is `Resource` x `Action` with scopes (all, specific, environment, group, self, owned) in `src/models/rbac.rs`.
- Nothing runs commands on nodes from the UI today; updates are polled or queued through Puppet.

## 3. Goals

1. Run an allowed Bolt **task** or **plan** against a node, a node group, or a classification-rule result, from the UI and the API.
2. Generate the Bolt inventory from PuppetDB and group membership, so operators never maintain a second inventory.
3. Gate execution with RBAC (group-scoped), optional approval, concurrency limits and maintenance windows.
4. Keep complete, redacted, queryable history: who ran what, against which targets, with what outcome.
5. Let plugins observe and veto runs through the existing events and interceptors.

## 4. Non-goals and anti-goals

- No arbitrary shell. `bolt command run` is **off by default** and sits behind a configuration switch, the admin permission, and a mandatory approval. `bolt script run` is not supported.
- No Bolt project editing, no module installation from the UI (modules come from r10k and the code deploy feature).
- No Puppet Enterprise orchestrator (PCP) support in the first release; transports are SSH and WinRM.
- No streaming terminal or interactive sessions.
- Anti-goal: credentials never reach the browser, plugins, logs or the audit trail. A failed or hanging Bolt run must never block the API or exhaust the host.

## 5. Architecture

### 5.1 Execution model

A new service `src/services/bolt.rs` wraps the `bolt` binary, like `r10k.rs`:

1. Resolve targets to a list of certnames (node, group, or rule), apply RBAC scoping, and drop nodes the caller may not touch.
2. Render a **per-run inventory file** (`inventory.yaml`, mode 0600) in a private temporary directory, with `config` entries for transport, user and a credential *reference* resolved at spawn time.
3. Spawn `bolt task run <name> --targets <list> --params <json> --format json` or `bolt plan run ...` with `--project <configured project dir>`, a hard timeout and `--concurrency <n>`. Parameters are passed through stdin or a 0600 params file, never as command-line arguments.
4. Parse the JSON result per target, store it, delete the temporary directory.

The process is run by a bounded worker pool so that the number of simultaneous Bolt processes is capped (section 7).

### 5.2 Configuration

New `orchestration.bolt` block in `config.yaml`:

```yaml
orchestration:
  bolt:
    enabled: false
    binary_path: /opt/puppetlabs/bolt/bin/bolt
    project_dir: /etc/openvox-webui/bolt
    modulepath: ["/etc/puppetlabs/code/environments/production/modules"]
    transports:
      ssh:   { user: bolt, private_key: "secret://file/bolt/ssh_key", host_key_check: true }
      winrm: { user: bolt, password: "secret://file/bolt/winrm_password", ssl: true }
    timeout_seconds: 900
    max_concurrent_runs: 4
    max_targets_per_run: 200
    allow_arbitrary_commands: false
    retention_days: 90
```

All credentials use `secret://` references (M28). The feature is disabled until configured, and a missing binary yields a clear health warning, not a crash.

### 5.3 Inventory generation

Targets are built from PuppetDB: `certname`, `facts.networking.fqdn`, `facts.kernel` (selects SSH or WinRM), plus node groups as Bolt groups. Connection address defaults to the certname, with an optional per-group override variable. Inventory is rendered per run from the targets actually selected, so it is always current and never contains nodes outside the run.

### 5.4 Allowed tasks and plans

Discovery uses `bolt task show --format json` and `bolt plan show --format json` against the configured modulepath, cached with the existing cache layer. A task or plan is runnable only if it is on the **allowlist**: an administrator enables items explicitly (default deny), optionally pinning allowed parameter names and a per-item approval requirement. Parameters are validated against the task metadata (types, required, `sensitive`) before spawn; `sensitive` parameters are redacted in storage.

## 6. Data model

New migration, tables scoped by `organization_id` like the rest of the schema:

| Table | Purpose |
|-------|---------|
| `bolt_allowlist` | enabled task/plan name, kind, approval flag, allowed parameter schema snapshot |
| `bolt_jobs` | id, kind (`task`, `plan`), name, requested_by, approval state, approver, parameters (redacted), target selector, status, timestamps |
| `bolt_job_targets` | job id, certname, status, exit code, started and finished times |
| `bolt_job_results` | job id, certname, result JSON (redacted, size-capped), stdout/stderr excerpt |

Job status: `pending_approval`, `queued`, `running`, `succeeded`, `partial`, `failed`, `cancelled`, `timed_out`. The state machine and approval fields mirror the update jobs so the same UI components and reports can be reused.

## 7. Safety controls

1. **Authorization.** New resource `orchestration` with actions `read`, `create` (run), `approve` and `admin` (allowlist, settings). Group-scoped grants limit which nodes a role can target; a run touching an out-of-scope node is rejected whole, not trimmed silently.
2. **Approval.** Items flagged on the allowlist, runs above a target-count threshold, and runs against groups marked critical require approval by a different user. Self-approval is not allowed.
3. **Limits.** `max_targets_per_run`, `max_concurrent_runs`, Bolt `--concurrency`, per-run timeout, and the maintenance windows of T63 for scheduled runs.
4. **Dry run.** Plans that support `noop` (and tasks that declare `supports_noop`) can be run with a noop switch first; the UI offers it before a real run.
5. **Host key and TLS.** SSH host key checking and WinRM TLS are on by default; turning them off is a configuration choice that raises a startup warning.
6. **Cancellation.** A running job can be cancelled; the process group is killed and unfinished targets are marked `cancelled`.
7. **Output handling.** Results are size-capped, `sensitive` parameters and values matching secret patterns are redacted before storage, and stored output is plain text rendered escaped in the UI.

## 8. API

```
GET    /api/v1/orchestration/bolt/status
GET    /api/v1/orchestration/bolt/tasks            # allowed tasks with metadata
GET    /api/v1/orchestration/bolt/plans
PUT    /api/v1/orchestration/bolt/allowlist        # admin
POST   /api/v1/orchestration/bolt/jobs             # create (task or plan)
GET    /api/v1/orchestration/bolt/jobs
GET    /api/v1/orchestration/bolt/jobs/:id
POST   /api/v1/orchestration/bolt/jobs/:id/approve
POST   /api/v1/orchestration/bolt/jobs/:id/reject
POST   /api/v1/orchestration/bolt/jobs/:id/cancel
GET    /api/v1/nodes/:certname/orchestration       # run history for a node
```

Creating a job accepts `{ kind, name, parameters, targets: { nodes | group_id }, noop, schedule_at? }` and returns the job in `pending_approval` or `queued`.

## 9. Frontend

- **Orchestration page** with tabs: Run (pick task or plan, fill a form generated from the metadata, choose targets, noop, submit), Jobs (list with status and filters), Allowlist (admin).
- **Job detail**: per-target results with status, exit code and output, live progress by polling.
- **Node detail** gets a "Run task" action and a history panel.
- Existing approval inbox from M17 is extended to show Bolt jobs.

## 10. Audit, events and plugins

- Every create, approve, reject, cancel, run start and finish writes an audit entry (actor, job, targets count and list, parameter *names* only).
- Events on the bus: `bolt.job.requested`, `bolt.job.approved`, `bolt.job.completed`, notify-only, delivered to webhooks. Payloads carry ids, status and counts, never parameter values or output.
- Interceptor `before.bolt.job.run` (allow or veto, fail-closed, because this is an action on nodes; the platform limits of 50 ms and 3 interceptors apply). It receives the task name, target list and parameter names.
- Slots `node.detail.actions` is reused; a new slot `orchestration.job.panels` allows plugins to add job detail panels.
- Capabilities follow the existing naming and are declared in plugin manifests.

## 11. Packaging and operations

- The RPM and DEB packages do **not** bundle OpenBolt; the documentation lists installing it and creating the service account and SSH key. The Puppet module gains optional parameters to manage the `orchestration.bolt` block.
- Health endpoint reports Bolt as `disabled`, `ok` or `degraded` (binary missing, project missing).
- Retention job deletes `bolt_job_results` older than `retention_days`.

## 12. Phases and tasks

| Task | Scope |
|------|-------|
| T113 | Integrate OpenBolt: `bolt.rs` service, configuration, process control, health |
| T123 | Inventory generation from PuppetDB and groups |
| T124 | Job model, migration, task/plan discovery, allowlist, run API |
| T125 | RBAC resource, approvals, limits, maintenance windows, cancellation |
| T126 | Audit, events, interceptor and slots |
| T127 | Orchestration UI and node actions |
| T128 | Retention, packaging, Puppet module parameters, documentation, BDD coverage |

Order: T113, T123, T124, T125 in sequence; T126 and T127 can follow T124 in parallel; T128 closes the milestone.

## 13. Testing

Unit tests for inventory rendering, parameter validation and redaction, the job state machine, and result parsing. A fake `bolt` script (shell) drives integration tests for timeouts, cancellation, partial failure and malformed JSON, so CI never needs real nodes. BDD scenarios in `tests/features/orchestration.feature` for the run, approval and denied-scope flows. Frontend tests for the generated parameter form and the job detail view.

## 14. Open questions

1. Should plan runs be restricted to plans without `run_command`/`run_script` steps, or is the allowlist (reviewed by an administrator) a sufficient control?
2. Is SSH key per organization needed for multi-tenant installs, or is one service key per installation acceptable in the first release?
3. Should results be exportable to PuppetDB as facts or events, or stay local to the WebUI?
4. Is PCP (OpenVox/Puppet Server orchestrator) transport worth a later milestone for agents that cannot be reached over SSH or WinRM?
5. Where should the Bolt project live in package installs, and who owns its Puppetfile and modules?

## 15. As implemented

Decisions taken while implementing, where they differ from sections 5 to 11 above (the code is `src/services/bolt/`, `src/api/orchestration.rs`, migration `20261113000001_bolt_orchestration.sql`; API reference in [../api/orchestration.md](../api/orchestration.md)):

1. **Secrets.** `secret://` references are resolved when the configuration is loaded, so the config holds resolved values and cannot carry a key file's content. The SSH key is therefore a path (`ssh.private_key_path`); only passwords may be references. `Debug` output of the transport settings redacts passwords.
2. **Inventory.** The per-run inventory lists only the selected nodes, with the transport chosen by the `kernel` fact (Windows: WinRM, else SSH). Node groups are not exported as Bolt groups and there is no per-group address override; the certname is the address. Names and addresses are validated (`[A-Za-z0-9._-]`) before use.
3. **Tables.** Three, not four: `bolt_allowlist`, `bolt_jobs` and `bolt_job_targets` (the per-target result and output live in the targets table). The job keeps the real parameters while it waits and runs, because an approved job must still be executable; when it ends (or fails, or the application restarts) they are replaced by their redacted copy, and every API read returns the redacted form.
4. **Approval.** A run needs approval when the allowlist entry asks for it, when it has more than `approval_target_threshold` targets, and always for commands. Critical-group approval is not implemented (groups have no such flag). Nobody approves their own request.
5. **Scheduling.** There is no `schedule_at`. A run can carry a maintenance window instead: it stays queued until the window opens and fails if the window ends first. Concurrency is the process-wide `max_concurrent_runs` pool (T63's per-job limits do not apply to Bolt jobs).
6. **Noop.** A task is run with `--noop` only if it declares `supports_noop`; plans receive the flag as given.
7. **Cancellation.** Cancelling a running job stops the Bolt process (it is killed when its future is dropped; Bolt's own children are not signalled separately). Unreported targets end `cancelled`.
8. **Group scope.** A role limited to a group can run against that group (`group_id`, no explicit `nodes`). Every other use of the feature needs an unscoped grant, including reading.
9. **Node history.** `GET /api/v1/orchestration/nodes/{certname}/jobs` (not under `/nodes`).
10. **Targets** must be active in PuppetDB; without PuppetDB the feature refuses to run anything, because the nodes cannot be checked.
11. **Status endpoint.** `GET /api/v1/orchestration/bolt/status` serves the health information (`disabled`, `ok`, `degraded`); it is not part of `/health` yet.
12. **Plugins.** Interceptor `before.bolt.job.run` (fail closed), events `bolt.job.requested|approved|completed`, slot `orchestration.job.panels`; see PLUGIN_PLATFORM.md §17.21.
13. **Packaging.** The Puppet module renders the `orchestration.bolt` block (`bolt_enabled` and related parameters); the RPM and DEB packages do not bundle OpenBolt.

