# Serial: the many password hashes would slow concurrent timing-sensitive
# scenarios (plugin handler timeouts).
@serial
Feature: Enrollment grace after a factor reset
  As a security administrator
  I want a user whose second factors I reset to enroll a new one by themselves
  So that the MFA policy stays enforced without the break-glass path

  These scenarios run the real router, authentication middleware and
  pipeline (OQ-12) on a temporary database.

  Scenario: Resetting factors opens an audited grace window
    Given a server with an enrollment grace window of 3600 seconds
    And a policy that requires a second factor for everyone
    And the user "dana" whose authenticator an administrator reset
    Then the reset answer should name the grace window expiry
    And dana should have an open grace window granted by the administrator
    And the audit log should record "auth.enrollment_grace_granted" with the reason "factor_reset"

  Scenario: A password login during the window yields an enrollment-only session
    Given a server with an enrollment grace window of 3600 seconds
    And a policy that requires a second factor for everyone
    And the user "dana" whose authenticator an administrator reset
    When dana logs in with her password
    Then dana should get an enrollment-only session at level 1 without a refresh token
    And the audit log should record "auth.enrollment_grace_login"

  Scenario: The enrollment routes are open to the enrollment-only session
    Given a server with an enrollment grace window of 3600 seconds
    And a policy that requires a second factor for everyone
    And the user "dana" whose authenticator an administrator reset
    When dana logs in with her password
    And dana calls "GET /api/v1/users/me/factors" with that session
    Then the call should succeed

  Scenario Outline: An enrollment-only session is refused outside enrollment
    Given a server with an enrollment grace window of 3600 seconds
    And a policy that requires a second factor for everyone
    And the user "dana" whose authenticator an administrator reset
    When dana logs in with her password
    And dana calls "<request>" with that session
    Then the call should be refused with "enrollment_only_session"

    Examples:
      | request                                                        |
      | GET /api/v1/auth/me                                            |
      | POST /api/v1/auth/step-up                                      |
      | PUT /api/v1/auth/policies                                      |
      | GET /api/v1/nodes                                              |
      | POST /api/v1/groups                                            |
      | GET /api/v1/users                                              |
      | POST /api/v1/api-keys                                          |
      | GET /api/v1/plugins                                            |
      | DELETE /api/v1/users/me/factors/00000000-0000-0000-0000-00000000000f |
      | POST /api/v1/users/me/factors                                  |
      | POST /api/v1/users/me/factors/recovery/enroll/begin            |

  Scenario: Enrolling a factor ends the window and the restricted session
    Given a server with an enrollment grace window of 3600 seconds
    And a policy that requires a second factor for everyone
    And the user "dana" whose authenticator an administrator reset
    When dana logs in with her password
    And dana enrolls an authenticator app with that session
    Then the enrollment should ask dana to sign in again
    And dana's enrollment-only session should be revoked
    And dana's grace window should be closed as "enrolled"
    And the audit log should record "auth.enrollment_grace_consumed"
    When dana logs in with her password
    Then dana should be offered a second factor challenge

  Scenario: An expired window denies the login and is audited
    Given a server with an enrollment grace window of 3600 seconds
    And a policy that requires a second factor for everyone
    And the user "dana" whose authenticator an administrator reset
    And dana's grace window has expired
    When dana logs in with her password
    Then the login should be refused with "mfa_enrollment_required"
    And dana's grace window should be closed as "expired"
    And the audit log should record "auth.enrollment_grace_expired"

  Scenario: A window admits a bounded number of logins and is single use
    Given a server with an enrollment grace window of 3600 seconds
    And a policy that requires a second factor for everyone
    And the user "dana" whose authenticator an administrator reset
    When dana logs in with her password 10 times
    And dana logs in with her password
    Then the login should be refused with "mfa_enrollment_required"
    And dana's grace window should be closed as "exhausted"
    And the audit log should record "auth.enrollment_grace_exhausted"
    When dana logs in with her password
    Then the login should be refused with "mfa_enrollment_required"

  Scenario: Without a window a user with nothing enrolled is still denied
    Given a server with an enrollment grace window of 3600 seconds
    And a policy that requires a second factor for everyone
    And the user "dana" who never enrolled a factor
    When dana logs in with her password
    Then the login should be refused with "mfa_enrollment_required"
    And dana should have no grace window

  Scenario: Disabled by configuration, a reset opens no window
    Given a server with an enrollment grace window of 0 seconds
    And a policy that requires a second factor for everyone
    And the user "dana" whose authenticator an administrator reset
    Then the reset answer should name no grace window
    When dana logs in with her password
    Then the login should be refused with "mfa_enrollment_required"
    And dana should have no grace window
    And the audit log should not record "auth.enrollment_grace_granted"

  Scenario: A policy window is granted once per account and never re-armed
    Given a server with an enrollment grace window of 3600 seconds granted once by policy
    And a policy that requires a second factor for everyone
    And the user "dana" who never enrolled a factor
    When dana logs in with her password
    Then dana should get an enrollment-only session at level 1 without a refresh token
    And the audit log should record "auth.enrollment_grace_granted" with the reason "policy"
    Given dana's grace window has expired
    When dana logs in with her password
    Then the login should be refused with "mfa_enrollment_required"
    When dana logs in with her password
    Then the login should be refused with "mfa_enrollment_required"
    And dana should have 1 grace window in total
