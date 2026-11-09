Feature: Declarative plugin panels in the authentication slots
  As a platform administrator
  I want plugin panels on the profile security and authentication administration
  pages to be host-rendered specs, validated at install and bound to the plugin's
  own routes
  So that a plugin cannot inject markup, reach other endpoints or show
  administration data to users who may not see it

  These scenarios send real HTTP requests through the API router and the
  authentication middleware to a first-party plugin that mirrors the slots and
  routes of the mfa-fido2 module, with its example declarative specs.

  Background:
    Given a plugin host for declarative panels with the viewer "alice" and an administrator

  Scenario: The authentication slots are mounted and render host components only
    When alice reads the slot catalog
    Then the slot "profile.security" should be mounted and declarative only
    And the slot "admin.auth" should be mounted, declarative only and gated by a permission

  Scenario: The example specs of the mfa-fido2 module pass preflight
    When the administrator preflights the security key plugin unchanged
    Then the preflight should be accepted

  Scenario Outline: Unsafe declarative specs are refused at preflight with their location
    When the administrator preflights the security key plugin with <change>
    Then the preflight should be refused with "<problem>"

    Examples:
      | change                                       | problem                                                                          |
      | a source route the plugin does not declare   | spec.blocks[1].source.route: GET /admin/secrets is not one of the plugin's api_routes |
      | an action with another method                | PUT /me/credentials/{credential_ref} is not one of the plugin's api_routes       |
      | an unknown key "html"                        | spec.blocks[1]: unknown key 'html'                                               |
      | an external markdown link                    | links may only point to host routes                                              |
      | a javascript markdown link                   | links may only point to host routes                                              |
      | a danger action without a confirmation       | a danger or DELETE action needs a confirm                                        |
      | a spec of 70 KiB                             | the spec is larger than 64 KiB                                                   |
      | an iframe in profile.security                | the slot profile.security only accepts render: declarative                       |
      | an admin.auth panel without requires         | the slot admin.auth needs a requires permission                                  |

  Scenario: Users see their security panel but not the administration panel
    Given the security key plugin is enabled
    When alice lists the UI slots
    Then the slot "profile.security" should list "security-keys"
    And the slot "admin.auth" should list nothing
    When alice fetches the contribution "security-key-admin"
    Then the response status should be 404
    When alice requests GET "/api/v1/plugins/mfa-fido2/admin/summary" from the plugin
    Then the response status should be 403
    When the administrator lists the UI slots
    Then the slot "admin.auth" should list "security-key-admin"

  Scenario: A contribution tells each caller which of its routes they may use
    Given the security key plugin is enabled
    When alice fetches the contribution "security-keys"
    Then the response status should be 200
    And the contribution spec should be the example "profile-security"
    And the route "GET /me/credentials" should be allowed
    And the route "PATCH /me/credentials/{credential_ref}" should be allowed
    When the administrator fetches the contribution "security-key-admin"
    Then the response status should be 200
    And the route "POST /admin/credentials/{credential_ref}/suspend" should be allowed
    And the route "POST /admin/credentials/{credential_ref}/unsuspend" should be allowed and need step-up
