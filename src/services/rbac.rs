//! RBAC (Role-Based Access Control) service

use anyhow::{Context, Result};
use std::collections::HashSet;
use uuid::Uuid;

use crate::models::{
    Action, EffectivePermissions, Permission, PermissionCheck, PermissionConstraint,
    Resource, Role, Scope, SystemRole,
};

/// Service for managing roles and permissions
pub struct RbacService {
    /// All roles in the system (in production, this would be from database)
    roles: Vec<Role>,
    /// Cache of user permissions (user_id -> permissions)
    permission_cache: std::collections::HashMap<Uuid, Vec<Permission>>,
}

impl RbacService {
    /// Create a new RBAC service with default system roles
    pub fn new() -> Self {
        let mut service = Self {
            roles: vec![],
            permission_cache: std::collections::HashMap::new(),
        };

        // Initialize with system roles
        for system_role in SystemRole::all() {
            service.roles.push(system_role.to_role());
        }

        service
    }

    /// Create service with existing roles
    pub fn with_roles(roles: Vec<Role>) -> Self {
        Self {
            roles,
            permission_cache: std::collections::HashMap::new(),
        }
    }

    /// Get all roles
    pub fn get_roles(&self) -> &[Role] {
        &self.roles
    }

    /// Get a role by ID
    pub fn get_role(&self, id: Uuid) -> Option<&Role> {
        self.roles.iter().find(|r| r.id == id)
    }

    /// Get a role by name
    pub fn get_role_by_name(&self, name: &str) -> Option<&Role> {
        self.roles.iter().find(|r| r.name == name)
    }

    /// Create a new role
    pub fn create_role(&mut self, role: Role) -> Result<&Role> {
        // Validate role name is unique
        if self.roles.iter().any(|r| r.name == role.name) {
            anyhow::bail!("Role with name '{}' already exists", role.name);
        }

        // Validate parent exists if specified
        if let Some(parent_id) = role.parent_id {
            if !self.roles.iter().any(|r| r.id == parent_id) {
                anyhow::bail!("Parent role not found");
            }
        }

        self.roles.push(role);
        Ok(self.roles.last().unwrap())
    }

    /// Update a role
    pub fn update_role(&mut self, id: Uuid, updates: Role) -> Result<&Role> {
        let role = self.roles.iter_mut().find(|r| r.id == id)
            .context("Role not found")?;

        if role.is_system {
            anyhow::bail!("Cannot modify system roles");
        }

        role.display_name = updates.display_name;
        role.description = updates.description;
        role.parent_id = updates.parent_id;
        role.permissions = updates.permissions;
        role.updated_at = chrono::Utc::now();

        // Invalidate permission cache
        self.permission_cache.clear();

        Ok(role)
    }

    /// Delete a role
    pub fn delete_role(&mut self, id: Uuid) -> Result<()> {
        let role = self.roles.iter().find(|r| r.id == id)
            .context("Role not found")?;

        if role.is_system {
            anyhow::bail!("Cannot delete system roles");
        }

        self.roles.retain(|r| r.id != id);
        self.permission_cache.clear();
        Ok(())
    }

    /// Get effective permissions for a user based on their roles
    pub fn get_effective_permissions(&self, role_ids: &[Uuid]) -> EffectivePermissions {
        let mut all_permissions: HashSet<Permission> = HashSet::new();
        let mut role_names: Vec<String> = vec![];

        for role_id in role_ids {
            if let Some(role) = self.get_role(*role_id) {
                role_names.push(role.name.clone());

                // Add direct permissions
                for perm in &role.permissions {
                    all_permissions.insert(perm.clone());
                }

                // Add inherited permissions from parent roles
                let mut current_parent = role.parent_id;
                while let Some(parent_id) = current_parent {
                    if let Some(parent_role) = self.get_role(parent_id) {
                        for perm in &parent_role.permissions {
                            all_permissions.insert(perm.clone());
                        }
                        current_parent = parent_role.parent_id;
                    } else {
                        break;
                    }
                }
            }
        }

        EffectivePermissions {
            user_id: Uuid::nil(), // Set by caller
            permissions: all_permissions.into_iter().collect(),
            roles: role_names,
        }
    }

    /// Check if a user has permission for an action on a resource
    pub fn check_permission(
        &self,
        role_ids: &[Uuid],
        resource: Resource,
        action: Action,
        resource_id: Option<Uuid>,
        environment: Option<&str>,
    ) -> PermissionCheck {
        let effective = self.get_effective_permissions(role_ids);

        for perm in &effective.permissions {
            // Check if permission matches resource
            if perm.resource != resource {
                continue;
            }

            // Check if permission grants the action (or admin which grants all)
            if perm.action != action && perm.action != Action::Admin {
                continue;
            }

            // Check scope
            let scope_matches = match &perm.scope {
                Scope::All => true,
                Scope::Environment(env) => {
                    environment.map(|e| e == env).unwrap_or(false)
                }
                Scope::Group(group_id) => {
                    // Check if resource_id matches group constraint
                    resource_id.map(|id| id == *group_id).unwrap_or(false)
                }
                Scope::Specific => {
                    // Check constraint
                    if let Some(constraint) = &perm.constraint {
                        match constraint {
                            PermissionConstraint::ResourceIds(ids) => {
                                resource_id.map(|id| ids.contains(&id)).unwrap_or(false)
                            }
                            PermissionConstraint::Environments(envs) => {
                                environment.map(|e| envs.contains(&e.to_string())).unwrap_or(false)
                            }
                            PermissionConstraint::GroupIds(ids) => {
                                resource_id.map(|id| ids.contains(&id)).unwrap_or(false)
                            }
                        }
                    } else {
                        false
                    }
                }
                Scope::Owned | Scope::Self_ => {
                    // These require additional context about ownership
                    // For now, return false - actual implementation needs user context
                    false
                }
            };

            if scope_matches {
                return PermissionCheck {
                    allowed: true,
                    resource,
                    action,
                    matched_permission: Some(perm.clone()),
                    reason: Some(format!("Granted by permission {:?}", perm.id)),
                };
            }
        }

        PermissionCheck {
            allowed: false,
            resource,
            action,
            matched_permission: None,
            reason: Some("No matching permission found".to_string()),
        }
    }

    /// Check if user has any of the specified actions on a resource
    pub fn has_any_permission(
        &self,
        role_ids: &[Uuid],
        resource: Resource,
        actions: &[Action],
    ) -> bool {
        for action in actions {
            let check = self.check_permission(role_ids, resource, *action, None, None);
            if check.allowed {
                return true;
            }
        }
        false
    }

    /// Get all permissions for a specific resource
    pub fn get_resource_permissions(
        &self,
        role_ids: &[Uuid],
        resource: Resource,
    ) -> Vec<Action> {
        let effective = self.get_effective_permissions(role_ids);
        effective.permissions
            .iter()
            .filter(|p| p.resource == resource)
            .map(|p| p.action)
            .collect()
    }
}

impl Default for RbacService {
    fn default() -> Self {
        Self::new()
    }
}

/// Middleware helper for permission checking
pub struct PermissionGuard {
    pub resource: Resource,
    pub action: Action,
}

impl PermissionGuard {
    pub fn new(resource: Resource, action: Action) -> Self {
        Self { resource, action }
    }

    pub fn read(resource: Resource) -> Self {
        Self::new(resource, Action::Read)
    }

    pub fn create(resource: Resource) -> Self {
        Self::new(resource, Action::Create)
    }

    pub fn update(resource: Resource) -> Self {
        Self::new(resource, Action::Update)
    }

    pub fn delete(resource: Resource) -> Self {
        Self::new(resource, Action::Delete)
    }

    pub fn admin(resource: Resource) -> Self {
        Self::new(resource, Action::Admin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbac_service_initialization() {
        let service = RbacService::new();
        assert_eq!(service.get_roles().len(), SystemRole::all().len());
    }

    #[test]
    fn test_admin_has_all_permissions() {
        let service = RbacService::new();
        let admin_id = SystemRole::Admin.uuid();

        for resource in Resource::all() {
            let check = service.check_permission(
                &[admin_id],
                resource,
                Action::Admin,
                None,
                None,
            );
            assert!(check.allowed, "Admin should have admin permission for {:?}", resource);
        }
    }

    #[test]
    fn test_viewer_read_only() {
        let service = RbacService::new();
        let viewer_id = SystemRole::Viewer.uuid();

        // Viewer should be able to read nodes
        let read_check = service.check_permission(
            &[viewer_id],
            Resource::Nodes,
            Action::Read,
            None,
            None,
        );
        assert!(read_check.allowed);

        // Viewer should NOT be able to create groups
        let create_check = service.check_permission(
            &[viewer_id],
            Resource::Groups,
            Action::Create,
            None,
            None,
        );
        assert!(!create_check.allowed);
    }

    #[test]
    fn test_operator_permissions() {
        let service = RbacService::new();
        let operator_id = SystemRole::Operator.uuid();

        // Operator can read and create groups
        assert!(service.check_permission(
            &[operator_id],
            Resource::Groups,
            Action::Read,
            None,
            None,
        ).allowed);

        assert!(service.check_permission(
            &[operator_id],
            Resource::Groups,
            Action::Create,
            None,
            None,
        ).allowed);

        // Operator cannot delete groups
        assert!(!service.check_permission(
            &[operator_id],
            Resource::Groups,
            Action::Delete,
            None,
            None,
        ).allowed);
    }

    #[test]
    fn test_auditor_has_audit_log_access() {
        let service = RbacService::new();
        let auditor_id = SystemRole::Auditor.uuid();

        let check = service.check_permission(
            &[auditor_id],
            Resource::AuditLogs,
            Action::Read,
            None,
            None,
        );
        assert!(check.allowed);
    }

    #[test]
    fn test_multiple_roles() {
        let service = RbacService::new();
        let viewer_id = SystemRole::Viewer.uuid();
        let auditor_id = SystemRole::Auditor.uuid();

        // User with both viewer and auditor roles
        let role_ids = vec![viewer_id, auditor_id];

        // Should have audit log access from auditor
        assert!(service.check_permission(
            &role_ids,
            Resource::AuditLogs,
            Action::Read,
            None,
            None,
        ).allowed);

        // Should have export from auditor
        assert!(service.check_permission(
            &role_ids,
            Resource::Reports,
            Action::Export,
            None,
            None,
        ).allowed);
    }

    #[test]
    fn test_create_custom_role() {
        let mut service = RbacService::new();

        let custom_role = Role {
            id: Uuid::new_v4(),
            name: "custom_role".to_string(),
            display_name: "Custom Role".to_string(),
            description: Some("A custom role".to_string()),
            is_system: false,
            parent_id: Some(SystemRole::Viewer.uuid()),
            permissions: vec![
                Permission {
                    id: Uuid::new_v4(),
                    resource: Resource::Groups,
                    action: Action::Create,
                    scope: Scope::All,
                    constraint: None,
                },
            ],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let result = service.create_role(custom_role);
        assert!(result.is_ok());

        // Custom role should have its own permission plus inherited viewer permissions
        let custom_id = service.get_role_by_name("custom_role").unwrap().id;
        let effective = service.get_effective_permissions(&[custom_id]);

        // Should have the custom create permission
        assert!(effective.permissions.iter().any(|p|
            p.resource == Resource::Groups && p.action == Action::Create
        ));

        // Should have inherited read permissions from viewer
        assert!(effective.permissions.iter().any(|p|
            p.resource == Resource::Nodes && p.action == Action::Read
        ));
    }

    #[test]
    fn test_cannot_delete_system_role() {
        let mut service = RbacService::new();
        let result = service.delete_role(SystemRole::Admin.uuid());
        assert!(result.is_err());
    }

    #[test]
    fn test_permission_guard() {
        let guard = PermissionGuard::read(Resource::Nodes);
        assert_eq!(guard.resource, Resource::Nodes);
        assert_eq!(guard.action, Action::Read);
    }
}
