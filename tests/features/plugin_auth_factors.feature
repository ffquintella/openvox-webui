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
      | the amr list [pin]               | amr needs one of otp, hwk, swk                                |
      | the amr list [hwk, hwk]          | amr 'hwk' is listed twice                                     |
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

  Scenario Outline: A plugin factor may report several methods or a synced key (OQ-5)
    Given an enabled WASM factor plugin "fm" with the amr <declared> that verifies with the amr <answered>
    And alice has enrolled the factor "fm:key"
    When alice signs in with her password
    And alice answers the challenge with the factor "fm:key"
    Then alice should hold a session with the methods "<methods>" at level 2

    Examples:
      | declared        | answered      | methods     |
      | swk             | "swk"         | pwd,swk     |
      | [hwk, pin]      | ["hwk","pin"] | pwd,hwk,pin |
      | [hwk, pin]      | "hwk"         | pwd,hwk     |
      | [hwk, swk, pin] | ["swk"]       | pwd,swk     |

  Scenario Outline: A plugin factor never reaches level 2 with qualifiers alone (OQ-5)
    Given an enabled WASM factor plugin "fq" with the amr [hwk, pin] that verifies with the amr <answered>
    And alice has enrolled the factor "fq:key"
    When alice signs in with her password
    And alice answers the challenge with the factor "fq:key"
    Then the answer should be refused with "factor_unavailable"
    And alice should hold no session

    Examples:
      | answered      |
      | ["pin"]       |
      | ["hwk","otp"] |
      | []            |

  Scenario: A phishing-resistant policy offers and accepts only security keys (OQ-6)
    Given the policy requires a phishing-resistant factor
    And an enabled WASM factor plugin "fa" whose verify succeeds
    And alice has enrolled the factor "totp"
    And alice has enrolled the factor "fa:key"
    When alice signs in with her password
    Then a challenge should be offered with the factors "fa:key"
    When alice answers the challenge with the factor "totp"
    Then the answer should be refused with "invalid"
    When alice answers the challenge with the factor "fa:key"
    Then alice should hold a session with the methods "pwd,hwk" at level 3

  Scenario: Without a security key a phishing-resistant policy never falls back to TOTP (OQ-6)
    Given the policy requires a phishing-resistant factor
    And an enabled WASM factor plugin "fa" whose verify succeeds
    And alice has enrolled the factor "totp"
    When alice signs in with her password
    Then the sign-in should be refused with "phishing_resistant_factor_required"
    And alice should hold no session
    When alice steps up a password session
    Then the sign-in should be refused with "phishing_resistant_factor_required"

  Scenario: Step-up under a phishing-resistant policy offers only security keys (OQ-6)
    Given the policy requires a phishing-resistant factor
    And an enabled WASM factor plugin "fa" whose verify succeeds
    And alice has enrolled the factor "totp"
    And alice has enrolled the factor "fa:key"
    When alice steps up a password session
    Then a challenge should be offered with the factors "fa:key"

  Scenario Outline: A plugin factor's error code reaches the client as a stable code (H-8)
    Given an enabled WASM factor plugin "fe" whose challenge answers the error "<kind>" "<code>"
    And alice has enrolled the factor "fe:key"
    When alice signs in with her password
    And alice answers the challenge with the factor "fe:key"
    Then the answer should be refused with "<seen>"
    And alice should hold no session

    Examples:
      | kind        | code                 | seen                 |
      | unavailable | no_usable_credential | no_usable_credential |
      | invalid     | ceremony_expired     | ceremony_expired     |
      | invalid     | Not A Code           | plugin_error         |

  Scenario: A plugin factor reads the enrolment time from the host (H-3)
    Given an enabled WASM factor plugin "pe" that echoes its enrolment input
    When alice begins enrolling the factor "pe:key"
    Then the plugin should have received the host time in its enrolment input

  Scenario: A plugin factor learns whether a challenge signs in or steps up (H-4)
    Given an enabled WASM factor plugin "pp" that echoes its challenge input
    And alice has enrolled the factor "pp:key"
    When alice signs in with her password
    And alice begins the challenge with the factor "pp:key"
    Then the plugin should have received the purpose "login"
    When alice steps up a password session
    And alice begins the challenge with the factor "pp:key"
    Then the plugin should have received the purpose "step_up"

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
