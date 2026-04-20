Feature: Puppet CA Management
  As an administrator
  I want to manage Puppet CA operations
  So that I can sign, reject, revoke certificates and check status

  Background:
    Given I am authenticated as an admin

  Scenario: Check CA status
    When I request CA status
    Then the response status should be 200
    And the CA status should include counts

  Scenario: List pending CSRs
    When I list pending CSRs
    Then the response status should be 200
    And the response should contain CSRs

  Scenario: Sign a CSR with DNS alt names
    When I sign CSR for "node1.example.com" with alt names "node1.example.com,node1"
    Then the response status should be 200
    And the certificate "node1.example.com" should be signed
    And the response should include dns alt names "node1.example.com,node1"

  Scenario: Reject a CSR
    When I reject CSR for "node2.example.com"
    Then the response status should be 200
    And the CSR "node2.example.com" should be rejected

  Scenario: Revoke a certificate
    When I revoke certificate for "node3.example.com"
    Then the response status should be 200
    And the certificate "node3.example.com" should be revoked

  Scenario: Renew CA certificate
    When I renew CA certificate for 3650 days
    Then the response status should be 200
    And the CA certificate should be renewed for 3650 days
