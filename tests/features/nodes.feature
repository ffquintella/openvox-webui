Feature: Node Management
  As an infrastructure administrator
  I want to view and manage nodes in my infrastructure
  So that I can monitor their status and configuration

  Background:
    Given I am authenticated as an admin

  Scenario: List all nodes
    Given a node "node1.example.com" exists
    And a node "node2.example.com" exists
    When I request the node list
    Then the response status should be 200

  Scenario: Get node details
    Given a node "web1.example.com" exists
    When I request details for node "web1.example.com"
    Then the response status should be 200
    And the response should contain node "web1.example.com"

  Scenario: Node not found
    When I request details for node "nonexistent.example.com"
    Then the response status should be 404
    And the response should contain an error

  Scenario: Get node facts
    Given a node "web1.example.com" exists with facts:
      """
      {
        "os": {
          "family": "RedHat",
          "release": { "major": "8" }
        },
        "networking": {
          "ip": "192.168.1.100"
        }
      }
      """
    When I request details for node "web1.example.com"
    Then the response status should be 200
    And the node should have fact "os.family" with value "RedHat"
