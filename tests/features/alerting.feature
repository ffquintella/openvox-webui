Feature: Alert Rule Conditions
  As an infrastructure administrator
  I want to configure alert rules with conditions
  So that I can be notified of infrastructure issues automatically

  Background:
    Given I am authenticated as an admin
    And I have configured notification channels

  @smoke
  Scenario: Create alert rule with node status condition
    When I create an alert rule with the following configuration:
      | name                | Production Node Failures     |
      | description         | Alert on production failures |
      | rule_type           | NodeStatus                   |
      | severity            | Critical                     |
      | logical_operator    | AND                          |
      | enabled             | true                         |
    And I add a condition:
      | condition_type | NodeStatus                |
      | operator       | in                        |
      | value          | ["failed", "unknown"]     |
    And I add a condition:
      | condition_type | EnvironmentFilter         |
      | operator       | =                         |
      | value          | ["production"]            |
    Then the response status should be 201
    And the alert rule should have 2 conditions

  @smoke
  Scenario: Alert rule matches node with failed status in production
    Given an alert rule exists with conditions:
      | condition_type    | operator | value                  |
      | NodeStatus        | in       | ["failed"]             |
      | EnvironmentFilter | =        | ["production"]         |
    And a node "web1.example.com" exists with:
      | environment | production |
      | status      | failed     |
    When I evaluate the alert rule
    Then the rule should match 1 node
    And the matched node should be "web1.example.com"

  @smoke
  Scenario: Alert rule does not match node with success status
    Given an alert rule exists with conditions:
      | condition_type    | operator | value                  |
      | NodeStatus        | in       | ["failed"]             |
      | EnvironmentFilter | =        | ["production"]         |
    And a node "web2.example.com" exists with:
      | environment | production |
      | status      | success    |
    When I evaluate the alert rule
    Then the rule should match 0 nodes

  Scenario: Alert rule with node fact condition matches
    Given an alert rule exists with conditions:
      | condition_type | operator | value                                                     |
      | NodeFact       | <        | {"fact_path": "memory.system_mb", "data_type": "Integer", "threshold": 2048} |
    And a node "lowmem.example.com" exists with:
      | fact_path         | fact_value |
      | memory.system_mb  | 1024       |
    When I evaluate the alert rule
    Then the rule should match 1 node
    And the matched node should be "lowmem.example.com"

  Scenario: Alert rule with node fact condition does not match
    Given an alert rule exists with conditions:
      | condition_type | operator | value                                                     |
      | NodeFact       | <        | {"fact_path": "memory.system_mb", "data_type": "Integer", "threshold": 2048} |
    And a node "highmem.example.com" exists with:
      | fact_path         | fact_value |
      | memory.system_mb  | 8192       |
    When I evaluate the alert rule
    Then the rule should match 0 nodes

  Scenario: Alert rule with LastReportTime condition matches stale node
    Given an alert rule exists with conditions:
      | condition_type  | operator | value          |
      | LastReportTime  | >        | {"hours": 24}  |
    And a node "stale.example.com" exists with:
      | last_report_time | 48 hours ago |
    When I evaluate the alert rule
    Then the rule should match 1 node
    And the matched node should be "stale.example.com"

  Scenario: Alert rule with LastReportTime does not match recent node
    Given an alert rule exists with conditions:
      | condition_type  | operator | value          |
      | LastReportTime  | >        | {"hours": 24}  |
    And a node "recent.example.com" exists with:
      | last_report_time | 2 hours ago |
    When I evaluate the alert rule
    Then the rule should match 0 nodes

  Scenario: Alert rule with ConsecutiveFailures condition matches unstable node
    Given an alert rule exists with conditions:
      | condition_type       | operator | value                                     |
      | ConsecutiveFailures  | >=       | {"count": 3, "within_hours": 12}          |
    And a node "unstable.example.com" exists with 5 consecutive failed reports in the last 6 hours
    When I evaluate the alert rule
    Then the rule should match 1 node
    And the matched node should be "unstable.example.com"

  Scenario: Alert rule with ConsecutiveFailures does not match stable node
    Given an alert rule exists with conditions:
      | condition_type       | operator | value                                     |
      | ConsecutiveFailures  | >=       | {"count": 3, "within_hours": 12}          |
    And a node "stable.example.com" exists with 1 failed report followed by 2 successful reports
    When I evaluate the alert rule
    Then the rule should match 0 nodes

  Scenario: Alert rule with ConsecutiveChanges condition matches
    Given an alert rule exists with conditions:
      | condition_type      | operator | value                                     |
      | ConsecutiveChanges  | >=       | {"count": 5, "within_hours": 24}          |
    And a node "changing.example.com" exists with 7 consecutive reports with changes in the last 12 hours
    When I evaluate the alert rule
    Then the rule should match 1 node

  Scenario: Alert rule with ConsecutiveChanges does not match
    Given an alert rule exists with conditions:
      | condition_type      | operator | value                                     |
      | ConsecutiveChanges  | >=       | {"count": 5, "within_hours": 24}          |
    And a node "stable.example.com" exists with 2 consecutive reports with changes in the last 12 hours
    When I evaluate the alert rule
    Then the rule should match 0 nodes

  Scenario: Alert rule with ClassChangeFrequency condition matches
    Given an alert rule exists with conditions:
      | condition_type         | operator | value                                                           |
      | ClassChangeFrequency   | >        | {"class_name": "apache::server", "change_count": 10, "within_hours": 6} |
    And a node "webserver.example.com" exists with 15 changes to "apache::server" class in the last 4 hours
    When I evaluate the alert rule
    Then the rule should match 1 node

  Scenario: Alert rule with ClassChangeFrequency does not match
    Given an alert rule exists with conditions:
      | condition_type         | operator | value                                                           |
      | ClassChangeFrequency   | >        | {"class_name": "apache::server", "change_count": 10, "within_hours": 6} |
    And a node "webserver.example.com" exists with 5 changes to "apache::server" class in the last 4 hours
    When I evaluate the alert rule
    Then the rule should match 0 nodes

  Scenario: Complex alert rule with AND operator matches only when all conditions met
    Given an alert rule exists with logical operator "AND" and conditions:
      | condition_type    | operator | value                  |
      | NodeStatus        | in       | ["failed"]             |
      | EnvironmentFilter | =        | ["production"]         |
      | GroupFilter       | in       | ["web-servers"]        |
    And a node "web1.example.com" exists with:
      | environment | production  |
      | status      | failed      |
      | group       | web-servers |
    When I evaluate the alert rule
    Then the rule should match 1 node

  Scenario: Complex alert rule with AND operator does not match when one condition fails
    Given an alert rule exists with logical operator "AND" and conditions:
      | condition_type    | operator | value                  |
      | NodeStatus        | in       | ["failed"]             |
      | EnvironmentFilter | =        | ["production"]         |
      | GroupFilter       | in       | ["web-servers"]        |
    And a node "db1.example.com" exists with:
      | environment | production    |
      | status      | failed        |
      | group       | db-servers    |
    When I evaluate the alert rule
    Then the rule should match 0 nodes

  Scenario: Complex alert rule with OR operator matches when any condition is met
    Given an alert rule exists with logical operator "OR" and conditions:
      | condition_type | operator | value                                                     |
      | NodeFact       | <        | {"fact_path": "memory.system_mb", "data_type": "Integer", "threshold": 2048} |
      | NodeFact       | >        | {"fact_path": "processors.count", "data_type": "Integer", "threshold": 32}    |
    And a node "highmem.example.com" exists with:
      | fact_path         | fact_value |
      | memory.system_mb  | 8192       |
      | processors.count  | 64         |
    When I evaluate the alert rule
    Then the rule should match 1 node

  Scenario: Test alert rule before enabling
    Given I have created an alert rule with conditions
    When I test the alert rule
    Then the response should include matched nodes
    And the response should include evaluation time

  Scenario: Disable alert rule stops evaluation
    Given an enabled alert rule exists
    When I disable the alert rule
    And I evaluate all alert rules
    Then the disabled rule should not generate alerts
