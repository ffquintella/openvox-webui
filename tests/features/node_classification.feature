Feature: Node Classification
  As an infrastructure administrator
  I want to classify nodes into groups based on their facts
  So that I can manage configuration at scale

  Background:
    Given I am authenticated as an admin

  Scenario: Create a new node group
    When I create a node group named "webservers"
    Then the response status should be 201
    And the group "webservers" should exist
    And the group should have no nodes

  Scenario: Create a node group with parent
    Given a node group "All Servers" exists
    When I create a node group named "Production Servers" with parent "All Servers"
    Then the response status should be 201
    And the group "Production Servers" should exist

  Scenario: Delete a node group
    Given a node group "temp-group" exists
    When I delete the group "temp-group"
    Then the response status should be 204
    And the group "temp-group" should not exist

  Scenario: Add a classification rule to a group
    Given a node group "redhat_servers" exists
    When I add a rule "os.family = RedHat" to group "redhat_servers"
    Then the response status should be 201

  Scenario: Classify a node by matching rules
    Given a node group "redhat_servers" exists
    And a classification rule "os.family = RedHat" on group "redhat_servers"
    And a node "web1.example.com" exists with facts:
      """
      {
        "os": {
          "family": "RedHat",
          "release": { "major": "8" }
        }
      }
      """
    When I classify node "web1.example.com"
    Then the response status should be 200
    And node "web1.example.com" should be classified in group "redhat_servers"

  Scenario: Node does not match classification rules
    Given a node group "debian_servers" exists
    And a classification rule "os.family = Debian" on group "debian_servers"
    And a node "web1.example.com" exists with facts:
      """
      {
        "os": {
          "family": "RedHat"
        }
      }
      """
    When I classify node "web1.example.com"
    Then the response status should be 200
    And node "web1.example.com" should not be classified in any group

  Scenario: Pin a node to a group
    Given a node group "special_nodes" exists
    And a node "special1.example.com" exists
    When I pin node "special1.example.com" to group "special_nodes"
    Then the response status should be 200
    And node "special1.example.com" should be classified in group "special_nodes"

  Scenario: Classification includes inherited classes
    Given a node group "base" exists
    And a node group "webservers" exists with parent "base"
    And group "base" has class "profile::base"
    And group "webservers" has class "profile::webserver"
    And a classification rule "role = webserver" on group "webservers"
    And a node "web1.example.com" exists with facts:
      """
      {
        "role": "webserver"
      }
      """
    When I classify node "web1.example.com"
    Then the classification should include class "profile::base"
    And the classification should include class "profile::webserver"

  # Environment Group Feature Tests
  # Environment groups ASSIGN environments to nodes instead of filtering by the node's current environment

  Scenario: Node does not match group with different environment (default behavior)
    Given a node group "production_servers" exists with environment "production"
    And a classification rule "role = webserver" on group "production_servers"
    And a node "web1.example.com" exists with facts:
      """
      {
        "role": "webserver"
      }
      """
    And node "web1.example.com" has environment "staging"
    When I classify node "web1.example.com"
    Then node "web1.example.com" should not be classified in group "production_servers"

  Scenario: Environment group matches node regardless of node's current environment
    Given a node group "homolog_servers" exists with environment "Homolog"
    And group "homolog_servers" is an environment group
    And a classification rule "certname = web1.example.com" on group "homolog_servers"
    And a node "web1.example.com" exists with facts:
      """
      {
        "certname": "web1.example.com"
      }
      """
    And node "web1.example.com" has environment "production"
    When I classify node "web1.example.com"
    Then node "web1.example.com" should be classified in group "homolog_servers"
    And the classification environment should be "Homolog"

  Scenario: Environment group assigns environment to node without environment
    Given a node group "development_env" exists with environment "development"
    And group "development_env" is an environment group
    And a classification rule "role = dev" on group "development_env"
    And a node "dev1.example.com" exists with facts:
      """
      {
        "role": "dev"
      }
      """
    When I classify node "dev1.example.com"
    Then node "dev1.example.com" should be classified in group "development_env"
    And the classification environment should be "development"

  Scenario: Regular group with environment still filters by node environment
    Given a node group "prod_webservers" exists with environment "production"
    And a classification rule "role = webserver" on group "prod_webservers"
    And a node "web1.example.com" exists with facts:
      """
      {
        "role": "webserver"
      }
      """
    And node "web1.example.com" has environment "production"
    When I classify node "web1.example.com"
    Then node "web1.example.com" should be classified in group "prod_webservers"

  Scenario: Create environment group via API
    When I create a node group named "env_group" with environment "staging" as environment group
    Then the response status should be 201
    And the group "env_group" should be an environment group

  # Match All Nodes Feature Tests
  # Groups with match_all_nodes=true match all nodes when no rules exist

  Scenario: Group with match_all_nodes matches all nodes when no rules
    Given a node group "all_nodes" exists with match_all_nodes enabled
    And a node "any-node.example.com" exists with facts:
      """
      {
        "kernel": "Linux"
      }
      """
    When I classify node "any-node.example.com"
    Then node "any-node.example.com" should be classified in group "all_nodes"

  Scenario: Group without match_all_nodes and no rules matches no nodes
    Given a node group "empty_group" exists
    And a node "any-node.example.com" exists with facts:
      """
      {
        "kernel": "Linux"
      }
      """
    When I classify node "any-node.example.com"
    Then node "any-node.example.com" should not be classified in group "empty_group"

  Scenario: Child group with match_all_nodes respects parent rules
    Given a node group "linux" exists
    And a classification rule "kernel = Linux" on group "linux"
    And a node group "all_linux" exists with parent "linux" and match_all_nodes enabled
    And a node "linux-node.example.com" exists with facts:
      """
      {
        "kernel": "Linux"
      }
      """
    And a node "windows-node.example.com" exists with facts:
      """
      {
        "kernel": "Windows"
      }
      """
    When I classify node "linux-node.example.com"
    Then node "linux-node.example.com" should be classified in group "linux"
    And node "linux-node.example.com" should be classified in group "all_linux"
    When I classify node "windows-node.example.com"
    Then node "windows-node.example.com" should not be classified in group "linux"
    And node "windows-node.example.com" should not be classified in group "all_linux"
