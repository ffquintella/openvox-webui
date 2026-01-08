#!/bin/bash
# Test script for notification system and ENC environment assignment
# This script tests the environment classification issue and creates test notifications

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
API_BASE="${API_BASE:-http://localhost:3000/api/v1}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-admin}"
NODE_CERTNAME="${NODE_CERTNAME:-segdc1vpr0018.fgv.br}"
TARGET_ENVIRONMENT="${TARGET_ENVIRONMENT:-pserver}"

echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}OpenVox WebUI Test Script${NC}"
echo -e "${BLUE}======================================${NC}"
echo ""

# Function to make API calls
api_call() {
    local method=$1
    local endpoint=$2
    local data=$3
    local token=$4

    if [ -n "$data" ]; then
        curl -s -X "$method" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer $token" \
            -d "$data" \
            "$API_BASE$endpoint"
    else
        curl -s -X "$method" \
            -H "Authorization: Bearer $token" \
            "$API_BASE$endpoint"
    fi
}

# Step 1: Authenticate
echo -e "${YELLOW}Step 1: Authenticating...${NC}"
AUTH_RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$ADMIN_USER\",\"password\":\"$ADMIN_PASS\"}" \
    "$API_BASE/auth/login")

TOKEN=$(echo "$AUTH_RESPONSE" | jq -r '.token // empty')
USER_ID=$(echo "$AUTH_RESPONSE" | jq -r '.user.id // empty')

if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
    echo -e "${RED}Failed to authenticate. Response:${NC}"
    echo "$AUTH_RESPONSE" | jq .
    exit 1
fi

echo -e "${GREEN}✓ Authenticated successfully${NC}"
echo -e "  User ID: $USER_ID"
echo ""

# Step 2: Check node classification
echo -e "${YELLOW}Step 2: Checking node classification for $NODE_CERTNAME...${NC}"
NODE_CLASSIFICATION=$(curl -s "$API_BASE/nodes/$NODE_CERTNAME/classification")

echo -e "${BLUE}Current classification:${NC}"
echo "$NODE_CLASSIFICATION" | jq .

CURRENT_ENV=$(echo "$NODE_CLASSIFICATION" | jq -r '.environment // "null"')
MATCHED_GROUPS=$(echo "$NODE_CLASSIFICATION" | jq -r '.groups[].name' | tr '\n' ',' | sed 's/,$//')

echo -e "${BLUE}Environment:${NC} $CURRENT_ENV"
echo -e "${BLUE}Matched Groups:${NC} $MATCHED_GROUPS"
echo ""

# Step 3: Create a test notification about the environment issue
if [ "$CURRENT_ENV" != "$TARGET_ENVIRONMENT" ]; then
    echo -e "${YELLOW}Step 3: Environment mismatch detected! Creating notification...${NC}"

    NOTIFICATION_DATA=$(cat <<EOF
{
    "user_id": "$USER_ID",
    "title": "ENC Environment Mismatch",
    "message": "Node $NODE_CERTNAME is classified with environment '$CURRENT_ENV' but expected '$TARGET_ENVIRONMENT'. Please check group configuration.",
    "type": "warning",
    "category": "classification",
    "link": "/nodes/$NODE_CERTNAME"
}
EOF
)

    NOTIFICATION_RESPONSE=$(api_call POST /notifications "$NOTIFICATION_DATA" "$TOKEN")
    NOTIFICATION_ID=$(echo "$NOTIFICATION_RESPONSE" | jq -r '.notification.id // empty')

    if [ -n "$NOTIFICATION_ID" ]; then
        echo -e "${GREEN}✓ Notification created${NC}"
        echo -e "  ID: $NOTIFICATION_ID"
    else
        echo -e "${RED}Failed to create notification${NC}"
        echo "$NOTIFICATION_RESPONSE" | jq .
    fi
else
    echo -e "${GREEN}✓ Environment matches expected value${NC}"
fi
echo ""

# Step 4: List all groups and check which ones match the node
echo -e "${YELLOW}Step 4: Analyzing group configurations...${NC}"
GROUPS=$(api_call GET /groups "" "$TOKEN")

echo "$GROUPS" | jq -r '.groups[] | "\(.id)|\(.name)|\(.environment // "null")"' | while IFS='|' read -r group_id group_name group_env; do
    echo -e "${BLUE}Group:${NC} $group_name"
    echo -e "  Environment: ${group_env}"

    # Check if node is pinned to this group
    GROUP_NODES=$(api_call GET "/groups/$group_id/nodes" "" "$TOKEN")
    IS_PINNED=$(echo "$GROUP_NODES" | jq -r --arg certname "$NODE_CERTNAME" '.nodes[] | select(.certname == $certname) | .match_type' | grep -q "pinned" && echo "yes" || echo "no")

    if [ "$IS_PINNED" = "yes" ]; then
        echo -e "  ${GREEN}✓ Node is PINNED to this group${NC}"

        # Create notification if environment doesn't match
        if [ "$group_env" != "$TARGET_ENVIRONMENT" ] && [ "$group_env" != "null" ] && [ "$group_env" != "*" ]; then
            NOTIFICATION_DATA=$(cat <<EOF
{
    "user_id": "$USER_ID",
    "title": "Pinned Group Environment Issue",
    "message": "Node $NODE_CERTNAME is pinned to group '$group_name' with environment '$group_env', but should be '$TARGET_ENVIRONMENT'",
    "type": "error",
    "category": "configuration",
    "link": "/groups/$group_id"
}
EOF
)
            api_call POST /notifications "$NOTIFICATION_DATA" "$TOKEN" > /dev/null
            echo -e "  ${YELLOW}! Created notification about environment mismatch${NC}"
        fi
    fi

    # Get group rules
    GROUP_RULES=$(api_call GET "/groups/$group_id/rules" "" "$TOKEN")
    RULE_COUNT=$(echo "$GROUP_RULES" | jq '.rules | length')
    echo -e "  Rules: $RULE_COUNT"
    echo ""
done

# Step 5: Get notification statistics
echo -e "${YELLOW}Step 5: Checking notification statistics...${NC}"
STATS=$(api_call GET /notifications/stats "" "$TOKEN")
echo "$STATS" | jq .
echo ""

# Step 6: List recent notifications
echo -e "${YELLOW}Step 6: Listing recent notifications...${NC}"
NOTIFICATIONS=$(api_call GET "/notifications?limit=10" "" "$TOKEN")
echo "$NOTIFICATIONS" | jq -r '.notifications[] | "\(.type)|\(.title)|\(.created_at)|\(.read)"' | while IFS='|' read -r type title created read; do
    if [ "$read" = "true" ]; then
        READ_MARKER="✓"
    else
        READ_MARKER="○"
    fi

    case "$type" in
        error)
            COLOR=$RED
            ;;
        warning)
            COLOR=$YELLOW
            ;;
        success)
            COLOR=$GREEN
            ;;
        *)
            COLOR=$BLUE
            ;;
    esac

    echo -e "${COLOR}[$READ_MARKER] $type${NC} - $title"
    echo -e "   Created: $created"
done
echo ""

# Step 7: Generate diagnostic report
echo -e "${YELLOW}Step 7: Generating diagnostic report...${NC}"

cat > /tmp/openvox-enc-diagnostic-report.txt <<EOF
OpenVox WebUI - ENC Environment Diagnostic Report
Generated: $(date)
================================================

Node Information:
-----------------
Certname: $NODE_CERTNAME
Current Environment: $CURRENT_ENV
Expected Environment: $TARGET_ENVIRONMENT
Matched Groups: $MATCHED_GROUPS

Issue Description:
------------------
The node is requesting environment '$TARGET_ENVIRONMENT' but the ENC is
returning environment '$CURRENT_ENV'. This causes Puppet to restart the
agent run with the server-specified environment.

Expected Behavior:
------------------
When a node is pinned to a group, the node should match that group
regardless of its current environment (bootstrap problem fix). The group's
environment should then be assigned to the node in the ENC response.

Classification Data:
--------------------
EOF

echo "$NODE_CLASSIFICATION" | jq . >> /tmp/openvox-enc-diagnostic-report.txt

cat >> /tmp/openvox-enc-diagnostic-report.txt <<EOF

Groups Configuration:
---------------------
EOF

echo "$GROUPS" | jq . >> /tmp/openvox-enc-diagnostic-report.txt

cat >> /tmp/openvox-enc-diagnostic-report.txt <<EOF

Notifications Created:
----------------------
EOF

echo "$NOTIFICATIONS" | jq . >> /tmp/openvox-enc-diagnostic-report.txt

echo -e "${GREEN}✓ Report saved to: /tmp/openvox-enc-diagnostic-report.txt${NC}"
echo ""

# Step 8: Provide recommendations
echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}Recommendations:${NC}"
echo -e "${BLUE}======================================${NC}"
echo ""

if [ "$CURRENT_ENV" != "$TARGET_ENVIRONMENT" ]; then
    echo -e "${YELLOW}Issue Detected:${NC} Environment mismatch"
    echo ""
    echo -e "${GREEN}Recommended Actions:${NC}"
    echo "1. Check which group has the node pinned:"
    echo "   curl -H 'Authorization: Bearer $TOKEN' $API_BASE/groups | jq '.groups[] | select(.pinned_nodes[] | contains(\"$NODE_CERTNAME\"))'"
    echo ""
    echo "2. Update the group's environment to '$TARGET_ENVIRONMENT':"
    echo "   curl -X PUT -H 'Content-Type: application/json' -H 'Authorization: Bearer $TOKEN' \\"
    echo "        -d '{\"environment\":\"$TARGET_ENVIRONMENT\"}' \\"
    echo "        $API_BASE/groups/{GROUP_ID}"
    echo ""
    echo "3. Verify the fix works:"
    echo "   curl $API_BASE/nodes/$NODE_CERTNAME/classification | jq '.environment'"
    echo ""
    echo "4. Run puppet agent to test:"
    echo "   puppet agent -t --environment=$TARGET_ENVIRONMENT"
    echo ""
else
    echo -e "${GREEN}✓ No issues detected${NC}"
    echo "The node's environment matches the expected value."
fi

echo -e "${BLUE}======================================${NC}"
echo -e "${GREEN}Test complete!${NC}"
echo -e "${BLUE}======================================${NC}"
