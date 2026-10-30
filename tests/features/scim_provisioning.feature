Feature: Inbound SCIM provisioning
  As an identity administrator
  I want my identity provider to provision users and groups over SCIM
  So that access to OpenVox WebUI follows the corporate directory

  These scenarios run the real SCIM filter, PATCH and mapping logic. The HTTP
  service, tokens and deactivation are covered by tests/integration/scim_tests.rs.

  Background:
    Given a SCIM user resource:
      """
      {"id": "2819c223-7f76-453a-919d-413861904646", "userName": "alice@example.com",
       "externalId": "entra-alice", "active": true,
       "emails": [{"value": "alice@example.com", "type": "work", "primary": true}]}
      """

  Scenario: A provider looks a user up by userName without regard to case
    When I evaluate the SCIM filter 'userName eq "ALICE@example.com"'
    Then the SCIM filter should match
    When I evaluate the SCIM filter 'externalId eq "ENTRA-ALICE"'
    Then the SCIM filter should not match

  Scenario: A malformed filter is refused
    When I evaluate the SCIM filter 'userName xx "alice"'
    Then the SCIM request should be refused with "invalidFilter"

  Scenario: Entra ID deactivates a user with a string boolean
    When I apply the SCIM patch:
      """
      {"schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
       "Operations": [{"op": "Replace", "path": "active", "value": "False"}]}
      """
    Then the SCIM user should be inactive

  Scenario: The userName of a user cannot be removed
    When I apply the SCIM patch:
      """
      {"schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
       "Operations": [{"op": "remove", "path": "userName"}]}
      """
    Then the SCIM request should be refused with "mutability"

  Scenario: Groups grant mapped roles but never super_admin
    Given the SCIM groups of the user:
      """
      [{"id": "6d1f0f8e-0000-4000-8000-000000000001", "display_name": "WebUI Operators",
        "external_id": "g-ops"}]
      """
    And the SCIM role mappings:
      """
      [{"kind": "display_name", "ref": "webui operators", "role_id": "r-op", "role": "operator"},
       {"kind": "external_id", "ref": "g-ops", "role_id": "r-super", "role": "super_admin"},
       {"kind": "display_name", "ref": "Other", "role_id": "r-admin", "role": "admin"}]
      """
    Then the SCIM roles granted should be "r-op"
