@wip
Feature: Facter Generation
  As an infrastructure administrator
  I want to generate external facts based on classification
  So that nodes can receive additional configuration data

  Background:
    Given I am authenticated as an admin

  Scenario: Generate facts from static values
    Given a fact template "basic" exists with:
      """
      {
        "name": "basic",
        "facts": [
          {"name": "custom_role", "value": {"type": "Static", "value": "webserver"}}
        ]
      }
      """
    And a node "web1.example.com" exists
    When I generate facts for node "web1.example.com" using template "basic"
    Then the response status should be 200
    And the generated fact "custom_role" should equal "webserver"

  Scenario: Generate facts from classification
    Given a node group "webservers" exists
    And a node "web1.example.com" is classified in group "webservers"
    And a fact template "classification" exists with:
      """
      {
        "name": "classification",
        "facts": [
          {"name": "node_groups", "value": {"type": "FromClassification", "value": "groups"}}
        ]
      }
      """
    When I generate facts for node "web1.example.com" using template "classification"
    Then the response status should be 200
    And the generated fact "node_groups" should contain "webservers"

  Scenario: Export facts in different formats
    Given a node "web1.example.com" exists
    And generated facts exist for node "web1.example.com"
    When I export facts for node "web1.example.com" in format "yaml"
    Then the response status should be 200
    And the response should be valid YAML

  Scenario: Export facts as shell script
    Given a node "web1.example.com" exists
    And generated facts exist for node "web1.example.com"
    When I export facts for node "web1.example.com" in format "shell"
    Then the response status should be 200
    And the response should contain "export FACTER_"
