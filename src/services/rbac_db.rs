//! Database-backed RBAC (Role-Based Access Control) service
//!
//! This module provides RBAC operations backed by SQLite database,
//! including role/permission CRUD, user-role assignments, and permission caching.

use anyhow::{Context, Result};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::models::{
    Action, CreatePermissionRequest, CreateRoleRequest, EffectivePermissions, Permission,
    PermissionCheck, PermissionConstraint, PermissionWithRole, Resource, Role, Scope,
};

/// Cache entry with TTL
struct CacheEntry<T> {
    data: T,
    expires_at: Instant,
}

impl<T> CacheEntry<T> {
    fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            expires_at: Instant::now() + ttl,
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// Database-backed RBAC service with caching
pub struct DbRbacService {
    pool: SqlitePool,
    /// Cache TTL (default: 5 minutes)
    cache_ttl: Duration,
    /// Role cache (role_id -> Role)
    role_cache: RwLock<HashMap<Uuid, CacheEntry<Role>>>,
    /// User permissions cache (user_id -> EffectivePermissions)
    permission_cache: RwLock<HashMap<Uuid, CacheEntry<EffectivePermissions>>>,
    /// Reverse lookup: role_id -> set of user_ids that have this role
    /// Used for selective cache invalidation when a role changes
    role_users_cache: RwLock<HashMap<Uuid, Vec<Uuid>>>,
}

impl DbRbacService {
    /// Create a new database-backed RBAC service
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            cache_ttl: Duration::from_secs(300), // 5 minutes default
            role_cache: RwLock::new(HashMap::new()),
            permission_cache: RwLock::new(HashMap::new()),
            role_users_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Create with custom cache TTL
    pub fn with_cache_ttl(pool: SqlitePool, ttl: Duration) -> Self {
        Self {
            pool,
            cache_ttl: ttl,
            role_cache: RwLock::new(HashMap::new()),
            permission_cache: RwLock::new(HashMap::new()),
            role_users_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Clear all caches
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.role_cache.write() {
            cache.clear();
        }
        if let Ok(mut cache) = self.permission_cache.write() {
            cache.clear();
        }
        if let Ok(mut cache) = self.role_users_cache.write() {
            cache.clear();
        }
    }

    /// Invalidate cache for a specific user
    pub fn invalidate_user_cache(&self, user_id: &Uuid) {
        if let Ok(mut cache) = self.permission_cache.write() {
            cache.remove(user_id);
        }
    }

    /// Invalidate cache for a specific role and affected users
    ///
    /// Selectively invalidates only the permission caches for users
    /// who have the modified role, rather than clearing all user caches.
    pub fn invalidate_role_cache(&self, role_id: &Uuid) {
        // Remove from role cache
        if let Ok(mut cache) = self.role_cache.write() {
            cache.remove(role_id);
        }

        // Get users affected by this role change
        let affected_users: Vec<Uuid> = if let Ok(cache) = self.role_users_cache.read() {
            cache.get(role_id).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };

        // Selectively invalidate only affected users' permission caches
        if !affected_users.is_empty() {
            if let Ok(mut cache) = self.permission_cache.write() {
                for user_id in &affected_users {
                    cache.remove(user_id);
                }
            }
        } else {
            // If we don't have tracking info, fall back to clearing all
            // This happens on startup or when the cache was cleared
            if let Ok(mut cache) = self.permission_cache.write() {
                cache.clear();
            }
        }
    }

    /// Track that a user has a specific role (for selective cache invalidation)
    fn track_user_role(&self, user_id: &Uuid, role_id: &Uuid) {
        if let Ok(mut cache) = self.role_users_cache.write() {
            cache
                .entry(*role_id)
                .or_insert_with(Vec::new)
                .push(*user_id);
        }
    }

    /// Remove tracking of a user's role
    fn untrack_user_role(&self, user_id: &Uuid, role_id: &Uuid) {
        if let Ok(mut cache) = self.role_users_cache.write() {
            if let Some(users) = cache.get_mut(role_id) {
                users.retain(|id| id != user_id);
            }
        }
    }

    /// Update role-user tracking when fetching user roles
    fn update_role_tracking(&self, user_id: &Uuid, role_ids: &[Uuid]) {
        if let Ok(mut cache) = self.role_users_cache.write() {
            // Add user to each role's user list
            for role_id in role_ids {
                let users = cache.entry(*role_id).or_insert_with(Vec::new);
                if !users.contains(user_id) {
                    users.push(*user_id);
                }
            }
        }
    }

    // =========================================================================
    // Role Operations
    // =========================================================================

    /// Get all roles from database
    ///
    /// Optimized to batch load permissions in a single query instead of N+1 queries.
    pub async fn get_all_roles(&self) -> Result<Vec<Role>> {
        let rows = sqlx::query(
            "SELECT id, name, display_name, description, is_system, parent_id, created_at, updated_at
             FROM roles ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch roles")?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Collect role IDs for batch loading
        let role_ids: Vec<String> = rows
            .iter()
            .map(|r| r.get::<String, _>("id"))
            .collect();

        // Batch load all permissions for all roles in a single query
        let permissions_map = self.batch_get_role_permissions(&role_ids).await?;

        let mut roles = Vec::new();
        for row in rows {
            let role_id_str: String = row.get("id");
            let permissions = permissions_map.get(&role_id_str).cloned().unwrap_or_default();
            roles.push(row_to_role(&row, permissions));
        }

        Ok(roles)
    }

    /// Get a role by ID
    pub async fn get_role(&self, id: &Uuid) -> Result<Option<Role>> {
        // Check cache first
        if let Ok(cache) = self.role_cache.read() {
            if let Some(entry) = cache.get(id) {
                if !entry.is_expired() {
                    return Ok(Some(entry.data.clone()));
                }
            }
        }

        // Fetch from database
        let id_str = id.to_string();
        let row = sqlx::query(
            "SELECT id, name, display_name, description, is_system, parent_id, created_at, updated_at
             FROM roles WHERE id = ?"
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch role")?;

        match row {
            Some(row) => {
                let permissions = self.get_role_permissions_db(id).await?;
                let role = row_to_role(&row, permissions);

                // Update cache
                if let Ok(mut cache) = self.role_cache.write() {
                    cache.insert(*id, CacheEntry::new(role.clone(), self.cache_ttl));
                }

                Ok(Some(role))
            }
            None => Ok(None),
        }
    }

    /// Get a role by name
    pub async fn get_role_by_name(&self, name: &str) -> Result<Option<Role>> {
        let row = sqlx::query(
            "SELECT id, name, display_name, description, is_system, parent_id, created_at, updated_at
             FROM roles WHERE name = ?"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch role by name")?;

        match row {
            Some(row) => {
                let role_id = parse_uuid(row.get::<String, _>("id"))?;
                let permissions = self.get_role_permissions_db(&role_id).await?;
                Ok(Some(row_to_role(&row, permissions)))
            }
            None => Ok(None),
        }
    }

    /// Create a new role
    pub async fn create_role(&self, request: CreateRoleRequest) -> Result<Role> {
        // Check if name already exists
        if self.get_role_by_name(&request.name).await?.is_some() {
            anyhow::bail!("Role with name '{}' already exists", request.name);
        }

        // Validate parent exists if specified
        if let Some(parent_id) = &request.parent_id {
            if self.get_role(parent_id).await?.is_none() {
                anyhow::bail!("Parent role not found");
            }
        }

        let role_id = Uuid::new_v4();
        let id_str = role_id.to_string();
        let parent_id_str = request.parent_id.map(|id| id.to_string());
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO roles (id, name, display_name, description, is_system, parent_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, FALSE, ?, ?, ?)"
        )
        .bind(&id_str)
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.description)
        .bind(&parent_id_str)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("Failed to create role")?;

        // Add permissions if provided
        if let Some(permissions) = request.permissions {
            for perm_request in permissions {
                self.add_permission_to_role(&role_id, perm_request).await?;
            }
        }

        self.get_role(&role_id)
            .await?
            .context("Failed to fetch created role")
    }

    /// Update a role
    pub async fn update_role(&self, id: &Uuid, request: CreateRoleRequest) -> Result<Role> {
        let existing = self.get_role(id).await?.context("Role not found")?;

        if existing.is_system {
            anyhow::bail!("Cannot modify system roles");
        }

        // Check name uniqueness if changed
        if request.name != existing.name && self.get_role_by_name(&request.name).await?.is_some() {
            anyhow::bail!("Role with name '{}' already exists", request.name);
        }

        let id_str = id.to_string();
        let parent_id_str = request.parent_id.map(|id| id.to_string());
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE roles SET name = ?, display_name = ?, description = ?, parent_id = ?, updated_at = ?
             WHERE id = ?"
        )
        .bind(&request.name)
        .bind(&request.display_name)
        .bind(&request.description)
        .bind(&parent_id_str)
        .bind(&now)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .context("Failed to update role")?;

        self.invalidate_role_cache(id);

        self.get_role(id)
            .await?
            .context("Failed to fetch updated role")
    }

    /// Delete a role
    pub async fn delete_role(&self, id: &Uuid) -> Result<bool> {
        let existing = self.get_role(id).await?.context("Role not found")?;

        if existing.is_system {
            anyhow::bail!("Cannot delete system roles");
        }

        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM roles WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .context("Failed to delete role")?;

        self.invalidate_role_cache(id);

        Ok(result.rows_affected() > 0)
    }

    // =========================================================================
    // Permission Operations
    // =========================================================================

    /// Get all permissions from database
    pub async fn get_all_permissions(&self) -> Result<Vec<PermissionWithRole>> {
        let rows = sqlx::query(
            "SELECT p.id, p.role_id, p.resource, p.action, p.scope_type, p.scope_value,
                    p.constraint_type, p.constraint_value, r.name as role_name
             FROM permissions p
             INNER JOIN roles r ON p.role_id = r.id
             ORDER BY r.name, p.resource, p.action",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch permissions")?;

        let mut permissions = Vec::new();
        for row in rows {
            let role_id = parse_uuid(row.get::<String, _>("role_id"))?;
            let role_name: String = row.get("role_name");
            let permission = row_to_permission(&row)?;
            permissions.push(PermissionWithRole {
                permission,
                role_id,
                role_name,
            });
        }

        Ok(permissions)
    }

    /// Get permissions for a role from database
    async fn get_role_permissions_db(&self, role_id: &Uuid) -> Result<Vec<Permission>> {
        let role_id_str = role_id.to_string();
        let rows = sqlx::query(
            "SELECT id, resource, action, scope_type, scope_value, constraint_type, constraint_value
             FROM permissions WHERE role_id = ?"
        )
        .bind(&role_id_str)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch permissions")?;

        let mut permissions = Vec::new();
        for row in rows {
            permissions.push(row_to_permission(&row)?);
        }

        Ok(permissions)
    }

    /// Batch load permissions for multiple roles in a single query
    ///
    /// This reduces N queries to 1 query for permission loading.
    async fn batch_get_role_permissions(
        &self,
        role_ids: &[String],
    ) -> Result<HashMap<String, Vec<Permission>>> {
        if role_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Build the IN clause placeholders
        let placeholders: Vec<&str> = role_ids.iter().map(|_| "?").collect();
        let query = format!(
            "SELECT id, role_id, resource, action, scope_type, scope_value, constraint_type, constraint_value
             FROM permissions
             WHERE role_id IN ({})
             ORDER BY role_id, resource, action",
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query(&query);
        for id in role_ids {
            query_builder = query_builder.bind(id);
        }

        let rows = query_builder
            .fetch_all(&self.pool)
            .await
            .context("Failed to batch fetch permissions")?;

        // Group permissions by role_id
        let mut permissions_map: HashMap<String, Vec<Permission>> = HashMap::new();
        for row in rows {
            let role_id: String = row.get("role_id");
            let permission = row_to_permission(&row)?;
            permissions_map
                .entry(role_id)
                .or_insert_with(Vec::new)
                .push(permission);
        }

        Ok(permissions_map)
    }

    /// Add a permission to a role
    pub async fn add_permission_to_role(
        &self,
        role_id: &Uuid,
        request: CreatePermissionRequest,
    ) -> Result<Permission> {
        let perm_id = Uuid::new_v4();
        let perm_id_str = perm_id.to_string();
        let role_id_str = role_id.to_string();
        let resource_str = request.resource.as_str();
        let action_str = request.action.as_str();
        let scope = request.scope.unwrap_or(Scope::All);
        let (scope_type, scope_value) = scope_to_db(&scope);
        let (constraint_type, constraint_value) = constraint_to_db(&request.constraint);
        let created_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO permissions (id, role_id, resource, action, scope_type, scope_value, constraint_type, constraint_value, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&perm_id_str)
        .bind(&role_id_str)
        .bind(resource_str)
        .bind(action_str)
        .bind(&scope_type)
        .bind(&scope_value)
        .bind(&constraint_type)
        .bind(&constraint_value)
        .bind(&created_at)
        .execute(&self.pool)
        .await
        .context("Failed to add permission")?;

        self.invalidate_role_cache(role_id);

        Ok(Permission {
            id: perm_id,
            resource: request.resource,
            action: request.action,
            scope,
            constraint: request.constraint,
        })
    }

    /// Remove a permission from a role
    pub async fn remove_permission(&self, permission_id: &Uuid) -> Result<bool> {
        let perm_id_str = permission_id.to_string();

        // Get the role_id first to invalidate cache
        let row = sqlx::query("SELECT role_id FROM permissions WHERE id = ?")
            .bind(&perm_id_str)
            .fetch_optional(&self.pool)
            .await?;

        let result = sqlx::query("DELETE FROM permissions WHERE id = ?")
            .bind(&perm_id_str)
            .execute(&self.pool)
            .await
            .context("Failed to remove permission")?;

        if let Some(row) = row {
            let role_id = parse_uuid(row.get::<String, _>("role_id"))?;
            self.invalidate_role_cache(&role_id);
        }

        Ok(result.rows_affected() > 0)
    }

    /// Update permissions for a role (replace all)
    pub async fn set_role_permissions(
        &self,
        role_id: &Uuid,
        permissions: Vec<CreatePermissionRequest>,
    ) -> Result<Vec<Permission>> {
        let role = self.get_role(role_id).await?.context("Role not found")?;

        if role.is_system {
            anyhow::bail!("Cannot modify system role permissions");
        }

        let role_id_str = role_id.to_string();

        // Delete existing permissions
        sqlx::query("DELETE FROM permissions WHERE role_id = ?")
            .bind(&role_id_str)
            .execute(&self.pool)
            .await
            .context("Failed to delete existing permissions")?;

        // Add new permissions
        let mut result = Vec::new();
        for perm_request in permissions {
            let perm = self.add_permission_to_role(role_id, perm_request).await?;
            result.push(perm);
        }

        self.invalidate_role_cache(role_id);

        Ok(result)
    }

    // =========================================================================
    // User-Role Assignment Operations
    // =========================================================================

    /// Get roles assigned to a user
    ///
    /// Optimized to batch load permissions in a single query instead of N+1 queries.
    /// Also updates the role-user tracking for selective cache invalidation.
    pub async fn get_user_roles(&self, user_id: &Uuid) -> Result<Vec<Role>> {
        let user_id_str = user_id.to_string();
        let rows = sqlx::query(
            "SELECT r.id, r.name, r.display_name, r.description, r.is_system, r.parent_id, r.created_at, r.updated_at
             FROM roles r
             INNER JOIN user_roles ur ON r.id = ur.role_id
             WHERE ur.user_id = ?
             ORDER BY r.name"
        )
        .bind(&user_id_str)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch user roles")?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Collect role IDs for batch loading
        let role_ids: Vec<String> = rows
            .iter()
            .map(|r| r.get::<String, _>("id"))
            .collect();

        // Update role-user tracking for selective cache invalidation
        let role_uuids: Vec<Uuid> = role_ids
            .iter()
            .filter_map(|id| Uuid::parse_str(id).ok())
            .collect();
        self.update_role_tracking(user_id, &role_uuids);

        // Batch load all permissions for all roles in a single query
        let permissions_map = self.batch_get_role_permissions(&role_ids).await?;

        let mut roles = Vec::new();
        for row in rows {
            let role_id_str: String = row.get("id");
            let permissions = permissions_map.get(&role_id_str).cloned().unwrap_or_default();
            roles.push(row_to_role(&row, permissions));
        }

        Ok(roles)
    }

    /// Get role IDs for a user (for JWT claims)
    pub async fn get_user_role_ids(&self, user_id: &Uuid) -> Result<Vec<Uuid>> {
        let user_id_str = user_id.to_string();
        let rows = sqlx::query("SELECT role_id FROM user_roles WHERE user_id = ?")
            .bind(&user_id_str)
            .fetch_all(&self.pool)
            .await
            .context("Failed to fetch user role IDs")?;

        let mut role_ids = Vec::new();
        for row in rows {
            role_ids.push(parse_uuid(row.get::<String, _>("role_id"))?);
        }

        Ok(role_ids)
    }

    /// Assign roles to a user
    pub async fn assign_roles(&self, user_id: &Uuid, role_ids: &[Uuid]) -> Result<()> {
        let user_id_str = user_id.to_string();

        // Verify all roles exist
        for role_id in role_ids {
            if self.get_role(role_id).await?.is_none() {
                anyhow::bail!("Role {} not found", role_id);
            }
        }

        // Delete existing assignments
        sqlx::query("DELETE FROM user_roles WHERE user_id = ?")
            .bind(&user_id_str)
            .execute(&self.pool)
            .await
            .context("Failed to delete existing role assignments")?;

        // Insert new assignments
        for role_id in role_ids {
            let id = Uuid::new_v4().to_string();
            let role_id_str = role_id.to_string();
            let now = chrono::Utc::now().to_rfc3339();

            sqlx::query(
                "INSERT INTO user_roles (id, user_id, role_id, created_at) VALUES (?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&user_id_str)
            .bind(&role_id_str)
            .bind(&now)
            .execute(&self.pool)
            .await
            .context("Failed to assign role")?;

            // Track the new role assignment for selective cache invalidation
            self.track_user_role(user_id, role_id);
        }

        self.invalidate_user_cache(user_id);

        Ok(())
    }

    /// Add a single role to a user
    pub async fn add_role_to_user(&self, user_id: &Uuid, role_id: &Uuid) -> Result<()> {
        // Verify role exists
        if self.get_role(role_id).await?.is_none() {
            anyhow::bail!("Role not found");
        }

        let id = Uuid::new_v4().to_string();
        let user_id_str = user_id.to_string();
        let role_id_str = role_id.to_string();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT OR IGNORE INTO user_roles (id, user_id, role_id, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&user_id_str)
        .bind(&role_id_str)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("Failed to add role to user")?;

        // Track the role assignment for selective cache invalidation
        self.track_user_role(user_id, role_id);
        self.invalidate_user_cache(user_id);

        Ok(())
    }

    /// Remove a role from a user
    pub async fn remove_role_from_user(&self, user_id: &Uuid, role_id: &Uuid) -> Result<bool> {
        let user_id_str = user_id.to_string();
        let role_id_str = role_id.to_string();

        let result = sqlx::query("DELETE FROM user_roles WHERE user_id = ? AND role_id = ?")
            .bind(&user_id_str)
            .bind(&role_id_str)
            .execute(&self.pool)
            .await
            .context("Failed to remove role from user")?;

        // Update tracking
        self.untrack_user_role(user_id, role_id);
        self.invalidate_user_cache(user_id);

        Ok(result.rows_affected() > 0)
    }

    // =========================================================================
    // Permission Checking
    // =========================================================================

    /// Get effective permissions for a user (with caching)
    pub async fn get_effective_permissions(&self, user_id: &Uuid) -> Result<EffectivePermissions> {
        // Check cache first
        if let Ok(cache) = self.permission_cache.read() {
            if let Some(entry) = cache.get(user_id) {
                if !entry.is_expired() {
                    return Ok(entry.data.clone());
                }
            }
        }

        // Compute effective permissions
        let roles = self.get_user_roles(user_id).await?;
        let mut all_permissions = std::collections::HashSet::new();
        let mut role_names = Vec::new();

        for role in &roles {
            role_names.push(role.name.clone());

            // Add direct permissions
            for perm in &role.permissions {
                all_permissions.insert(perm.clone());
            }

            // Add inherited permissions from parent roles
            let mut current_parent = role.parent_id;
            while let Some(parent_id) = current_parent {
                if let Some(parent_role) = self.get_role(&parent_id).await? {
                    for perm in &parent_role.permissions {
                        all_permissions.insert(perm.clone());
                    }
                    current_parent = parent_role.parent_id;
                } else {
                    break;
                }
            }
        }

        let effective = EffectivePermissions {
            user_id: *user_id,
            permissions: all_permissions.into_iter().collect(),
            roles: role_names,
        };

        // Update cache
        if let Ok(mut cache) = self.permission_cache.write() {
            cache.insert(*user_id, CacheEntry::new(effective.clone(), self.cache_ttl));
        }

        Ok(effective)
    }

    /// Check if a user has permission for an action on a resource
    pub async fn check_permission(
        &self,
        user_id: &Uuid,
        resource: Resource,
        action: Action,
        resource_id: Option<Uuid>,
        environment: Option<&str>,
    ) -> Result<PermissionCheck> {
        let effective = self.get_effective_permissions(user_id).await?;

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
                Scope::Environment(env) => environment.map(|e| e == env).unwrap_or(false),
                Scope::Group(group_id) => resource_id.map(|id| id == *group_id).unwrap_or(false),
                Scope::Specific => {
                    if let Some(constraint) = &perm.constraint {
                        match constraint {
                            PermissionConstraint::ResourceIds(ids) => {
                                resource_id.map(|id| ids.contains(&id)).unwrap_or(false)
                            }
                            PermissionConstraint::Environments(envs) => environment
                                .map(|e| envs.contains(&e.to_string()))
                                .unwrap_or(false),
                            PermissionConstraint::GroupIds(ids) => {
                                resource_id.map(|id| ids.contains(&id)).unwrap_or(false)
                            }
                        }
                    } else {
                        false
                    }
                }
                Scope::Owned | Scope::Self_ => false, // Requires additional context
            };

            if scope_matches {
                return Ok(PermissionCheck {
                    allowed: true,
                    resource,
                    action,
                    matched_permission: Some(perm.clone()),
                    reason: Some(format!("Granted by permission {:?}", perm.id)),
                });
            }
        }

        Ok(PermissionCheck {
            allowed: false,
            resource,
            action,
            matched_permission: None,
            reason: Some("No matching permission found".to_string()),
        })
    }

    /// Check permission using role IDs directly (for middleware)
    pub async fn check_permission_by_roles(
        &self,
        role_ids: &[Uuid],
        resource: Resource,
        action: Action,
        resource_id: Option<Uuid>,
        environment: Option<&str>,
    ) -> Result<PermissionCheck> {
        let mut all_permissions = std::collections::HashSet::new();

        for role_id in role_ids {
            if let Some(role) = self.get_role(role_id).await? {
                // Add direct permissions
                for perm in &role.permissions {
                    all_permissions.insert(perm.clone());
                }

                // Add inherited permissions
                let mut current_parent = role.parent_id;
                while let Some(parent_id) = current_parent {
                    if let Some(parent_role) = self.get_role(&parent_id).await? {
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

        for perm in &all_permissions {
            if perm.resource != resource {
                continue;
            }

            if perm.action != action && perm.action != Action::Admin {
                continue;
            }

            let scope_matches = match &perm.scope {
                Scope::All => true,
                Scope::Environment(env) => environment.map(|e| e == env).unwrap_or(false),
                Scope::Group(group_id) => resource_id.map(|id| id == *group_id).unwrap_or(false),
                Scope::Specific => {
                    if let Some(constraint) = &perm.constraint {
                        match constraint {
                            PermissionConstraint::ResourceIds(ids) => {
                                resource_id.map(|id| ids.contains(&id)).unwrap_or(false)
                            }
                            PermissionConstraint::Environments(envs) => environment
                                .map(|e| envs.contains(&e.to_string()))
                                .unwrap_or(false),
                            PermissionConstraint::GroupIds(ids) => {
                                resource_id.map(|id| ids.contains(&id)).unwrap_or(false)
                            }
                        }
                    } else {
                        false
                    }
                }
                Scope::Owned | Scope::Self_ => false,
            };

            if scope_matches {
                return Ok(PermissionCheck {
                    allowed: true,
                    resource,
                    action,
                    matched_permission: Some(perm.clone()),
                    reason: Some(format!("Granted by permission {:?}", perm.id)),
                });
            }
        }

        Ok(PermissionCheck {
            allowed: false,
            resource,
            action,
            matched_permission: None,
            reason: Some("No matching permission found".to_string()),
        })
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn parse_uuid(s: String) -> Result<Uuid> {
    Uuid::parse_str(&s).map_err(|_| anyhow::anyhow!("Invalid UUID: {}", s))
}

fn row_to_role(row: &sqlx::sqlite::SqliteRow, permissions: Vec<Permission>) -> Role {
    let id_str: String = row.get("id");
    let parent_id_str: Option<String> = row.get("parent_id");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Role {
        id: Uuid::parse_str(&id_str).unwrap_or(Uuid::nil()),
        name: row.get("name"),
        display_name: row.get("display_name"),
        description: row.get("description"),
        is_system: row.get("is_system"),
        parent_id: parent_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
        permissions,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    }
}

fn row_to_permission(row: &sqlx::sqlite::SqliteRow) -> Result<Permission> {
    let id_str: String = row.get("id");
    let resource_str: String = row.get("resource");
    let action_str: String = row.get("action");
    let scope_type: String = row.get("scope_type");
    let scope_value: Option<String> = row.get("scope_value");
    let constraint_type: Option<String> = row.get("constraint_type");
    let constraint_value: Option<String> = row.get("constraint_value");

    Ok(Permission {
        id: parse_uuid(id_str)?,
        resource: parse_resource(&resource_str)?,
        action: parse_action(&action_str)?,
        scope: db_to_scope(&scope_type, scope_value)?,
        constraint: db_to_constraint(constraint_type, constraint_value)?,
    })
}

fn parse_resource(s: &str) -> Result<Resource> {
    match s {
        "nodes" => Ok(Resource::Nodes),
        "groups" => Ok(Resource::Groups),
        "reports" => Ok(Resource::Reports),
        "facts" => Ok(Resource::Facts),
        "users" => Ok(Resource::Users),
        "roles" => Ok(Resource::Roles),
        "settings" => Ok(Resource::Settings),
        "audit_logs" => Ok(Resource::AuditLogs),
        "facter_templates" => Ok(Resource::FacterTemplates),
        "api_keys" => Ok(Resource::ApiKeys),
        _ => anyhow::bail!("Unknown resource: {}", s),
    }
}

fn parse_action(s: &str) -> Result<Action> {
    match s {
        "read" => Ok(Action::Read),
        "create" => Ok(Action::Create),
        "update" => Ok(Action::Update),
        "delete" => Ok(Action::Delete),
        "admin" => Ok(Action::Admin),
        "export" => Ok(Action::Export),
        "classify" => Ok(Action::Classify),
        "generate" => Ok(Action::Generate),
        _ => anyhow::bail!("Unknown action: {}", s),
    }
}

fn scope_to_db(scope: &Scope) -> (String, Option<String>) {
    match scope {
        Scope::All => ("all".to_string(), None),
        Scope::Owned => ("owned".to_string(), None),
        Scope::Self_ => ("self".to_string(), None),
        Scope::Specific => ("specific".to_string(), None),
        Scope::Environment(env) => ("environment".to_string(), Some(env.clone())),
        Scope::Group(id) => ("group".to_string(), Some(id.to_string())),
    }
}

fn db_to_scope(scope_type: &str, scope_value: Option<String>) -> Result<Scope> {
    match scope_type {
        "all" => Ok(Scope::All),
        "owned" => Ok(Scope::Owned),
        "self" => Ok(Scope::Self_),
        "specific" => Ok(Scope::Specific),
        "environment" => Ok(Scope::Environment(scope_value.unwrap_or_default())),
        "group" => {
            let id = scope_value
                .map(|s| Uuid::parse_str(&s))
                .transpose()
                .map_err(|_| anyhow::anyhow!("Invalid group UUID"))?
                .unwrap_or(Uuid::nil());
            Ok(Scope::Group(id))
        }
        _ => anyhow::bail!("Unknown scope type: {}", scope_type),
    }
}

fn constraint_to_db(constraint: &Option<PermissionConstraint>) -> (Option<String>, Option<String>) {
    match constraint {
        None => (None, None),
        Some(PermissionConstraint::ResourceIds(ids)) => {
            let value = serde_json::to_string(ids).ok();
            (Some("resource_ids".to_string()), value)
        }
        Some(PermissionConstraint::Environments(envs)) => {
            let value = serde_json::to_string(envs).ok();
            (Some("environments".to_string()), value)
        }
        Some(PermissionConstraint::GroupIds(ids)) => {
            let value = serde_json::to_string(ids).ok();
            (Some("group_ids".to_string()), value)
        }
    }
}

fn db_to_constraint(
    constraint_type: Option<String>,
    constraint_value: Option<String>,
) -> Result<Option<PermissionConstraint>> {
    match (constraint_type, constraint_value) {
        (None, _) | (_, None) => Ok(None),
        (Some(ctype), Some(cvalue)) => match ctype.as_str() {
            "resource_ids" => {
                let ids: Vec<Uuid> = serde_json::from_str(&cvalue)?;
                Ok(Some(PermissionConstraint::ResourceIds(ids)))
            }
            "environments" => {
                let envs: Vec<String> = serde_json::from_str(&cvalue)?;
                Ok(Some(PermissionConstraint::Environments(envs)))
            }
            "group_ids" => {
                let ids: Vec<Uuid> = serde_json::from_str(&cvalue)?;
                Ok(Some(PermissionConstraint::GroupIds(ids)))
            }
            _ => anyhow::bail!("Unknown constraint type: {}", ctype),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_conversion() {
        let (scope_type, scope_value) = scope_to_db(&Scope::All);
        assert_eq!(scope_type, "all");
        assert!(scope_value.is_none());

        let scope = db_to_scope(&scope_type, scope_value).unwrap();
        assert_eq!(scope, Scope::All);
    }

    #[test]
    fn test_environment_scope_conversion() {
        let (scope_type, scope_value) = scope_to_db(&Scope::Environment("production".to_string()));
        assert_eq!(scope_type, "environment");
        assert_eq!(scope_value, Some("production".to_string()));

        let scope = db_to_scope(&scope_type, scope_value).unwrap();
        assert_eq!(scope, Scope::Environment("production".to_string()));
    }

    #[test]
    fn test_parse_resource() {
        assert_eq!(parse_resource("nodes").unwrap(), Resource::Nodes);
        assert_eq!(parse_resource("groups").unwrap(), Resource::Groups);
        assert!(parse_resource("invalid").is_err());
    }

    #[test]
    fn test_parse_action() {
        assert_eq!(parse_action("read").unwrap(), Action::Read);
        assert_eq!(parse_action("admin").unwrap(), Action::Admin);
        assert!(parse_action("invalid").is_err());
    }
}
