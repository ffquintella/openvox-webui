# Orchestration API (OpenBolt)

Base path: `/api/v1/orchestration`. Design: [BOLT_INTEGRATION.md](../architecture/BOLT_INTEGRATION.md). All endpoints need a session or an API key and answer `503` while `orchestration.bolt.enabled` is `false` (except `GET /bolt/status`).

## Permissions

Resource `orchestration`, actions `read`, `create` (run), `approve` and `admin` (admin implies the others). Reading and approving need an unscoped grant. Running needs an unscoped grant, or a grant scoped to a group when the request targets that group (`group_id`) and lists no `nodes`. Administrators only: the allowlist, discovery, and `kind: "command"`.

## Endpoints

| Method and path | Permission | Description |
|-----------------|-----------|-------------|
| `GET /bolt/status` | read | Integration state (`disabled`, `ok`, `degraded`), Bolt version, limits and configuration warnings |
| `GET /bolt/tasks`, `GET /bolt/plans` | read | Allowlisted tasks or plans with their parameters (`type`, `description`, `sensitive`, `default`), `supports_noop` and `requires_approval` |
| `GET /bolt/discover` | admin | Every task and plan Bolt finds, with `allowed` and `requires_approval` |
| `GET /bolt/allowlist` | read | `{entries: [{kind, name, requires_approval, created_by, created_at}]}` |
| `PUT /bolt/allowlist` | admin | Replaces the allowlist: `{entries: [{kind: "task"\|"plan", name, requires_approval}]}` |
| `POST /bolt/jobs` | create (admin for `command`) | Requests a run; see below. `201` with the job |
| `GET /bolt/jobs` | read | Jobs, newest first. Query: `status`, `certname`, `limit` (default 100, at most 500) |
| `GET /bolt/jobs/{id}` | read | The job and its per-target results |
| `POST /bolt/jobs/{id}/approve` | approve | Optional body `{notes}`. Not allowed for your own request |
| `POST /bolt/jobs/{id}/reject` | approve | Optional body `{notes}` |
| `POST /bolt/jobs/{id}/cancel` | create (own job) or approve | Cancels a waiting job at once; stops Bolt for a running one |
| `GET /nodes/{certname}/jobs` | read | Run history of one node |

## Requesting a run

```json
{
  "kind": "task",
  "name": "package",
  "parameters": {"action": "status", "name": "nginx"},
  "nodes": ["web1.example.com"],
  "group_id": null,
  "noop": false,
  "maintenance_window_start": null,
  "maintenance_window_end": null
}
```

- `kind` is `task`, `plan` or `command` (`name` is then the command line; needs `allow_arbitrary_commands`).
- Targets are `nodes` plus the members of `group_id`. They must be active in PuppetDB, at most `max_targets_per_run`.
- The item must be on the allowlist and the parameters must match its metadata (required, types, no unknown names). `noop` needs a task that declares `supports_noop`.
- The run waits for approval when the allowlist entry asks for it, when it has more than `approval_target_threshold` targets, and always for commands. Another user with `orchestration:approve` decides.
- With a maintenance window the job stays queued until it opens, and fails if the window ends first.

Job status: `pending_approval`, `queued`, `running`, `succeeded`, `partial`, `failed`, `cancelled`, `timed_out`, `rejected`. Parameters in responses are redacted: declared `sensitive` parameters and names that look like secrets show `[REDACTED]`, and the stored copy is redacted when the job ends.

## Audit and events

Audit entries use `resource_type: "bolt_jobs"` and the actions `bolt_job.requested`, `.approved`, `.rejected`, `.cancelled`, `.started`, `.finished`, plus `bolt_allowlist.updated`. They list parameter *names* only. Events `bolt.job.requested`, `bolt.job.approved` and `bolt.job.completed` carry ids, status and counts.
