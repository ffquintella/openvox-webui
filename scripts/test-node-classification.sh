#!/bin/bash
# Script to test node classification via the API

set -e

NODE_CERTNAME="${1:-segdc1vpr0018.fgv.br}"
USERNAME="${2:-admin}"
PASSWORD="${3:-ffq141421356239}"
API_URL="${4:-http://openvox.esi.fgv.br}"

echo "=== Testing Node Classification ==="
echo "Node: $NODE_CERTNAME"
echo "API: $API_URL"
echo ""

# Login and get token
echo "1. Logging in..."
TOKEN=$(curl -s -X POST "${API_URL}/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"${USERNAME}\",\"password\":\"${PASSWORD}\"}" | jq -r '.token')

if [ "$TOKEN" == "null" ] || [ -z "$TOKEN" ]; then
  echo "  ✗ Login failed"
  exit 1
fi
echo "  ✓ Login successful"
echo ""

# Get node classification
echo "2. Getting node classification..."
CLASSIFICATION=$(curl -s -H "Authorization: Bearer $TOKEN" \
  "${API_URL}/api/v1/nodes/${NODE_CERTNAME}/classification")

echo "$CLASSIFICATION" | jq '.'
echo ""

# Extract environment
ENVIRONMENT=$(echo "$CLASSIFICATION" | jq -r '.environment')
echo "=== Result ==="
echo "Environment returned: $ENVIRONMENT"
echo ""

# Get matched groups
echo "Matched groups:"
echo "$CLASSIFICATION" | jq -r '.groups[] | "  - \(.name) (match_type: \(.match_type))"'
echo ""

# Get node facts
echo "3. Getting node facts (catalog_environment)..."
NODE_DATA=$(curl -s -H "Authorization: Bearer $TOKEN" \
  "${API_URL}/api/v1/nodes/${NODE_CERTNAME}")

CATALOG_ENV=$(echo "$NODE_DATA" | jq -r '.catalog_environment // "null"')
echo "  Node's catalog_environment fact: $CATALOG_ENV"
echo ""

# Get all groups
echo "4. Getting all groups to check configurations..."
GROUPS=$(curl -s -H "Authorization: Bearer $TOKEN" \
  "${API_URL}/api/v1/groups")

echo "Groups with their environments:"
echo "$GROUPS" | jq -r '.[] | "  - \(.name): environment=\(.environment // "null"), pinned_nodes=\(.pinned_nodes | length)"'
echo ""

# Check if node is pinned to any group
echo "5. Checking which groups have this node pinned..."
PINNED_GROUPS=$(echo "$GROUPS" | jq -r --arg node "$NODE_CERTNAME" \
  '.[] | select(.pinned_nodes | map(. == $node) | any) | .name')

if [ -z "$PINNED_GROUPS" ]; then
  echo "  Node is not pinned to any group"
else
  echo "$PINNED_GROUPS" | while read -r group; do
    ENV=$(echo "$GROUPS" | jq -r --arg name "$group" '.[] | select(.name == $name) | .environment // "null"')
    echo "  - $group (environment: $ENV)"
  done
fi
echo ""

echo "=== Diagnosis ==="
if [ "$ENVIRONMENT" == "production" ]; then
  echo "❌ Node is getting 'production' environment from ENC"
  echo ""
  echo "Possible causes:"
  echo "1. Check if 'Puppe Servers' group environment matches node's catalog_environment ($CATALOG_ENV)"
  echo "2. Check if another group is setting environment to 'production'"
  echo "3. Check if there's an 'All Nodes' or similar group with environment='production'"
  echo ""
  echo "To fix:"
  echo "- If 'Puppe Servers' has environment='pserver', make sure the node's catalog_environment"
  echo "  fact is also 'pserver', OR"
  echo "- Set 'Puppe Servers' group environment to '*' or 'Any' to match all environments"
elif [ "$ENVIRONMENT" == "pserver" ]; then
  echo "✓ Node is correctly getting 'pserver' environment from ENC"
else
  echo "⚠ Node environment is: $ENVIRONMENT"
fi
