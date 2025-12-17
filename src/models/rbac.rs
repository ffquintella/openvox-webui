//! Role-Based Access Control (RBAC) models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A role that can be assigned to users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Unique identifier
    pub id: Uuid,

    /// Role name (e.g., "admin", "operator", "viewer")
    pub name: String,

    /// Human-readable display name
    pub display_name: String,

    /// Description of the role
    pub description: Option<String>,

    /// Whether this is a built-in system role
    pub is_system: bool,

    /// Parent role for inheritance (optional)
    pub parent_id: Option<Uuid>,

    /// Permissions assigned to this role
    #[serde(default)]
    pub permissions: Vec<Permission>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl Default for Role {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            display_name: String::new(),
            description: None,
            is_system: false,
            parent_id: None,
            permissions: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// A permission that can be granted to roles
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Permission {
    /// Unique identifier
    pub id: Uuid,

    /// Resource this permission applies to
    pub resource: Resource,

    /// Action allowed on the resource
    pub action: Action,

    /// Scope of the permission
    pub scope: Scope,

    /// Optional constraint (e.g., specific group IDs)
    pub constraint: Option<PermissionConstraint>,
}

impl Default for Permission {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            resource: Resource::Nodes,
            action: Action::Read,
            scope: Scope::All,
            constraint: None,
        }
    }
}

/// Resources that can be protected by RBAC
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    /// Node resources from PuppetDB
    Nodes,
    /// Node classification groups
    Groups,
    /// Puppet run reports
    Reports,
    /// Node facts
    Facts,
    /// User accounts
    Users,
    /// RBAC roles
    Roles,
    /// System settings
    Settings,
    /// Audit logs
    AuditLogs,
    /// Facter templates
    FacterTemplates,
    /// API keys
    ApiKeys,
    /// Puppet CA certificates
    Certificates,
}

impl Resource {
    /// Get all available resources
    pub fn all() -> Vec<Resource> {
        vec![
            Resource::Nodes,
            Resource::Groups,
            Resource::Reports,
            Resource::Facts,
            Resource::Users,
            Resource::Roles,
            Resource::Settings,
            Resource::AuditLogs,
            Resource::FacterTemplates,
            Resource::ApiKeys,
            Resource::Certificates,
        ]
    }

    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Resource::Nodes => "nodes",
            Resource::Groups => "groups",
            Resource::Reports => "reports",
            Resource::Facts => "facts",
            Resource::Users => "users",
            Resource::Roles => "roles",
            Resource::Settings => "settings",
            Resource::AuditLogs => "audit_logs",
            Resource::FacterTemplates => "facter_templates",
            Resource::ApiKeys => "api_keys",
            Resource::Certificates => "certificates",
        }
    }
}

/// Actions that can be performed on resources
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Read/view resource
    Read,
    /// Create new resource
    Create,
    /// Update existing resource
    Update,
    /// Delete resource
    Delete,
    /// Full admin access
    Admin,
    /// Export resource data
    Export,
    /// Classify nodes
    Classify,
    /// Generate facts
    Generate,
    /// Sign certificates
    Sign,
    /// Reject certificates
    Reject,
    /// Revoke certificates
    Revoke,
}

impl Action {
    /// Get all available actions
    pub fn all() -> Vec<Action> {
        vec![
            Action::Read,
            Action::Create,
            Action::Update,
            Action::Delete,
            Action::Admin,
            Action::Export,
            Action::Classify,
            Action::Generate,
            Action::Sign,
            Action::Reject,
            Action::Revoke,
        ]
    }

    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Read => "read",
            Action::Create => "create",
            Action::Update => "update",
            Action::Delete => "delete",
            Action::Admin => "admin",
            Action::Export => "export",
            Action::Classify => "classify",
            Action::Generate => "generate",
            Action::Sign => "sign",
            Action::Reject => "reject",
            Action::Revoke => "revoke",
        }
    }
}

/// Scope of a permission
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// All resources of this type
    #[default]
    All,
    /// Only resources owned by the user
    Owned,
    /// Only the user's own record (for users resource)
    Self_,
    /// Specific resources identified by constraint
    Specific,
    /// Resources within a specific environment
    Environment(String),
    /// Resources within a specific group
    Group(Uuid),
}

/// Constraint for specific permissions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum PermissionConstraint {
    /// List of specific resource IDs
    ResourceIds(Vec<Uuid>),
    /// List of specific environments
    Environments(Vec<String>),
    /// List of specific group IDs
    GroupIds(Vec<Uuid>),
}

/// User with their assigned roles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserWithRoles {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub roles: Vec<Role>,
    pub created_at: DateTime<Utc>,
}

/// Request to create a new role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub permissions: Option<Vec<CreatePermissionRequest>>,
}

/// Request to create a permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePermissionRequest {
    pub resource: Resource,
    pub action: Action,
    pub scope: Option<Scope>,
    pub constraint: Option<PermissionConstraint>,
}

/// Request to assign roles to a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignRolesRequest {
    pub role_ids: Vec<Uuid>,
}

/// Effective permissions for a user (computed from all assigned roles)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectivePermissions {
    pub user_id: Uuid,
    pub permissions: Vec<Permission>,
    pub roles: Vec<String>,
}

/// Permission check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCheck {
    pub allowed: bool,
    pub resource: Resource,
    pub action: Action,
    pub matched_permission: Option<Permission>,
    pub reason: Option<String>,
}

/// Permission with its associated role information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionWithRole {
    #[serde(flatten)]
    pub permission: Permission,
    pub role_id: Uuid,
    pub role_name: String,
}

/// Built-in system roles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemRole {
    Admin,
    Operator,
    Viewer,
    GroupAdmin,
    Auditor,
}

impl SystemRole {
    /// Get the role name
    pub fn name(&self) -> &'static str {
        match self {
            SystemRole::Admin => "admin",
            SystemRole::Operator => "operator",
            SystemRole::Viewer => "viewer",
            SystemRole::GroupAdmin => "group_admin",
            SystemRole::Auditor => "auditor",
        }
    }

    /// Get the display name
    pub fn display_name(&self) -> &'static str {
        match self {
            SystemRole::Admin => "Administrator",
            SystemRole::Operator => "Operator",
            SystemRole::Viewer => "Viewer",
            SystemRole::GroupAdmin => "Group Administrator",
            SystemRole::Auditor => "Auditor",
        }
    }

    /// Get the description
    pub fn description(&self) -> &'static str {
        match self {
            SystemRole::Admin => "Full system access with all permissions",
            SystemRole::Operator => "Day-to-day operations access",
            SystemRole::Viewer => "Read-only access to all resources",
            SystemRole::GroupAdmin => "Full access to assigned node groups",
            SystemRole::Auditor => "Read access with audit log visibility",
        }
    }

    /// Get all system roles
    pub fn all() -> Vec<SystemRole> {
        vec![
            SystemRole::Admin,
            SystemRole::Operator,
            SystemRole::Viewer,
            SystemRole::GroupAdmin,
            SystemRole::Auditor,
        ]
    }

    /// Create the Role struct for this system role
    pub fn to_role(&self) -> Role {
        Role {
            id: self.uuid(),
            name: self.name().to_string(),
            display_name: self.display_name().to_string(),
            description: Some(self.description().to_string()),
            is_system: true,
            parent_id: None,
            permissions: self.default_permissions(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Get the fixed UUID for this system role
    pub fn uuid(&self) -> Uuid {
        match self {
            SystemRole::Admin => Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            SystemRole::Operator => Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            SystemRole::Viewer => Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
            SystemRole::GroupAdmin => Uuid::parse_str("00000000-0000-0000-0000-000000000004").unwrap(),
            SystemRole::Auditor => Uuid::parse_str("00000000-0000-0000-0000-000000000005").unwrap(),
        }
    }

    /// Get default permissions for this role
    pub fn default_permissions(&self) -> Vec<Permission> {
        match self {
            SystemRole::Admin => {
                // Admin has all permissions on all resources
                let mut perms = vec![];
                for resource in Resource::all() {
                    perms.push(Permission {
                        id: Uuid::new_v4(),
                        resource,
                        action: Action::Admin,
                        scope: Scope::All,
                        constraint: None,
                    });
                }
                perms
            }
            SystemRole::Operator => {
                vec![
                    // Full access to nodes
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Nodes,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Nodes,
                        action: Action::Classify,
                        scope: Scope::All,
                        constraint: None,
                    },
                    // Full access to groups
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Groups,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Groups,
                        action: Action::Create,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Groups,
                        action: Action::Update,
                        scope: Scope::All,
                        constraint: None,
                    },
                    // Read reports and facts
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Reports,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Facts,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    // Read settings
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Settings,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                ]
            }
            SystemRole::Viewer => {
                vec![
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Nodes,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Groups,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Reports,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Facts,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                ]
            }
            SystemRole::GroupAdmin => {
                // Group admin has full access but scoped to specific groups
                vec![
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Groups,
                        action: Action::Admin,
                        scope: Scope::Specific,
                        constraint: None, // Constraint set per user
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Nodes,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Nodes,
                        action: Action::Classify,
                        scope: Scope::All,
                        constraint: None,
                    },
                ]
            }
            SystemRole::Auditor => {
                vec![
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Nodes,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Groups,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Reports,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Facts,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::AuditLogs,
                        action: Action::Read,
                        scope: Scope::All,
                        constraint: None,
                    },
                    Permission {
                        id: Uuid::new_v4(),
                        resource: Resource::Reports,
                        action: Action::Export,
                        scope: Scope::All,
                        constraint: None,
                    },
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_role_to_role() {
        let admin_role = SystemRole::Admin.to_role();
        assert_eq!(admin_role.name, "admin");
        assert!(admin_role.is_system);
        assert!(!admin_role.permissions.is_empty());
    }

    #[test]
    fn test_admin_has_all_permissions() {
        let admin_role = SystemRole::Admin.to_role();
        let resources = Resource::all();

        for resource in resources {
            let has_admin = admin_role.permissions.iter().any(|p| {
                p.resource == resource && p.action == Action::Admin
            });
            assert!(has_admin, "Admin should have admin permission for {:?}", resource);
        }
    }

    #[test]
    fn test_viewer_is_read_only() {
        let viewer_role = SystemRole::Viewer.to_role();

        for permission in &viewer_role.permissions {
            assert_eq!(permission.action, Action::Read,
                "Viewer should only have read permissions, found {:?}", permission.action);
        }
    }

    #[test]
    fn test_permission_serialization() {
        let permission = Permission {
            id: Uuid::new_v4(),
            resource: Resource::Nodes,
            action: Action::Read,
            scope: Scope::Environment("production".to_string()),
            constraint: None,
        };

        let json = serde_json::to_string(&permission).unwrap();
        let parsed: Permission = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.resource, Resource::Nodes);
        assert_eq!(parsed.action, Action::Read);
    }

    #[test]
    fn test_scope_serialization() {
        let scope = Scope::Group(Uuid::new_v4());
        let json = serde_json::to_string(&scope).unwrap();
        assert!(json.contains("group"));
    }
}
