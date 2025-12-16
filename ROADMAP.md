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
- [ ] Design permission model (resources, actions, scopes)
- [ ] Implement Role data model
- [ ] Implement Permission data model
- [ ] Create default roles (Admin, Operator, Viewer)
- [ ] Implement permission checking middleware
- [ ] Create RBAC database schema and migrations

### 1.4 Testing Infrastructure
- [ ] Configure Cucumber for BDD testing
- [ ] Set up unit test framework
- [ ] Configure integration test environment
- [ ] Create test fixtures and factories
- [ ] Set up code coverage reporting

## Phase 2: Authentication & Authorization

### 2.1 User Management
- [ ] User registration and account creation
- [ ] Password hashing (Argon2)
- [ ] JWT token generation and validation
- [ ] Token refresh mechanism
- [ ] Password reset flow
- [ ] Session management

### 2.2 RBAC Implementation
- [ ] Role assignment to users
- [ ] Permission inheritance (role hierarchy)
- [ ] Resource-level permissions (node groups, environments)
- [ ] Action-based permissions (read, write, delete, admin)
- [ ] Scope-based permissions (all, owned, specific resources)
- [ ] Permission caching for performance

### 2.3 RBAC Management Tools
- [ ] Role CRUD operations
- [ ] Permission CRUD operations
- [ ] User-Role assignment interface
- [ ] Role-Permission assignment interface
- [ ] Permission matrix visualization
- [ ] Bulk permission operations

### 2.4 RBAC API Endpoints
- [ ] GET /api/v1/roles - List all roles
- [ ] POST /api/v1/roles - Create role
- [ ] GET /api/v1/roles/:id - Get role details
- [ ] PUT /api/v1/roles/:id - Update role
- [ ] DELETE /api/v1/roles/:id - Delete role
- [ ] GET /api/v1/roles/:id/permissions - Get role permissions
- [ ] PUT /api/v1/roles/:id/permissions - Update role permissions
- [ ] GET /api/v1/permissions - List all permissions
- [ ] GET /api/v1/users/:id/roles - Get user roles
- [ ] PUT /api/v1/users/:id/roles - Assign roles to user
- [ ] GET /api/v1/users/:id/permissions - Get effective permissions

### 2.5 RBAC Frontend
- [ ] Role management page
- [ ] Permission management page
- [ ] User role assignment interface
- [ ] Permission matrix editor
- [ ] Access denied handling
- [ ] Permission-aware UI components

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
| 0.1.0   | 1     | Basic backend structure, auth foundation, RBAC foundation |
| 0.2.0   | 2     | Full authentication, RBAC implementation & management tools |
| 0.3.0   | 3     | PuppetDB integration, node listing |
| 0.4.0   | 4     | Node classification system with RBAC |
| 0.5.0   | 5     | Facter integration |
| 0.6.0   | 6     | Dashboard and visualizations |
| 0.7.0   | 7     | Configuration management UI |
| 0.8.0   | 8     | Reporting, alerting, multi-tenancy |
| 0.9.0   | 9.1-9.3 | Performance, security, RPM/DEB packages |
| 1.0.0   | 9.4-9.5 | Puppet module, full documentation, production-ready |

## References

- [OpenVox Project](https://voxpupuli.org/openvox/)
- [OpenVox GitHub](https://github.com/openvoxproject)
- [PuppetDB API Documentation](https://puppet.com/docs/puppetdb/latest/api/)
- [Puppet Node Classification](https://www.puppet.com/docs/pe/2023.8/grouping_and_classifying_nodes.html)
- [NIST RBAC Model](https://csrc.nist.gov/projects/role-based-access-control)
