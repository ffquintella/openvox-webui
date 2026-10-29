Feature: Advanced classification
  As an infrastructure administrator
  I want expressions, group policies and explainable results
  So that classification stays predictable as the fleet grows

  Background:
    Given a node "web01" with facts:
      """
      {"os": {"family": "RedHat", "release": {"full": "9.3"}}, "virtual": "kvm",
       "networking": {"ip": "10.20.4.7"}}
      """

  Scenario: An expression groups nodes by a version and a network
    Given the advanced classification groups:
      """
      [{"name": "modern-internal", "classes": {"ntp": {}},
        "expression": {"all": [
          {"fn": "semver_gte", "args": [{"fact": "os.release.full"}, "9.0"]},
          {"fn": "in_cidr", "args": [{"fact": "networking.ip"}, "10.20.0.0/16"]},
          {"not": {"fact": "virtual", "op": "=", "value": "physical"}}]}},
       {"name": "legacy", "classes": {"old": {}},
        "expression": {"fn": "semver_lt", "args": [{"fact": "os.release.full"}, "9.0"]}}]
      """
    When I classify "web01" with the engine
    Then the node should be in the groups "modern-internal"
    And the class "ntp" should be present
    And the class "old" should be absent

  Scenario: Rules and an expression on the same group apply together
    Given the advanced classification groups:
      """
      [{"name": "both", "rules": [["os.family", "RedHat"]],
        "expression": {"fact": "virtual", "op": "=", "value": "physical"}}]
      """
    When I classify "web01" with the engine
    Then the node should be in no group
    And the group "both" should be explained as "not_matched"

  Scenario: The higher priority sibling wins a conflicting variable
    Given the advanced classification groups:
      """
      [{"name": "low", "priority": 10, "match_all_nodes": true, "variables": {"tier": "low"}},
       {"name": "high", "priority": 500, "match_all_nodes": true, "variables": {"tier": "high"}}]
      """
    When I classify "web01" with the engine
    Then the variable "tier" should be "high"
    And the provenance of the variable "tier" should be "low, high"

  Scenario: Exclusive siblings keep only the highest priority
    Given the advanced classification groups:
      """
      [{"name": "gold", "priority": 300, "match_all_nodes": true,
        "behavior": {"exclusive_set": "tier"}},
       {"name": "silver", "priority": 200, "match_all_nodes": true,
        "behavior": {"exclusive_set": "tier"}}]
      """
    When I classify "web01" with the engine
    Then the node should be in the groups "gold"
    And the group "silver" should be explained as "excluded"

  Scenario: A group that blocks inheritance keeps only its own classes
    Given the advanced classification groups:
      """
      [{"name": "base", "match_all_nodes": true, "classes": {"base": {}}},
       {"name": "isolated", "parent": "base", "match_all_nodes": true, "classes": {"web": {}},
        "behavior": {"inheritance": {"mode": "block"}}}]
      """
    When I classify "web01" with the engine
    Then the node should be in the groups "base, isolated"
    And the class "web" should be present
    And the class "base" should be absent

  Scenario: An independent group matches when its parent does not
    Given the advanced classification groups:
      """
      [{"name": "parent", "rules": [["os.family", "Debian"]]},
       {"name": "child", "parent": "parent", "rules": [["virtual", "kvm"]],
        "behavior": {"inheritance": {"mode": "independent"}}}]
      """
    When I classify "web01" with the engine
    Then the node should be in the groups "child"

  Scenario: Fail on conflict reports a different value for the same key
    Given the advanced classification groups:
      """
      [{"name": "base", "match_all_nodes": true, "variables": {"owner": "ops"}},
       {"name": "strict", "parent": "base", "match_all_nodes": true, "variables": {"owner": "web"},
        "behavior": {"merge": "fail_on_conflict"}}]
      """
    When I classify "web01" with the engine
    Then the variable "owner" should be "ops"
    And the classification should report a merge conflict

  Scenario: An expired group no longer matches
    Given the advanced classification groups:
      """
      [{"name": "old-rollout", "match_all_nodes": true,
        "behavior": {"expires_at": "2020-01-01T00:00:00Z"}}]
      """
    When I classify "web01" with the engine
    Then the node should be in no group
    And the group "old-rollout" should be explained as "inactive"

  Scenario: The explanation shows the values behind a decision
    Given the advanced classification groups:
      """
      [{"name": "root", "rules": [["os.family", "RedHat"]]},
       {"name": "physical-only", "parent": "root",
        "expression": {"fact": "virtual", "op": "=", "value": "physical"}},
       {"name": "never-reached", "parent": "physical-only", "match_all_nodes": true}]
      """
    When I classify "web01" with the engine
    Then the group "physical-only" should be explained as "not_matched"
    And the expression trace of the group "physical-only" should show 'virtual = "physical"'
    And the group "never-reached" should be explained as "not_evaluated"

  Scenario: A malformed expression is rejected with its location
    When I validate the expression:
      """
      {"all": [{"fn": "no_such_function", "args": []}]}
      """
    Then the expression should be rejected mentioning "expression.all[0]"
