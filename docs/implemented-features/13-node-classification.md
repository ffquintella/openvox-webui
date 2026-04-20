# Phase 4: Node Classification System

## Completed Tasks

### 4.1 Classification Engine
- [x] Design classification rule engine
- [x] Implement fact-based matching rules
- [x] Support for structured facts matching
- [x] Support for trusted facts matching
- [x] Rule operators: =, !=, ~, !~, >, >=, <, <=, in, not_in; group-level and/or via `RuleMatchType`
- [x] Rule inheritance from parent groups

### 4.2 Node Groups Management UI
- [x] Two-column layout with groups list and detail panel
- [x] Group hierarchy visualization with parent/child relationships
- [x] Create/Edit group modal with all settings
- [x] Classification rules editor (add/remove rules with all operators)
- [x] Pinned nodes management (add/remove from available nodes)
- [x] Puppet classes editor (add/remove classes)
- [x] Class parameters editor (add/remove key-value parameters)
- [x] Matched nodes display count
- [x] Tabbed interface for rules/pinned/classes management
- [ ] **RBAC: Group-level permissions** (backend integration pending)

### 4.3 API Endpoints (Backend Implementation) - COMPLETE
- [x] CRUD /api/v1/groups - Node groups management (fully implemented)
- [x] GET /api/v1/groups/:id - Get group with rules and pinned nodes
- [x] PUT /api/v1/groups/:id - Update group (partial updates supported)
- [x] DELETE /api/v1/groups/:id - Delete group (cascades to rules/pinned)
- [x] GET /api/v1/groups/:id/nodes - Get nodes in group (returns pinned nodes)
- [x] GET /api/v1/groups/:id/rules - Get classification rules
- [x] POST /api/v1/groups/:id/rules - Add classification rule
- [x] DELETE /api/v1/groups/:id/rules/:ruleId - Delete classification rule
- [x] POST /api/v1/groups/:id/pinned - Add pinned node
- [x] DELETE /api/v1/groups/:id/pinned/:certname - Remove pinned node

## Details

Comprehensive node classification system for dynamic grouping and configuration:

### Classification Rules

Rules match nodes based on facts. Each rule specifies:
- **Fact:** The fact to match (e.g., `os.family`, `processors.count`)
- **Operator:** How to match (=, !=, ~, !~, >, >=, <, <=, in, not_in)
- **Value:** The value to match against
- **Match Type:** AND or OR for combining multiple rules

**Supported Operators:**

| Operator | Type | Example |
|----------|------|---------|
| `=` | Equality | os.family = "Debian" |
| `!=` | Inequality | os.family != "Windows" |
| `~` | Regex match | fqdn ~ "^prod-" |
| `!~` | Regex no-match | fqdn !~ "^test-" |
| `>` | Greater than | processors.count > 4 |
| `>=` | Greater or equal | memory.system_mb >= 8192 |
| `<` | Less than | processors.count < 2 |
| `<=` | Less or equal | memory.system_mb <= 4096 |
| `in` | Array contains | environment in ["prod", "staging"] |
| `not_in` | Array not contains | role not_in ["database", "cache"] |

### Group Hierarchy

- Parent-child relationships
- Multi-level inheritance
- Rule inheritance from parent groups
- Cascade deletion
- Default "All Nodes" group

### Node Pinning

- Manually pin nodes to groups
- Override rule-based classification
- Bulk pinning operations
- Visual indicators for pinned nodes

### Puppet Classes

Associate Puppet classes with groups:
- Add/remove classes
- Class parameter configuration
- Override parameters per node
- Class dependency management

### Backend Implementation

- `GroupRepository` with full CRUD for groups, rules, and pinned nodes
- SQLite database storage for all group data
- Default "All Nodes" group created by migration
- `AppError` helper methods for consistent error responses
- Request types: `CreateGroupRequest`, `UpdateGroupRequest`, `CreateRuleRequest`, `AddPinnedNodeRequest`

### Frontend UI Components

```
frontend/src/pages/
├── Classification.tsx           # Main classification page
├── classification/
│   ├── GroupsList.tsx           # Groups list panel
│   ├── GroupDetails.tsx         # Group detail panel
│   ├── GroupForm.tsx            # Create/edit modal
│   ├── RulesEditor.tsx          # Rules tab editor
│   ├── PinnedNodesEditor.tsx    # Pinned nodes tab
│   └── ClassesEditor.tsx        # Classes tab editor

frontend/src/hooks/
├── useGroups.ts                 # Group API hooks
└── useClassification.ts         # Classification hooks
```

### API Endpoints

**Group Management:**
```
GET    /api/v1/groups                # List all groups
POST   /api/v1/groups                # Create group
GET    /api/v1/groups/:id            # Get group details
PUT    /api/v1/groups/:id            # Update group
DELETE /api/v1/groups/:id            # Delete group
```

**Rules Management:**
```
GET    /api/v1/groups/:id/rules      # Get rules
POST   /api/v1/groups/:id/rules      # Add rule
DELETE /api/v1/groups/:id/rules/:ruleId  # Remove rule
```

**Pinned Nodes:**
```
GET    /api/v1/groups/:id/nodes      # Get pinned nodes
POST   /api/v1/groups/:id/pinned     # Add pinned node
DELETE /api/v1/groups/:id/pinned/:certname  # Remove pinned node
```

**Classification (Future):**
```
POST   /api/v1/classify/:certname    # Classify single node
GET    /api/v1/nodes/:certname/groups # Get node's groups
```

### Configuration Example

```yaml
groups:
  - name: "All Nodes"
    description: "Default group for all nodes"
    parent_group_id: null
    rules:
      - fact: "os.family"
        operator: "~"
        value: ".*"
        match_type: "ANY"
    classes:
      - "base"
      - "monitoring"

  - name: "Production Web Servers"
    description: "Production web server nodes"
    parent_group_id: "all-nodes-id"
    rules:
      - fact: "environment"
        operator: "="
        value: "production"
        match_type: "ALL"
      - fact: "role"
        operator: "="
        value: "web"
    classes:
      - "webserver"
      - "ssl"
```

## Future Enhancements

- GET /api/v1/nodes/:certname/groups - Get node's groups (needs fact data)
- POST /api/v1/classify/:certname - Classify a node (needs fact data)
- Group-level RBAC permissions (backend integration)

## Key Files

- `src/models/group.rs` - Group data model
- `src/models/classification_rule.rs` - Rule data model
- `src/repositories/group_repository.rs` - Group persistence
- `src/handlers/groups.rs` - Group endpoints
- `src/services/classification.rs` - Classification logic
- `frontend/src/pages/Classification.tsx` - UI
