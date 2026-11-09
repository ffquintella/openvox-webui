Feature: Plugin second factors
  As a security administrator
  I want plugins to contribute second factors under the host's control
  So that new factors can be added without ever weakening sign-in

  These scenarios run the real plugin manager, the WASM runtime with small
  WAT guests and the authentication pipeline on an in-memory database.

  Background:
    Given a plugin host with the user "alice" and a policy that requires a second factor

  Scenario Outline: Unsafe factor manifests are refused
    When I validate a factor plugin manifest with <change>
    Then the manifest should be refused with "<problem>"

    Examples:
      | change                           | problem                                                       |
      | no auth:factor capability        | auth_factors need the capability auth:factor                  |
      | an optional auth:factor          | auth:factor cannot be optional                                |
      | the amr "rcv"                    | amr 'rcv' is not allowed                                      |
      | the factor id "pa:key"           | factor id 'pa:key' is invalid                                 |
      | the remote runtime               | auth_factors need the wasm or builtin runtime                 |
      | a trusted module page            | cannot contribute trusted modules                             |

  Scenario: A well-formed factor manifest is accepted with api_version 2
    When I validate a factor plugin manifest with nothing changed
    Then the manifest should be accepted with the factor "security-key"

  Scenario: An enabled plugin factor is offered under its qualified id and passes a challenge
    Given an enabled WASM factor plugin "fa" whose verify succeeds
    And alice has enrolled the factor "fa:key"
    When alice signs in with her password
    Then a challenge should be offered with the factors "fa:key"
    When alice answers the challenge with the factor "fa:key"
    Then alice should hold a session with the methods "pwd,hwk" at level 2

  Scenario: A disabled plugin factor is neither offered nor usable
    Given an enabled WASM factor plugin "fa" whose verify succeeds
    And alice has enrolled the factor "fa:key"
    When the plugin "fa" is disabled
    And alice signs in with her password
    Then the sign-in should be refused with "factor_unavailable"
    And the factor "fa:key" should not be registered
    And alice should hold no session

  Scenario Outline: A failing plugin factor never passes a challenge
    Given an enabled WASM factor plugin "fx" whose verify <behaviour>
    And alice has enrolled the factor "fx:key"
    When alice signs in with her password
    And alice answers the challenge with the factor "fx:key"
    Then the answer should be refused with "factor_unavailable"
    And alice should hold no session

    Examples:
      | behaviour                                  |
      | traps                                      |
      | runs past its timeout                      |
      | answers verified with the amr "otp"        |
      | answers verified for a record it was not given |

  Scenario: The plugin's quota makes its factor unavailable
    Given an enabled WASM factor plugin "fq" limited to 2 calls per minute whose verify succeeds
    And alice has enrolled the factor "fq:key"
    When alice signs in with her password
    And alice answers the challenge with the factor "fq:key"
    Then alice should hold a session with the methods "pwd,hwk" at level 2
    When alice signs in with her password
    And alice answers the challenge with the factor "fq:key"
    Then the answer should be refused with "factor_unavailable"

  Scenario: Repeated failures open the circuit breaker and unregister the factor
    Given an enabled WASM factor plugin "fz" whose verify traps
    And alice has enrolled the factor "fz:key"
    When the factor "fz:key" is verified 5 times
    Then the factor "fz:key" should not be registered
    And the plugin "fz" should be in the state "error"
    When alice signs in with her password
    Then the sign-in should be refused with "factor_unavailable"

  Scenario: A plugin only receives the records of its own factor
    Given an enabled WASM factor plugin "pa" that echoes its challenge input
    And an enabled WASM factor plugin "pb" that echoes its challenge input
    And alice has enrolled the factor "pa:key" 2 times
    And alice has enrolled the factor "pb:key"
    And alice has enrolled the factor "webauthn"
    When alice signs in with her password
    And alice begins the challenge with the factor "pa:key"
    Then the plugin should have received exactly alice's "pa:key" records
    When alice signs in with her password
    And alice begins the challenge with the factor "pb:key"
    Then the plugin should have received exactly alice's "pb:key" records

  Scenario: A plugin factor reads the enrolment time from the host (H-3)
    Given an enabled WASM factor plugin "pe" that echoes its enrolment input
    When alice begins enrolling the factor "pe:key"
    Then the plugin should have received the host time in its enrolment input

  Scenario: crypto.random returns fresh secure random bytes
    Given an enabled WASM plugin "rnd" with crypto:random that asks for 32 random bytes
    When the plugin runs 2 times
    Then each run should receive 32 random bytes in base64url and the runs should differ

  Scenario Outline: crypto.random refuses lengths outside 1 to 64
    Given an enabled WASM plugin "rnd" with crypto:random that asks for <len> random bytes
    When the plugin runs 1 times
    Then the host call should fail with "invalid"

    Examples:
      | len |
      | 0   |
      | 65  |

  Scenario: crypto.random needs its capability
    Given an enabled WASM plugin "rnd" without crypto:random that asks for 32 random bytes
    When the plugin runs 1 times
    Then the host call should fail with "forbidden"
