Feature: Plugin API routes
  As a platform administrator
  I want plugins to serve API routes only under the host's control
  So that a plugin endpoint never sees credentials, never bypasses
  permissions, step-up or rate limits, and never replaces a core route

  These scenarios send real HTTP requests through the API router with the
  authentication middleware, to a WASM plugin built from a small WAT guest
  ("keys") and to a first-party probe plugin that counts its calls ("probe").

  Background:
    Given a plugin host serving API routes with the viewer "alice" and an administrator

  Scenario Outline: Unsafe route manifests are refused
    When I validate a route plugin manifest with <change>
    Then the route manifest should be refused with "<problem>"

    Examples:
      | change                                   | problem                                               |
      | no api:routes capability                 | api_routes need the capability api:routes             |
      | an optional api:routes                   | api:routes cannot be optional                         |
      | the path "/config"                       | would shadow the core routes                          |
      | the path "/{ref}/x"                      | must start with a fixed segment                       |
      | the remote runtime                       | api_routes need the wasm or builtin runtime           |
      | the plugin id "trust"                    | plugin id 'trust' cannot serve api_routes             |
      | the method "TRACE"                       | method 'TRACE' is not supported                       |
      | the rate limit "60/day"                  | rate_limit '60/day' is invalid                        |
      | the permission "plugin.other.admin:read" | must be a core resource or start with plugin.keys.    |
      | the permission "nodes:fly"               | is not a core permission                              |
      | a headers key                            | unknown field                                         |

  Scenario: The routes of the mfa-fido2 module are accepted with api:routes
    When I validate a route plugin manifest with the mfa-fido2 routes
    Then the route manifest should be accepted

  Scenario: A route call carries the caller's delegated identity and no credentials
    Given the WASM route plugin "keys" is enabled
    When alice calls PATCH "/api/v1/plugins/keys/me/credentials/k%201?limit=5" with her token in the query, a cookie and the body:
      """
      {"nickname": "Desk"}
      """
    Then the response status should be 200
    And the route response should not be cached
    And the plugin should have received the handler "echo" of kind "route"
    And the plugin input should name alice with the roles "viewer"
    And the plugin input should have the params '{"credential_ref": "k 1"}', the query '{"limit": "5"}' and the body '{"nickname": "Desk"}'
    And the plugin input should hold only "method,route,params,query,body,user,request_id,now_unix"
    And the plugin input should not contain alice's token, the cookie or any header

  Scenario Outline: The plugin sets the status within the allowed set
    Given the WASM route plugin "keys" is enabled
    When alice calls <method> "<path>"
    Then the response status should be <status>
    And the route response body should be <body>

    Examples:
      | method | path                             | status | body                                                   |
      | POST   | /api/v1/plugins/keys/conflict    | 409    | '{"error": {"code": "already_suspended"}}'             |
      | DELETE | /api/v1/plugins/keys/items/k1    | 204    | empty                                                  |
      | GET    | /api/v1/plugins/keys/redirect    | 503    | the generic plugin error                               |
      | GET    | /api/v1/plugins/keys/unauthorized | 503   | the generic plugin error                               |
      | GET    | /api/v1/plugins/keys/headers     | 503    | the generic plugin error                               |

  Scenario Outline: A trapping or hanging handler fails closed with a generic error
    Given the WASM route plugin "keys" is enabled
    When alice calls GET "/api/v1/plugins/keys/<path>"
    Then the response status should be 503
    And the route response body should be the generic plugin error
    And the route response should not mention "<detail>"

    Examples:
      | path | detail      |
      | boom | unreachable |
      | spin | timed out   |

  Scenario: The manifest rate limit applies per route and per user
    Given the builtin route plugin "probe" is enabled
    When alice calls GET "/api/v1/plugins/probe/limited" 2 times
    Then the response status should be 200
    When alice calls GET "/api/v1/plugins/probe/limited"
    Then the response status should be 429
    And the route response code should be "rate_limited"
    And the route response should have a Retry-After header
    When the administrator calls GET "/api/v1/plugins/probe/limited"
    Then the response status should be 200
    And the probe should have handled 3 calls

  Scenario: A step-up route needs a session at the MFA level
    Given the builtin route plugin "probe" is enabled
    And alice has enrolled an authenticator app
    When alice calls POST "/api/v1/plugins/probe/sensitive"
    Then the response status should be 401
    And the route response code should be "step_up_required"
    And the probe should have handled 0 calls
    When alice's session reaches the MFA level
    And alice calls POST "/api/v1/plugins/probe/sensitive"
    Then the response status should be 200
    And the probe should have handled 1 calls

  Scenario: The host enforces the route's permission before the plugin runs
    Given the builtin route plugin "probe" is enabled
    When alice calls GET "/api/v1/plugins/probe/admin/summary"
    Then the response status should be 403
    And the route response code should be "forbidden"
    And the probe should have handled 0 calls
    When the administrator calls GET "/api/v1/plugins/probe/admin/summary"
    Then the response status should be 200
    And the probe should have handled 1 calls

  Scenario Outline: Routes of a plugin that cannot run answer 404
    Given the WASM route plugin "keys" is enabled
    When <event>
    And alice calls PATCH "/api/v1/plugins/keys/me/credentials/k1"
    Then the response status should be 404
    And the route response code should be "not_found"

    Examples:
      | event                                           |
      | the route plugin "keys" is disabled             |
      | the route plugin "keys" is uninstalled          |
      | the package of "keys" fails its integrity check |

  Scenario: Unknown plugins and routes answer the same 404
    Given the WASM route plugin "keys" is enabled
    When alice calls GET "/api/v1/plugins/keys/nothing/here"
    Then the response status should be 404
    When alice calls GET "/api/v1/plugins/nope/me/credentials"
    Then the response status should be 404
    And the route response code should be "not_found"

  Scenario: Plugin routes never shadow core routes
    Given the builtin route plugin "probe" is enabled
    When the administrator calls GET "/api/v1/plugins/probe/config"
    Then the response status should be 200
    When the administrator calls GET "/api/v1/plugins/probe/jobs/x"
    Then the response status should be 404
    When the administrator calls GET "/api/v1/plugins/trust/keys"
    Then the response status should be 200
    When alice calls GET "/api/v1/plugins/probe/me%2Fx"
    Then the response status should be 404
    When alice calls GET "/api/v1/plugins/probe/me/x"
    Then the response status should be 200
    And the probe should have handled 1 calls

  Scenario: Bodies are bounded and must be JSON before the plugin runs
    Given the builtin route plugin "probe" is enabled
    When alice calls POST "/api/v1/plugins/probe/items" with a body of 300 KiB
    Then the response status should be 413
    And the route response code should be "request_too_large"
    When alice calls POST "/api/v1/plugins/probe/items" with a text/plain body
    Then the response status should be 415
    When alice calls POST "/api/v1/plugins/probe/items" with the body:
      """
      {"broken":
      """
    Then the response status should be 400
    And the route response code should be "invalid_json"
    And the probe should have handled 0 calls

  Scenario: Audit entries written during a route call carry the calling user
    Given the WASM route plugin "keys" is enabled
    When alice calls POST "/api/v1/plugins/keys/audit"
    Then the response status should be 201
    And the audit entry "plugin.keys.probe" should name alice as the user
    And the host should have audited "plugin.route.invoked" for alice with the status 201
