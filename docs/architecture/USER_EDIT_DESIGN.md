# Editing users, with plugin extension points — Design

Status: implemented (2026-11-10, T122); corrected after the builder check, see section 10

## 1. Intent

The Users page (`frontend/src/pages/Users.tsx`) lets an administrator create and delete users and assign roles, but not correct a user's details. Administrators need to fix a username or email, and to force a user to choose a new password. Plugins (PLUGIN_PLATFORM.md) must be able to extend this screen and take part in the save.

## 2. Current state

- `PUT /api/v1/users/{id}` already accepts `username`, `email`, `password`, `role`, `auth_provider` and `external_id`, with organization scoping. It had **no permission check** (any signed-in user could call it) and wrote no audit entry.
- `users.force_password_change` exists (migration `20241216000005`), is exposed on `UserPublic`, and the frontend shows `components/ForcePasswordChange.tsx` at login when it is set.
- The side panel in `Users.tsx` shows only roles and delete.
- The slot catalog (`src/services/plugins/slots.rs`) has node, group and report panels but nothing for users. The interceptor catalog (`src/services/plugins/interceptors.rs`) and the event bus (`events.rs`) exist.

## 3. Goals

1. An administrator can edit a user's username and email from the Users page.
2. An administrator can require a user to change their password at next login, optionally setting a temporary password in the same action.
3. Plugins can add panels and buttons to the user detail view, observe user updates, and veto a user update.

## 4. Non-goals and anti-goals

- No bulk edit. No change to the Profile page (self-service).
- Plugins cannot rewrite user fields; the interceptor can only allow or veto.
- No plugin-declared custom fields in core (possible later; plugins keep their own data in plugin storage).
- Anti-goal: a broken or slow plugin must never make user management unusable, and passwords must never reach plugins, logs or the audit trail.

## 5. Core behavior

1. **Edit form.** An Edit button in the user side panel switches username and email to an inline form. Save calls the existing `PUT /api/v1/users/{id}` with only the changed fields. Existing uniqueness and organization checks stay.
2. **Read-only users.** Users with `directory_id` set (directory or SCIM managed) are not editable; the button is hidden and the API rejects username/email changes for them with a clear error. (Builder call: verify the existing API behavior and add the guard only where missing.)
3. **Force password change.** `UpdateUserRequest` gains `force_password_change: Option<bool>`. The side panel offers a "Require password change at next login" switch, hidden for users whose `auth_provider` is `saml`. An optional temporary password goes in the same request; when a temporary password is given the flag defaults to true. The flag is cleared when the user completes a password change (verify this already happens in the change-password path).
4. **Authorization.** The same permission as the current update endpoint (users update). Administrators cannot lock themselves out: forcing a change on oneself is allowed, editing is not blocked.
5. **Audit.** Each update writes an audit-log entry naming the actor, the target and the list of changed fields. Password values are never recorded.

## 6. Plugin extension points

1. **Slots** `user.detail.panels` (panel) and `user.detail.actions` (action), context `{ user_id }`, requires permission, same render types as `group.detail.panels` and `node.detail.actions`. Registered in `slots.rs` and documented in PLUGIN_PLATFORM.md §8.1 and PLUGINS.md.
2. **Event** `user.updated`, notify-only, payload `{ user_id, organization_id, actor_id, changed_fields: [..] }`. No values of changed fields are sent beyond username and email, and never password data. Delivered through the existing event bus and webhooks.
3. **Interceptor** `before.user.update`, added to the interceptable catalog. Payload: `{ user_id, actor_id, current: {username, email}, proposed: {username, email}, force_password_change }`. Answer is allow or veto with a reason; a veto returns an error to the administrator showing the reason. Limits are the platform's: 50 ms timeout, at most 3 interceptors per event. Fail mode is fail-open (my call: a failing plugin must not block user administration). It runs only when username, email or the force-password flag changes, and never receives a password.
4. Capabilities follow the existing naming (`ui:slot:user.detail.panels`, `ui:slot:user.detail.actions`, `events:subscribe`, and the interceptor capability derived by `capability_of`).

## 7. Documentation and release

Update `CHANGELOG.md` (Unreleased: Added), `ROADMAP.md`, `docs/PLUGINS.md`, `docs/architecture/PLUGIN_PLATFORM.md` (§8.1 slot table, interceptor catalog), the user guide and API docs. English only.

## 8. Testing

Backend unit tests for the update validation, the read-only guard, flag handling, the interceptor veto and fail-open paths, and audit content. BDD scenarios in `tests/features/` for the admin flow. Frontend tests for the edit form and the hidden switch for SSO users.

## 9. Left to the builder

Form layout and validation messages, exact permission names, whether the slot hosts reuse a shared component from group/node detail, and the interceptor payload field naming within the contract above.

## 10. Decisions after the builder check

- **Permission**: `PUT /api/v1/users/{id}` now requires `users:admin` (the permission of the directory and SCIM administration). Creating and deleting users and assigning roles have the same gap and are left for a separate change.
- **Directory users**: the API already rejected identity and password changes for them; it now also rejects `force_password_change: true`, and the UI hides Edit.
- **SSO-only** (`auth_provider: saml`): a temporary password or a forced change is refused (`400`). Users with `both` can be forced; the flag applies to their local sign-in.
- **Temporary password**: at least 8 characters, as when creating a user; it implies `force_password_change: true` unless the request says otherwise. An empty password string is ignored.
- **Event**: `user.updated` already exists (SCIM). It is reused; administrator edits publish `{user_id, organization_id, username, source: "admin", changed_fields}` with the acting user as actor.
- **Interceptor**: `before.user.update` fails open (my call), runs after authorization and before storage (so before the uniqueness checks) and also when a temporary password is set (`password_changed: true`, never the password).
- **Audit**: written after a successful change as `user.updated` (changed field names only); a veto is audited by the interceptor runtime.
- **Not done**: revoking a user's active sessions or API keys when a change is forced (the user is asked at next login); `reset_password` still does not clear the flag; no handler-level test of an interceptor veto on this endpoint (the interceptor runtime is covered by its own tests).
