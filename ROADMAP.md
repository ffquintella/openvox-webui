# OpenVox WebUI Roadmap

This document outlines the development roadmap for OpenVox WebUI, a web interface for managing and monitoring OpenVox infrastructure.

## Vision

Provide a modern, intuitive web interface for OpenVox that enables:
- Real-time infrastructure monitoring and visualization
- Node classification using dynamic groups (similar to Puppet Enterprise)
- PuppetDB querying with graphical dashboards
- Facter-based node management and classification
- Configuration management through YAML files
- Role-based access control for secure multi-user environments

## Testing Strategy

Each phase includes specific testing requirements. Tests are organized as follows:

### Test Organization

```
tests/
├── cucumber.rs              # BDD test runner
├── integration_tests.rs     # Integration test entry point
├── common/                  # Shared test utilities
│   ├── mod.rs
│   ├── factories.rs         # Test data factories
│   ├── fixtures.rs          # Reusable test fixtures
│   ├── mocks.rs             # Mock services (PuppetDB, etc.)
│   └── test_app.rs          # Test application wrapper
├── integration/             # Integration tests
│   └── api_tests.rs
└── features/                # BDD feature files
    ├── support/
    │   └── world.rs         # Cucumber world/context
    ├── step_definitions/
    │   └── mod.rs           # Step implementations
    ├── authentication.feature
    ├── rbac.feature
    ├── nodes.feature
    ├── node_classification.feature
    ├── facter_generation.feature
    └── reports.feature
```

### Test Tags

Feature files use tags to control test execution:
- `@wip` - Work in Progress: Tests for features not yet implemented (skipped by default)
- `@smoke` - Quick smoke tests for CI
- `@slow` - Long-running tests, excluded from quick feedback loops

### Testing Commands

```bash
make test              # Run all tests (unit, BDD, frontend)
make test-unit         # Run Rust unit tests only
make test-bdd          # Run Cucumber BDD tests only
make test-frontend     # Run frontend tests (requires npm install)
cargo test             # Run all Rust tests including integration
```

### Phase Testing Guidelines

**When implementing a phase:**
1. Remove `@wip` tag from relevant feature files
2. Implement step definitions for new scenarios
3. Add unit tests for new services/models
4. Add integration tests for new API endpoints
5. Update mocks if new external services are involved
6. Ensure `make test` passes before marking phase complete

### Feature File Status

| Feature File | Phase | Status | Description |
|--------------|-------|--------|-------------|
| `reports.feature` | 1.4 | ✅ Active | Report management (baseline) |
| `authentication.feature` | 2.1 | ✅ Active | User authentication flows |
| `rbac.feature` | 2.2 | ✅ Active | Role-based access control |
| `nodes.feature` | 3 | 🚧 @wip | Node management via PuppetDB |
| `node_classification.feature` | 4 | 🚧 @wip | Classification engine |
| `facter_generation.feature` | 5 | 🚧 @wip | Facter integration |

**Legend:**
- ✅ Active - Tests run as part of `make test`
- 🚧 @wip - Work in progress, skipped until phase is implemented

---

## Versioning Strategy

This project follows [Semantic Versioning](https://semver.org/) (SemVer): `MAJOR.MINOR.PATCH`

### Version Components

| Component | When to Increment | Example |
|-----------|-------------------|---------|
| **MAJOR** | Breaking API changes, incompatible changes | `1.0.0` → `2.0.0` |
| **MINOR** | New features, backward-compatible | `0.1.0` → `0.2.0` |
| **PATCH** | Bug fixes, small improvements, each commit during development | `0.1.0` → `0.1.1` |

### Development Workflow

**For every commit during active development:**
1. Increment the PATCH version before committing
2. Update version in both `Cargo.toml` and `frontend/package.json`
3. Keep versions synchronized across both files

### Version Bump Commands

```bash
# Bump patch version (use for each commit)
make version-patch

# Bump minor version (new features)
make version-minor

# Bump major version (breaking changes)
make version-major

# Show current version
make version
```

### Version Files

Versions must be kept in sync across:
- `Cargo.toml` - Rust backend version
- `frontend/package.json` - Frontend version

### Pre-release Versions

During development phases, use pre-release identifiers:
- `0.1.0-alpha.1` - Early development
- `0.1.0-beta.1` - Feature complete, testing
- `0.1.0-rc.1` - Release candidate

### Commit Message Guidelines

Include version in commit messages when bumping:
```
feat: add user authentication (v0.2.0)
fix: resolve token refresh issue (v0.1.15)
```

---

## Code Organization Guidelines

### File Size Limits

**All source files must be kept under 1000 lines.** When a file approaches or exceeds this limit, it should be split into smaller, focused modules.

### When to Split Files

Split a file when:
1. It exceeds 800 lines (proactive) or 1000 lines (mandatory)
2. It contains multiple unrelated concerns
3. It has more than 5-6 public functions/structs that could be logically grouped
4. Tests become difficult to navigate

### How to Split Files

**For Rust backend (`src/`):**

```rust
// Before: src/services/auth.rs (1200 lines)
// After:
src/services/
├── auth/
│   ├── mod.rs           // Re-exports and module docs
│   ├── service.rs       // AuthService struct and core methods
│   ├── password.rs      // Password hashing (Argon2)
│   ├── tokens.rs        // JWT and reset token handling
│   └── queries.rs       // Database queries
```

**For TypeScript frontend (`frontend/src/`):**

```typescript
// Before: frontend/src/components/Dashboard.tsx (1100 lines)
// After:
frontend/src/components/dashboard/
├── index.ts             // Re-exports
├── Dashboard.tsx        // Main component
├── DashboardHeader.tsx  // Header section
├── DashboardWidgets.tsx // Widget container
├── NodeStatusCard.tsx   // Individual widget
└── hooks/
    └── useDashboardData.ts
```

### Module Organization Pattern

When splitting, follow this pattern:

1. **Create a directory** with the original file name
2. **Create `mod.rs`** (Rust) or `index.ts` (TypeScript) for re-exports
3. **Move related code** into focused files
4. **Maintain public API** - external imports should not change
5. **Update imports** in dependent files if necessary

### Example: Splitting a Service

```rust
// src/services/auth/mod.rs
//! Authentication service module

mod password;
mod service;
mod tokens;

pub use password::*;
pub use service::AuthService;
pub use tokens::*;
```

### Naming Conventions for Split Files

| File Type | Naming | Example |
|-----------|--------|---------|
| Main struct/service | `service.rs` or `{name}.rs` | `service.rs`, `client.rs` |
| Data types/models | `types.rs` or `models.rs` | `types.rs` |
| Database operations | `queries.rs` or `repository.rs` | `queries.rs` |
| Utility functions | `utils.rs` or `helpers.rs` | `utils.rs` |
| Constants | `constants.rs` | `constants.rs` |
| Tests | `tests.rs` or `{name}_test.rs` | `tests.rs` |

### Checking File Sizes

```bash
# Find Rust files over 800 lines
find src -name "*.rs" -exec wc -l {} + | awk '$1 > 800' | sort -rn

# Find TypeScript files over 800 lines
find frontend/src -name "*.ts" -o -name "*.tsx" -exec wc -l {} + | awk '$1 > 800' | sort -rn
```

---

## Phase 1: Foundation

### 1.1 Project Setup
- [x] Initialize repository with Apache 2.0 license
- [x] Configure Rust workspace with Axum framework
- [x] Set up React frontend with TypeScript
- [x] Configure development environment
- [x] Set up CI/CD pipeline (GitHub Actions) - Disabled by default
- [x] Configure code quality tools (clippy, rustfmt, eslint, prettier)
- [x] Create package build scripts (RPM/DEB)

### 1.2 Core Backend Architecture
- [x] Implement Axum server with basic routing
- [x] Set up configuration management (YAML-based)
- [x] Implement logging and tracing infrastructure
- [x] Create error handling framework
- [x] Set up database connection pooling (SQLx)
- [x] Implement authentication middleware (JWT)

### 1.3 RBAC Foundation (Early Implementation)
- [x] Design permission model (resources, actions, scopes)
- [x] Implement Role data model
- [x] Implement Permission data model
- [x] Create default roles (Admin, Operator, Viewer, GroupAdmin, Auditor)
- [x] Implement permission checking middleware
- [x] Create RBAC database schema and migrations

### 1.4 Testing Infrastructure
- [x] Configure Cucumber for BDD testing
- [x] Set up unit test framework with test helpers
- [x] Configure integration test environment (TestApp with temp SQLite)
- [x] Create test fixtures and factories
- [x] Set up code coverage reporting (cargo-tarpaulin)

## Phase 2: Authentication & Authorization

### 2.1 User Management
- [x] User registration and account creation
- [x] Password hashing (Argon2)
- [x] JWT token generation and validation
- [x] Token refresh mechanism
- [x] Password reset flow
- [x] Session management

### 2.x Testing Requirements
**Feature files to enable:** `authentication.feature`, `rbac.feature`

**Unit tests to add:**
- `src/services/auth.rs` - Password hashing, token generation/validation
- `src/services/user.rs` - User CRUD operations
- `src/middleware/auth.rs` - Token extraction, validation middleware

**Integration tests to add:**
- `POST /api/v1/auth/login` - Login flow
- `POST /api/v1/auth/refresh` - Token refresh
- `POST /api/v1/auth/logout` - Logout
- `POST /api/v1/users` - User creation
- `GET /api/v1/users/:id` - User retrieval
- Permission enforcement on protected endpoints

**Step definitions to implement:**
```gherkin
Given I am authenticated as an admin
Given I am authenticated as a user with role "{role}"
Given I am not authenticated
When I login with username "{user}" and password "{pass}"
Then I should receive a valid access token
Then the response status should be 401
```

**Mocks to update:**
- `MockAuthService` - For testing without real JWT validation

### 2.2 RBAC Implementation
- [x] Role assignment to users
- [x] Permission inheritance (role hierarchy)
- [x] Resource-level permissions (node groups, environments)
- [x] Action-based permissions (read, write, delete, admin)
- [x] Scope-based permissions (all, owned, specific resources)
- [x] Permission caching for performance

### 2.3 RBAC Management Tools
- [x] Role CRUD operations
- [x] Permission CRUD operations
- [x] User-Role assignment interface
- [x] Role-Permission assignment interface
- [x] Permission matrix visualization (GET /api/v1/permissions/matrix)
- [x] Bulk permission operations (POST /api/v1/permissions/bulk)

### 2.4 RBAC API Endpoints
- [x] GET /api/v1/roles - List all roles
- [x] POST /api/v1/roles - Create role
- [x] GET /api/v1/roles/:id - Get role details
- [x] PUT /api/v1/roles/:id - Update role
- [x] DELETE /api/v1/roles/:id - Delete role
- [x] GET /api/v1/roles/:id/permissions - Get role permissions
- [x] PUT /api/v1/roles/:id/permissions - Update role permissions
- [x] GET /api/v1/permissions - List all permissions
- [x] GET /api/v1/users/:id/roles - Get user roles
- [x] PUT /api/v1/users/:id/roles - Assign roles to user
- [x] GET /api/v1/users/:id/permissions - Get effective permissions

### 2.5 RBAC Frontend
- [x] Role management page
- [x] Permission management page
- [x] User role assignment interface
- [x] Permission matrix editor
- [x] Access denied handling
- [x] Permission-aware UI components

## Phase 3: PuppetDB Integration

### 3.1 PuppetDB Client
- [ ] Implement PuppetDB API client
- [ ] Support for PQL (Puppet Query Language)
- [ ] Node queries (`/pdb/query/v4/nodes`)
- [ ] Facts queries (`/pdb/query/v4/facts`)
- [ ] Reports queries (`/pdb/query/v4/reports`)
- [ ] Resources queries (`/pdb/query/v4/resources`)
- [ ] Events queries (`/pdb/query/v4/events`)
- [ ] Catalogs queries (`/pdb/query/v4/catalogs`)

### 3.x Testing Requirements
**Feature files to enable:** `nodes.feature`, `reports.feature`

**Unit tests to add:**
- `src/services/puppetdb.rs` - PQL query building, response parsing
- `src/services/cache.rs` - Cache operations, TTL handling

**Integration tests to add:**
- `GET /api/v1/nodes` - Node listing with pagination
- `GET /api/v1/nodes/:certname` - Node details
- `GET /api/v1/nodes/:certname/facts` - Node facts
- `GET /api/v1/reports` - Reports listing
- `POST /api/v1/query` - PQL query execution

**Step definitions to implement:**
```gherkin
Given a node "{certname}" exists
Given a node "{certname}" exists with facts:
When I request the node list
When I request details for node "{certname}"
Then the response should contain node "{certname}"
Then the node should have fact "{path}" with value "{value}"
```

**Mocks to update:**
- `MockPuppetDb` - Add more query types, pagination support
- Add `MockPuppetDbServer` for HTTP-level integration tests

### 3.2 Data Caching Layer
- [ ] Implement caching strategy for PuppetDB data
- [ ] Background sync jobs for data freshness
- [ ] Cache invalidation mechanisms
- [ ] Configurable cache TTLs

### 3.3 API Endpoints
- [ ] GET /api/v1/nodes - List all nodes
- [ ] GET /api/v1/nodes/:certname - Get node details
- [ ] GET /api/v1/nodes/:certname/facts - Get node facts
- [ ] GET /api/v1/nodes/:certname/reports - Get node reports
- [ ] GET /api/v1/facts - Query facts across nodes
- [ ] GET /api/v1/reports - Query reports
- [ ] POST /api/v1/query - Execute PQL queries

## Phase 4: Node Classification System

### 4.1 Classification Engine
- [ ] Design classification rule engine
- [ ] Implement fact-based matching rules
- [ ] Support for structured facts matching
- [ ] Support for trusted facts matching
- [ ] Rule operators: =, !=, ~, >, <, in, and, or
- [ ] Rule inheritance from parent groups

### 4.x Testing Requirements
**Feature files to enable:** `node_classification.feature`

**Unit tests to add:**
- `src/services/classification.rs` - Rule evaluation, group matching
- `src/models/classification.rs` - Rule operators, match types

**Integration tests to add:**
- `POST /api/v1/groups` - Create node group
- `GET /api/v1/groups/:id` - Get group details
- `POST /api/v1/groups/:id/rules` - Add classification rule
- `POST /api/v1/classify/:certname` - Classify node
- `GET /api/v1/nodes/:certname/groups` - Get node's groups

**Step definitions to implement:**
```gherkin
Given a node group "{name}" exists
Given a node group "{name}" exists with parent "{parent}"
Given a classification rule "{rule}" on group "{group}"
When I create a node group named "{name}"
When I add a rule "{rule}" to group "{group}"
When I classify node "{certname}"
When I pin node "{certname}" to group "{group}"
Then the group "{name}" should exist
Then node "{certname}" should be classified in group "{group}"
Then the classification should include class "{class}"
```

**Test scenarios to cover:**
- Rule matching: equals, regex, greater than, in array
- Group hierarchy and inheritance
- Pinned nodes override rules
- Multiple groups matching same node
- Rule match types: ALL vs ANY

### 4.2 Node Groups Management
- [ ] Create node group data model
- [ ] Implement group hierarchy (parent/child)
- [ ] Dynamic group membership based on rules
- [ ] Static (pinned) node assignment
- [ ] Group inheritance for classes and parameters
- [ ] **RBAC: Group-level permissions**

### 4.3 API Endpoints
- [ ] CRUD /api/v1/groups - Node groups management
- [ ] GET /api/v1/groups/:id/nodes - Get nodes in group
- [ ] POST /api/v1/groups/:id/rules - Add classification rules
- [ ] GET /api/v1/nodes/:certname/groups - Get node's groups
- [ ] POST /api/v1/classify/:certname - Classify a node

## Phase 5: Facter Integration

### 5.1 Facter Data Management
- [ ] Implement Facter data parser
- [ ] Support for core facts
- [ ] Support for custom facts
- [ ] Support for external facts
- [ ] Fact history tracking

### 5.x Testing Requirements
**Feature files to enable:** `facter_generation.feature`

**Unit tests to add:**
- `src/services/facter.rs` - Fact generation, template rendering, export formats

**Integration tests to add:**
- `GET /api/v1/facter/templates` - List templates
- `POST /api/v1/facter/generate` - Generate facts
- `GET /api/v1/facter/export/:certname` - Export facts

**Step definitions to implement:**
```gherkin
Given a fact template "{name}" exists
When I generate facts for node "{certname}" using template "{template}"
When I export facts for node "{certname}" in "{format}" format
Then the generated facts should include "{fact_name}"
Then the exported facts should be valid "{format}"
```

**Export formats to test:**
- JSON export
- YAML export
- Shell script export (FACTER_* variables)

### 5.2 Facter Generation
- [ ] Design facter generation templates
- [ ] Generate external facts from classifications
- [ ] Export facts in JSON/YAML formats
- [ ] Fact validation and schema enforcement

### 5.3 API Endpoints
- [ ] GET /api/v1/facter/templates - List fact templates
- [ ] POST /api/v1/facter/generate - Generate facts for node
- [ ] GET /api/v1/facter/export/:certname - Export node facts

## Phase 6: Dashboard & Visualization

### 6.1 React Frontend Foundation
- [ ] Set up React with TypeScript
- [ ] Configure state management (Zustand/Redux)
- [ ] Implement routing (React Router)
- [ ] Set up UI component library
- [ ] Implement authentication flow
- [ ] Create responsive layout system
- [ ] **RBAC: Permission-aware routing**

### 6.x Testing Requirements
**Frontend tests location:** `frontend/tests/`

**Component tests to add (Vitest + React Testing Library):**
- `LoginForm.test.tsx` - Login form validation, submission
- `NodeList.test.tsx` - Node listing, pagination, filtering
- `GroupEditor.test.tsx` - Group CRUD operations
- `Dashboard.test.tsx` - Widget rendering, data loading
- `PermissionGuard.test.tsx` - Permission-based rendering

**E2E tests to add (Playwright/Cypress):**
- Login flow and session handling
- Node browsing and search
- Group management workflow
- Dashboard interactions

**Test utilities to create:**
```typescript
// frontend/tests/utils/
mockApi.ts          // API response mocking
renderWithProviders.ts  // Component wrapper with store/router
testFixtures.ts     // Reusable test data
```

**Frontend testing commands:**
```bash
make test-frontend    # Run Vitest tests
npm run test:e2e      # Run E2E tests (when configured)
npm run test:coverage # Coverage report
```

### 6.2 Dashboard Components
- [ ] Node overview dashboard
- [ ] Node health status indicators
- [ ] Recent activity timeline
- [ ] Quick search functionality
- [ ] Filtering and sorting controls

### 6.3 Visualization & Graphics
- [ ] Node status distribution charts (pie/donut)
- [ ] Report success/failure trends (line charts)
- [ ] Resource change heatmaps
- [ ] Node group membership visualization
- [ ] Fact distribution histograms
- [ ] Infrastructure topology graph
- [ ] Time-series metrics charts

### 6.4 Node Detail Views
- [ ] Node summary page
- [ ] Facts browser with search
- [ ] Report history with diff view
- [ ] Resource catalog viewer
- [ ] Group membership display
- [ ] Classification rule matches

## Phase 7: Configuration Management

### 7.1 YAML Configuration System
- [ ] Application configuration schema
- [ ] PuppetDB connection settings
- [ ] Authentication configuration
- [ ] Node group definitions
- [ ] Classification rules definitions
- [ ] Dashboard layout preferences
- [ ] **RBAC configuration in YAML**

### 7.x Testing Requirements
**Unit tests to add:**
- `src/config/mod.rs` - Configuration parsing, validation, defaults
- `src/config/schema.rs` - Schema validation

**Integration tests to add:**
- Configuration loading from files
- Environment variable overrides
- Invalid configuration handling
- Configuration hot-reload (if implemented)

**Test configurations to create:**
```
tests/fixtures/configs/
├── valid_minimal.yaml      # Minimum required config
├── valid_full.yaml         # All options specified
├── invalid_missing.yaml    # Missing required fields
├── invalid_types.yaml      # Wrong types
└── puppetdb_variants.yaml  # Various PuppetDB configs
```

### 7.2 Configuration UI
- [ ] Settings management interface
- [ ] YAML editor with validation
- [ ] Configuration import/export
- [ ] Configuration versioning

## Phase 8: Advanced Features

### 8.1 Reporting & Analytics
- [ ] Custom report builder
- [ ] Scheduled report generation
- [ ] Report export (PDF, CSV, JSON)
- [ ] Compliance reporting
- [ ] Drift detection reports

### 8.x Testing Requirements
**Unit tests to add:**
- `src/services/reporting.rs` - Report generation, scheduling
- `src/services/alerting.rs` - Alert rules, notifications

**Integration tests to add:**
- Report generation endpoints
- Webhook delivery (with mock server)
- Alert triggering conditions

**Feature file to add:** `reporting.feature`
```gherkin
Feature: Reporting
  Scenario: Generate compliance report
    Given nodes exist with various compliance states
    When I generate a compliance report
    Then the report should include all non-compliant nodes

  Scenario: Schedule recurring report
    When I schedule a daily report for "compliance"
    Then the report should be generated at the scheduled time
```

**Notification testing:**
- Mock webhook endpoint for testing deliveries
- Email sending tests (with mock SMTP)

### 8.2 Alerting & Notifications
- [ ] Alert rule configuration
- [ ] Webhook notifications
- [ ] Email notifications
- [ ] Slack/Teams integration
- [ ] Alert history and acknowledgment

### 8.3 Multi-tenancy & Advanced RBAC
- [ ] Organization/tenant support
- [ ] Tenant isolation
- [ ] Cross-tenant admin roles
- [ ] Environment-based permissions
- [ ] API key management with scoped permissions
- [ ] Comprehensive audit logging

## Phase 9: Production Readiness

### 9.1 Performance Optimization
- [ ] Database query optimization
- [ ] API response caching
- [ ] Frontend bundle optimization
- [ ] Lazy loading implementation
- [ ] Permission caching optimization

### 9.x Testing Requirements
**Performance tests to add:**
```
tests/performance/
├── load_test.rs        # Concurrent request handling
├── query_bench.rs      # Database query benchmarks
└── classification_bench.rs  # Classification engine benchmarks
```

**Load testing scenarios:**
- 100 concurrent users browsing nodes
- Bulk classification of 1000 nodes
- Large PuppetDB query results (10k+ nodes)

**Security tests to add:**
- SQL injection attempts
- XSS payload handling
- JWT tampering
- Rate limiting verification
- CORS policy enforcement

**Package testing:**
```bash
# Test RPM installation
make test-rpm    # Install in container, verify service starts

# Test DEB installation
make test-deb    # Install in container, verify service starts

# Test Puppet module
make test-puppet # Apply module, verify configuration
```

**Smoke tests for packages:**
```gherkin
Feature: Package Installation
  Scenario: RPM installs and starts service
    Given a fresh RHEL 8 system
    When I install the openvox-webui RPM
    Then the service should be running
    And the API should respond to health checks

  Scenario: Puppet module configures application
    Given a fresh system with Puppet agent
    When I apply the openvox::webui class
    Then the configuration file should exist
    And the service should be running
```

### 9.2 Security Hardening
- [ ] Security audit
- [ ] OWASP compliance review
- [ ] Rate limiting
- [ ] Input sanitization review
- [ ] SSL/TLS configuration
- [ ] RBAC security review

### 9.3 Package Building
- [ ] Build system for native packages
- [ ] RPM package for RHEL/CentOS/Fedora/Rocky
- [ ] DEB package for Debian/Ubuntu
- [ ] Systemd service unit files
- [ ] Package signing and repository setup
- [ ] Package metadata and dependencies

### 9.4 Puppet Module
- [ ] Create `openvox-webui` Puppet module
- [ ] Module parameters for configuration
- [ ] Service management (install, configure, service)
- [ ] Template-based configuration file generation
- [ ] Support for RHEL and Debian family OS
- [ ] Hiera integration for hierarchical configuration
- [ ] PuppetDB connection auto-configuration
- [ ] RBAC initial setup via Puppet
- [ ] Module documentation and examples
- [ ] Publish to Puppet Forge

### 9.5 Installation & Documentation
- [ ] Installation documentation for packages
- [ ] Puppet module usage guide
- [ ] Configuration reference
- [ ] Upgrade procedures
- [ ] Backup and restore procedures

## Default Roles & Permissions

### Built-in Roles

| Role | Description | Typical Permissions |
|------|-------------|---------------------|
| **Admin** | Full system access | All permissions on all resources |
| **Operator** | Day-to-day operations | Read/write nodes, groups, reports; read-only settings |
| **Viewer** | Read-only access | Read all resources, no modifications |
| **Group Admin** | Manage specific groups | Full access to assigned node groups only |
| **Auditor** | Compliance and audit | Read all resources, access audit logs |

### Permission Matrix

| Resource | Actions | Scopes |
|----------|---------|--------|
| nodes | read, classify | all, environment, group |
| groups | read, create, update, delete | all, owned, specific |
| reports | read, export | all, environment |
| facts | read, generate, export | all, environment |
| users | read, create, update, delete | all, self |
| roles | read, create, update, delete | all |
| settings | read, update | all |
| audit_logs | read | all |

## Future Considerations

- OpenBolt integration for orchestration
- Real-time WebSocket updates
- GraphQL API support
- Plugin/extension system
- Custom dashboard builder
- Mobile-responsive design improvements
- Internationalization (i18n)
- LDAP/SAML/OIDC integration for SSO
- Fine-grained attribute-based access control (ABAC)

## Version Milestones

| Version | Phase | Key Deliverables |
|---------|-------|------------------|
| 0.1.x   | 1     | Basic backend structure, auth foundation, RBAC foundation |
| 0.2.x   | 2     | Full authentication, RBAC implementation & management tools |
| 0.3.x   | 3     | PuppetDB integration, node listing |
| 0.4.x   | 4     | Node classification system with RBAC |
| 0.5.x   | 5     | Facter integration |
| 0.6.x   | 6     | Dashboard and visualizations |
| 0.7.x   | 7     | Configuration management UI |
| 0.8.x   | 8     | Reporting, alerting, multi-tenancy |
| 0.9.x   | 9.1-9.3 | Performance, security, RPM/DEB packages |
| 1.0.x   | 9.4-9.5 | Puppet module, full documentation, production-ready |

## References

- [OpenVox Project](https://voxpupuli.org/openvox/)
- [OpenVox GitHub](https://github.com/openvoxproject)
- [PuppetDB API Documentation](https://puppet.com/docs/puppetdb/latest/api/)
- [Puppet Node Classification](https://www.puppet.com/docs/pe/2023.8/grouping_and_classifying_nodes.html)
- [NIST RBAC Model](https://csrc.nist.gov/projects/role-based-access-control)
