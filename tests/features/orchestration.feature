Feature: OpenBolt orchestration
  As a platform administrator
  I want to run allowlisted OpenBolt tasks and plans on nodes under RBAC
  So that fleet changes are controlled, audited and never run by surprise

  These scenarios send real HTTP requests through the API router with the
  authentication middleware. The run lifecycle (approval, limits, redaction,
  cancellation, timeouts) is covered by the service tests with a fake bolt.

  Scenario: The feature is off until it is enabled
    Given an orchestration server with OpenBolt disabled
    When the administrator asks for the OpenBolt status
    Then the answer should be 200 with state "disabled"
    When the administrator requests a task run on "web1.example.com"
    Then the answer should be 503

  Scenario: Only administrators manage the allowlist
    Given an orchestration server with OpenBolt enabled
    When the administrator allows the task "package" requiring approval
    Then the answer should be 200
    And the allowlist should contain the task "package" requiring approval
    And the orchestration audit log should record "bolt_allowlist.updated"

  Scenario Outline: Unsafe allowlist entries are refused
    Given an orchestration server with OpenBolt enabled
    When the administrator allows the <kind> "<name>"
    Then the answer should be 400

    Examples:
      | kind    | name         |
      | task    | Bad Name     |
      | task    | a;b          |
      | plan    | ../x         |
      | command | uptime       |

  Scenario: A viewer cannot read or run anything
    Given an orchestration server with OpenBolt enabled
    When the viewer asks for the OpenBolt status
    Then the answer should be 403
    When the viewer requests a task run on "web1.example.com"
    Then the answer should be 403
    When the viewer lists the OpenBolt jobs
    Then the answer should be 403

  Scenario: A task that is not on the allowlist cannot be run, and commands stay off
    Given an orchestration server with OpenBolt enabled
    When the administrator requests a task run on "web1.example.com"
    Then the answer should not be 201
    When the administrator requests a command run on "web1.example.com"
    Then the answer should not be 201

  Scenario: The plugin platform knows the new interceptor and slot
    Given an orchestration server with OpenBolt enabled
    Then the interceptor catalog should list "before.bolt.job.run" failing closed
