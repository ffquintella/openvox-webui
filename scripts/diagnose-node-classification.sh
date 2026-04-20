#!/bin/bash
# Script to diagnose node classification and environment assignment

set -e

NODE_CERTNAME="${1:-segdc1vpr0018.fgv.br}"

echo "=== Node Classification Diagnostics ==="
echo ""
echo "Node: $NODE_CERTNAME"
echo ""

# Check if we can reach the API
echo "1. Testing API connectivity..."
if ! curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/api/v1/health > /dev/null; then
    echo "  ✗ Cannot reach OpenVox WebUI API. Is the service running?"
    exit 1
fi
echo "  ✓ API is reachable"
echo ""

# Get the node's classification (requires authentication)
echo "2. Fetching node classification..."
echo "  (You'll need to run this manually with authentication)"
echo ""
echo "  Run this command with a valid JWT token:"
echo "  curl -H 'Authorization: Bearer YOUR_TOKEN' \\"
echo "    'http://localhost:8080/api/v1/nodes/$NODE_CERTNAME/classification'"
echo ""

# Get PuppetDB facts
echo "3. Checking node facts in PuppetDB..."
echo "  Run this command to see the node's catalog_environment:"
echo "  curl -X GET 'http://puppetdb:8080/pdb/query/v4/nodes/$NODE_CERTNAME/facts/catalog_environment' \\"
echo "    --data-urlencode 'query=[\"=\", \"certname\", \"$NODE_CERTNAME\"]'"
echo ""

# Check groups that might match
echo "4. To find which group is setting the environment to 'production':"
echo ""
echo "  a) Log into the OpenVox WebUI (http://localhost:8080)"
echo "  b) Go to the 'Groups' page"
echo "  c) Look for groups that have:"
echo "     - Environment set to 'production'"
echo "     - Rules that match this node's facts"
echo "     - Or this node pinned to the group"
echo ""
echo "  d) To fix the issue, either:"
echo "     - Change the group's environment from 'production' to 'pserver'"
echo "     - Create a new group specifically for Puppet server nodes with environment='pserver'"
echo "     - Remove this node from the production group if incorrectly assigned"
echo ""

echo "=== Quick Fix Instructions ==="
echo ""
echo "If you want this node to use the 'pserver' environment:"
echo ""
echo "1. Find the node group that matches '$NODE_CERTNAME'"
echo "2. Edit that group's environment setting to 'pserver'"
echo "3. Or create a new group with higher priority (parent group) that:"
echo "   - Has environment = 'pserver'"
echo "   - Has a rule to match this node (e.g., clientcert = '$NODE_CERTNAME')"
echo "   - Or has this node pinned to it"
echo ""
echo "After making changes in OpenVox WebUI, run: puppet agent -t"
echo ""
