Feature: Report Management
  As an infrastructure administrator
  I want to view Puppet run reports
  So that I can monitor configuration changes and failures

  Background:
    Given I am authenticated as an admin

  Scenario: List reports for a node
    Given a node "web1.example.com" exists
    And a report exists for node "web1.example.com" with status "changed"
    When I request reports for node "web1.example.com"
    Then the response status should be 200
    And the response should contain reports

  Scenario: Filter reports by status
    Given a report exists for node "web1.example.com" with status "failed"
    And a report exists for node "web2.example.com" with status "changed"
    When I request reports with status "failed"
    Then the response status should be 200
    And all reports should have status "failed"
