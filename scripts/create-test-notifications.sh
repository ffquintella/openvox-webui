#!/bin/bash
# Script to create test notifications for development/testing

set -e

# Configuration
API_BASE="${API_BASE:-http://localhost:3000/api/v1}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-admin}"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}Creating test notifications...${NC}"
echo ""

# Authenticate
echo -e "${YELLOW}Authenticating...${NC}"
AUTH_RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$ADMIN_USER\",\"password\":\"$ADMIN_PASS\"}" \
    "$API_BASE/auth/login")

TOKEN=$(echo "$AUTH_RESPONSE" | jq -r '.token // empty')
USER_ID=$(echo "$AUTH_RESPONSE" | jq -r '.user.id // empty')

if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
    echo -e "${RED}Failed to authenticate${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Authenticated${NC}"
echo ""

# Create test notifications
echo -e "${YELLOW}Creating test notifications...${NC}"
echo ""

# Success notification
echo -e "${BLUE}1. Creating SUCCESS notification...${NC}"
curl -s -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{
        "user_id": "'"$USER_ID"'",
        "title": "Deployment Successful",
        "message": "Code deploy to production environment completed successfully. All services are running.",
        "type": "success",
        "category": "deployment",
        "link": "/code"
    }' \
    "$API_BASE/notifications" | jq -r '.notification.id' | xargs -I {} echo -e "${GREEN}✓ Created notification: {}${NC}"

# Info notification
echo -e "${BLUE}2. Creating INFO notification...${NC}"
curl -s -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{
        "user_id": "'"$USER_ID"'",
        "title": "New Nodes Detected",
        "message": "5 new nodes have been detected in PuppetDB. Click to review and classify them.",
        "type": "info",
        "category": "discovery",
        "link": "/nodes"
    }' \
    "$API_BASE/notifications" | jq -r '.notification.id' | xargs -I {} echo -e "${GREEN}✓ Created notification: {}${NC}"

# Warning notification
echo -e "${BLUE}3. Creating WARNING notification...${NC}"
curl -s -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{
        "user_id": "'"$USER_ID"'",
        "title": "Certificate Expiring Soon",
        "message": "SSL certificate for web1.example.com will expire in 15 days. Please renew.",
        "type": "warning",
        "category": "certificates",
        "link": "/ca"
    }' \
    "$API_BASE/notifications" | jq -r '.notification.id' | xargs -I {} echo -e "${GREEN}✓ Created notification: {}${NC}"

# Error notification
echo -e "${BLUE}4. Creating ERROR notification...${NC}"
curl -s -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{
        "user_id": "'"$USER_ID"'",
        "title": "Puppet Run Failed",
        "message": "Puppet agent run failed on db1.example.com with 3 errors. Check the logs for details.",
        "type": "error",
        "category": "puppet",
        "link": "/nodes/db1.example.com"
    }' \
    "$API_BASE/notifications" | jq -r '.notification.id' | xargs -I {} echo -e "${GREEN}✓ Created notification: {}${NC}"

# Multiple notifications for different scenarios
echo -e "${BLUE}5. Creating REPORT notification...${NC}"
curl -s -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{
        "user_id": "'"$USER_ID"'",
        "title": "Weekly Report Available",
        "message": "Your weekly infrastructure report is ready for review. Click to view statistics and trends.",
        "type": "info",
        "category": "reports",
        "link": "/analytics"
    }' \
    "$API_BASE/notifications" | jq -r '.notification.id' | xargs -I {} echo -e "${GREEN}✓ Created notification: {}${NC}"

echo -e "${BLUE}6. Creating ALERT notification...${NC}"
curl -s -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{
        "user_id": "'"$USER_ID"'",
        "title": "High Resource Usage Detected",
        "message": "Node web-prod-01 is using 95% CPU. Alert threshold exceeded.",
        "type": "error",
        "category": "monitoring",
        "link": "/alerting"
    }' \
    "$API_BASE/notifications" | jq -r '.notification.id' | xargs -I {} echo -e "${GREEN}✓ Created notification: {}${NC}"

echo ""
echo -e "${GREEN}✓ All test notifications created${NC}"
echo ""

# Show stats
echo -e "${YELLOW}Current notification statistics:${NC}"
curl -s -H "Authorization: Bearer $TOKEN" "$API_BASE/notifications/stats" | jq .

echo ""
echo -e "${BLUE}To view notifications in the UI:${NC}"
echo "  1. Open your browser to http://localhost:3000"
echo "  2. Login with admin/admin"
echo "  3. Click the notification bell icon in the top-right corner"
echo ""
echo -e "${BLUE}To test SSE stream:${NC}"
echo "  curl -N -H 'Authorization: Bearer $TOKEN' $API_BASE/notifications/stream"
