# Phase 1.3: RBAC Foundation

## Completed Tasks

- [x] Design permission model (resources, actions, scopes)
- [x] Implement Role data model
- [x] Implement Permission data model
- [x] Create default roles (Admin, Operator, Viewer, GroupAdmin, Auditor)
- [x] Implement permission checking middleware
- [x] Create RBAC database schema and migrations

## Details

Role-Based Access Control (RBAC) foundation provides the security framework for multi-user environments.

### Permission Model

**Resources:**
- nodes, groups, reports, facts, users, roles, settings, audit_logs

**Actions:**
- read, create, update, delete, admin, execute, export

**Scopes:**
- all (system-wide)
- environment (environment-based)
- group (group-specific)
- owned (user-owned resources)
- specific (specific resource IDs)

### Default Roles

| Role | Purpose | Scope |
|------|---------|-------|
| **Admin** | Full system access | All resources, all actions |
| **Operator** | Daily operations | Nodes, groups, reports read/write |
| **Viewer** | Read-only access | All resources, read-only |
| **GroupAdmin** | Group management | Assigned groups, full control |
| **Auditor** | Compliance audit | All resources, read/audit logs |

### RBAC Architecture

- `src/models/role.rs` - Role data model
- `src/models/permission.rs` - Permission data model
- `src/services/rbac.rs` - RBAC service layer
- `src/middleware/permission.rs` - Permission checking middleware
- Database tables: roles, permissions, role_permissions, user_roles

### Database Schema

- `migrations/20241216000002_rbac.sql` - Initial RBAC schema
- Role definitions with metadata
- Permission definitions with resource/action/scope
- Role-permission associations
- User-role associations with organization scoping

### Permission Checking

Middleware validates requests against user roles and permissions before reaching handlers. Supports:
- Resource-level permissions
- Action-level permissions
- Scope-based access control
- Tenant isolation (multi-tenancy)

## Key Files

- `src/models/role.rs` - Role model
- `src/models/permission.rs` - Permission model
- `src/services/rbac.rs` - RBAC logic
- `src/middleware/permission.rs` - Permission middleware
- `migrations/20241216000002_rbac.sql` - Schema
