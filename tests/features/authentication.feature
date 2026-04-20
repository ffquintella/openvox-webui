Feature: Authentication
  As a user
  I want to authenticate with the system
  So that I can access protected resources

  Scenario: Access protected resource without authentication
    Given I am not authenticated
    When I request the node list
    Then the response status should be 401
    And the response should contain an error

  Scenario: Admin can access all resources
    Given I am authenticated as an admin
    When I request the node list
    Then the response status should be 200

  Scenario: Regular user can view nodes
    Given I am authenticated as a user
    When I request the node list
    Then the response status should be 200

  Scenario: Regular user cannot create groups
    Given I am authenticated as a user
    When I create a node group named "test-group"
    Then the response status should be 403
    And the response should contain an error
