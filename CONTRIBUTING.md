# Contributing to OpenVox WebUI

Thank you for your interest in contributing to OpenVox WebUI! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Development Workflow](#development-workflow)
- [Testing Requirements](#testing-requirements)
- [Code Style](#code-style)
- [Commit Guidelines](#commit-guidelines)
- [Pull Request Process](#pull-request-process)
- [Issue Guidelines](#issue-guidelines)

## Code of Conduct

This project follows the [Vox Pupuli Code of Conduct](https://voxpupuli.org/coc/). Please read and adhere to it in all interactions.

## Getting Started

1. Fork the repository
2. Clone your fork locally
3. Set up the development environment
4. Create a feature branch
5. Make your changes
6. Submit a pull request

## Development Setup

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Node.js 20+ and npm
- Docker and Docker Compose (for local PuppetDB)
- Git

### Initial Setup

```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/openvox-webui.git
cd openvox-webui

# Install Rust dependencies
cargo build

# Install frontend dependencies
cd frontend
npm install
cd ..

# Copy example configuration
cp config/config.example.yaml config/config.yaml

# Start development services (PuppetDB mock)
docker-compose up -d

# Run the backend
cargo run

# In another terminal, run the frontend
cd frontend
npm run dev
```

### Environment Configuration

Create a `config/config.yaml` file based on `config/config.example.yaml`:

```yaml
server:
  host: "127.0.0.1"
  port: 8080

puppetdb:
  url: "http://localhost:8081"
  timeout_secs: 30

auth:
  jwt_secret: "development-secret-change-in-production"
  token_expiry_hours: 24
```

## Development Workflow

### Branch Naming

Use descriptive branch names:

- `feature/` - New features (e.g., `feature/node-classification`)
- `fix/` - Bug fixes (e.g., `fix/puppetdb-connection-timeout`)
- `docs/` - Documentation changes (e.g., `docs/api-endpoints`)
- `refactor/` - Code refactoring (e.g., `refactor/error-handling`)
- `test/` - Test additions/modifications (e.g., `test/classification-rules`)

### Making Changes

1. Create a feature branch from `main`:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. Make your changes following our code style guidelines

3. Write tests for your changes (see Testing Requirements)

4. Run the test suite:
   ```bash
   # Run all tests
   cargo test

   # Run BDD tests
   cargo test --test cucumber

   # Run frontend tests
   cd frontend && npm test
   ```

5. Ensure code passes linting:
   ```bash
   cargo clippy -- -D warnings
   cargo fmt --check
   cd frontend && npm run lint
   ```

## Testing Requirements

### Behavior-Driven Development (BDD)

We use Cucumber for BDD testing. All features must have corresponding `.feature` files.

#### Writing Feature Files

Feature files go in `tests/features/`:

```gherkin
# tests/features/node_classification.feature
Feature: Node Classification
  As an infrastructure administrator
  I want to classify nodes into groups
  So that I can manage configuration at scale

  Scenario: Create a new node group
    Given I am authenticated as an admin
    When I create a node group named "webservers"
    Then the group "webservers" should exist
    And the group should have no nodes

  Scenario: Add a classification rule
    Given a node group "webservers" exists
    When I add a rule "os.family = RedHat"
    Then nodes with RedHat family should be classified
```

#### Step Definitions

Step definitions go in `tests/features/step_definitions/`:

```rust
// tests/features/step_definitions/classification_steps.rs
use cucumber::{given, when, then, World};

#[given("I am authenticated as an admin")]
async fn authenticated_as_admin(world: &mut TestWorld) {
    world.authenticate_admin().await;
}

#[when(expr = "I create a node group named {string}")]
async fn create_node_group(world: &mut TestWorld, name: String) {
    world.create_group(&name).await;
}
```

### Unit Tests

Every module should have corresponding unit tests:

```rust
// src/services/classification.rs
pub fn matches_rule(fact_value: &str, rule: &Rule) -> bool {
    // implementation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_rule_equality() {
        let rule = Rule::new("=", "RedHat");
        assert!(matches_rule("RedHat", &rule));
        assert!(!matches_rule("Debian", &rule));
    }

    #[test]
    fn test_matches_rule_regex() {
        let rule = Rule::new("~", "^Red.*");
        assert!(matches_rule("RedHat", &rule));
        assert!(!matches_rule("Debian", &rule));
    }
}
```

### Integration Tests

Integration tests go in `tests/integration/`:

```rust
// tests/integration/puppetdb_client_test.rs
#[tokio::test]
async fn test_puppetdb_node_query() {
    let client = setup_test_client().await;
    let nodes = client.get_nodes().await.unwrap();
    assert!(!nodes.is_empty());
}
```

### Test Coverage Requirements

- New features must have BDD scenarios
- All public functions must have unit tests
- Critical integrations must have integration tests
- Aim for >80% code coverage

## Code Style

### Rust

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for formatting
- Use `clippy` for linting
- Document public APIs with doc comments

```rust
/// Classifies a node based on its facts and returns matching groups.
///
/// # Arguments
///
/// * `certname` - The certificate name of the node
/// * `facts` - The node's facts
///
/// # Returns
///
/// A vector of group IDs that the node belongs to
///
/// # Errors
///
/// Returns an error if the classification rules cannot be evaluated
pub async fn classify_node(
    certname: &str,
    facts: &Facts,
) -> Result<Vec<GroupId>, ClassificationError> {
    // implementation
}
```

### TypeScript/React

- Use TypeScript strict mode
- Follow ESLint configuration
- Use Prettier for formatting
- Use functional components with hooks

```typescript
interface NodeListProps {
  groupId?: string;
  onNodeSelect: (certname: string) => void;
}

export const NodeList: React.FC<NodeListProps> = ({ groupId, onNodeSelect }) => {
  const { nodes, isLoading, error } = useNodes(groupId);

  if (isLoading) return <LoadingSpinner />;
  if (error) return <ErrorMessage error={error} />;

  return (
    <ul>
      {nodes.map((node) => (
        <NodeItem key={node.certname} node={node} onClick={onNodeSelect} />
      ))}
    </ul>
  );
};
```

## Commit Guidelines

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

### Examples

```
feat(classification): add regex matching for rules

Implements regex pattern matching using the ~ operator
for classification rules, allowing patterns like "os.family ~ ^Red.*"

Closes #123
```

```
fix(puppetdb): handle connection timeout gracefully

Previously, connection timeouts would cause a panic.
Now returns a proper error that can be handled by the caller.

Fixes #456
```

## Pull Request Process

1. **Before submitting:**
   - Ensure all tests pass
   - Update documentation if needed
   - Add changelog entry under `[Unreleased]`
   - Rebase on latest `main`

2. **PR Description:**
   - Describe what changes were made
   - Reference related issues
   - Include screenshots for UI changes
   - List any breaking changes

3. **Review Process:**
   - PRs require at least one approval
   - All CI checks must pass
   - Address review feedback promptly

4. **After Merge:**
   - Delete your feature branch
   - Verify changes in main

### PR Template

```markdown
## Description
Brief description of changes

## Related Issues
Closes #XXX

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
Describe tests added/modified

## Checklist
- [ ] Tests pass locally
- [ ] Code follows style guidelines
- [ ] Documentation updated
- [ ] Changelog updated
```

## Issue Guidelines

### Bug Reports

Include:
- Clear description of the bug
- Steps to reproduce
- Expected vs actual behavior
- Environment details (OS, Rust version, etc.)
- Relevant logs or screenshots

### Feature Requests

Include:
- Clear description of the feature
- Use case / motivation
- Proposed implementation (optional)
- Alternatives considered

## Questions?

- Open a [Discussion](https://github.com/openvoxproject/openvox-webui/discussions)
- Join the [Vox Pupuli Slack](https://voxpupuli.org/community/)
- Check existing issues and documentation

Thank you for contributing!
