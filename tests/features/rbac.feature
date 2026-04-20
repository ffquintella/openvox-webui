Feature: Role-Based Access Control
  As a system administrator
  I want to manage roles and permissions
  So that I can control user access to resources

  Background:
    Given I am authenticated as an admin

  # Role Management
  Scenario: List all roles
    When I request the list of roles
    Then the response status should be 200
    And the response should contain system roles "admin", "operator", "viewer"

  Scenario: Create a custom role
    When I create a role with name "team_lead" and display name "Team Lead"
    Then the response status should be 201
    And the role "team_lead" should exist

  Scenario: Cannot delete system roles
    When I try to delete the role "admin"
    Then the response status should be 403
    And the response should contain an error

  Scenario: Delete a custom role
    Given a custom role "temp_role" exists
    When I delete the role "temp_role"
    Then the response status should be 204
    And the role "temp_role" should not exist

  # Permission Management
  Scenario: Assign permissions to a role
    Given a custom role "custom_role" exists
    When I assign permission "nodes:read:all" to role "custom_role"
    Then the response status should be 200
    And role "custom_role" should have permission "nodes:read:all"

  Scenario: List available resources
    When I request the list of resources
    Then the response status should be 200
    And the response should contain resources:
      | name    |
      | nodes   |
      | groups  |
      | reports |
      | facts   |
      | users   |
      | roles   |

  Scenario: List available actions
    When I request the list of actions
    Then the response status should be 200
    And the response should contain actions:
      | name     |
      | read     |
      | create   |
      | update   |
      | delete   |
      | admin    |

  # User Role Assignment
  Scenario: Assign role to user
    Given a user "testuser" exists
    When I assign role "operator" to user "testuser"
    Then the response status should be 200
    And user "testuser" should have role "operator"

  Scenario: User inherits permissions from role
    Given a user "testuser" exists
    And user "testuser" has role "viewer"
    When I request effective permissions for user "testuser"
    Then the response should include permission "nodes:read"
    And the response should include permission "groups:read"
    And the response should include permission "reports:read"

  Scenario: Multiple roles combine permissions
    Given a user "testuser" exists
    And user "testuser" has role "viewer"
    And user "testuser" has role "auditor"
    When I request effective permissions for user "testuser"
    Then the response should include permission "audit_logs:read"

  # Permission Enforcement
  Scenario: Admin can access all resources
    Given I am authenticated as a user with role "admin"
    When I request the node list
    Then the response status should be 200

  Scenario: Viewer can read but not create
    Given I am authenticated as a user with role "viewer"
    When I request the node list
    Then the response status should be 200
    When I try to create a node group named "test-group"
    Then the response status should be 403

  Scenario: Operator can create groups
    Given I am authenticated as a user with role "operator"
    When I create a node group named "test-group"
    Then the response status should be 201

  Scenario: Unauthenticated user is denied
    Given I am not authenticated
    When I request the node list
    Then the response status should be 401

  # Role Hierarchy
  Scenario: Role inherits from parent
    Given a role "senior_operator" with parent "operator"
    And a user "testuser" has role "senior_operator"
    When I request effective permissions for user "testuser"
    Then the user should have all permissions from role "operator"

  # Scoped Permissions
  Scenario: Environment-scoped permission
    Given a user "testuser" has role "viewer"
    And user "testuser" has environment-scoped permission "production"
    When I request nodes for environment "production"
    Then the response status should be 200
    When I request nodes for environment "staging"
    Then the response status should be 403

  Scenario: Group-scoped permission
    Given a user "testuser" exists
    And a node group "webservers" exists
    And user "testuser" has group-scoped admin permission for "webservers"
    When I try to update group "webservers"
    Then the response status should be 200
    When I try to update group "databases"
    Then the response status should be 403
