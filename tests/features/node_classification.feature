@wip
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
