#!/bin/bash
# Setup Puppet Server to use OpenVox WebUI as External Node Classifier (ENC)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PUPPET_CONF="/etc/puppetlabs/puppet/puppet.conf"
ENC_SCRIPT="/opt/openvox/enc.sh"

# Auto-detect OpenVox WebUI URL
if [ -z "$WEBUI_URL" ]; then
    if netstat -anp 2>/dev/null | grep -q "openvox-webu.*:443"; then
        WEBUI_URL="https://localhost"
    elif netstat -anp 2>/dev/null | grep -q "openvox-webu.*:8080"; then
        WEBUI_URL="http://localhost:8080"
    else
        WEBUI_URL="http://localhost:8080"
    fi
fi

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}This script must be run as root${NC}"
    exit 1
fi

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Setup Puppet ENC for OpenVox WebUI${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo ""

# Function to print section headers
print_section() {
    echo -e "\n${YELLOW}▶ $1${NC}"
    echo "───────────────────────────────────────────────────────────"
}

# Function to print success
print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

# Function to print error
print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Function to print info
print_info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

# Function to ask for confirmation
confirm() {
    read -p "$(echo -e ${YELLOW}$1 [y/N]: ${NC})" -n 1 -r
    echo
    [[ $REPLY =~ ^[Yy]$ ]]
}

# 1. Check if OpenVox WebUI is running
print_section "Checking OpenVox WebUI"

if curl -k -s "${WEBUI_URL}/api/v1/health" > /dev/null 2>&1; then
    print_success "OpenVox WebUI is running at ${WEBUI_URL}"
else
    print_error "OpenVox WebUI is not accessible at ${WEBUI_URL}"
    echo ""
    echo "Please ensure OpenVox WebUI is running:"
    echo "  ${BLUE}systemctl status openvox-webui${NC}"
    echo ""
    echo "If using a different URL, set WEBUI_URL environment variable:"
    echo "  ${BLUE}WEBUI_URL=https://localhost ./setup-puppet-enc.sh${NC}"
    echo ""
    exit 1
fi

# 2. Create ENC script
print_section "Creating ENC Script"

ENC_DIR=$(dirname "$ENC_SCRIPT")
if [ ! -d "$ENC_DIR" ]; then
    print_info "Creating directory: $ENC_DIR"
    mkdir -p "$ENC_DIR"
fi

print_info "Creating ENC script at: $ENC_SCRIPT"

cat > "$ENC_SCRIPT" <<EOF
#!/bin/bash
# External Node Classifier for OpenVox WebUI
# This script is called by Puppet Server to get node classification

set -e

# Configuration
WEBUI_URL="${WEBUI_URL}"
CERTNAME="\$1"

# Validate input
if [ -z "\$CERTNAME" ]; then
    echo "Error: No certname provided" >&2
    exit 1
fi

# Query OpenVox WebUI API for node classification
# Use -k to allow self-signed certificates
CLASSIFICATION=\$(curl -k -s "\${WEBUI_URL}/api/v1/nodes/\${CERTNAME}/classification" 2>&1)

# Check if we got valid JSON
if echo "\$CLASSIFICATION" | grep -q "^{"; then
    # Convert JSON to YAML format expected by Puppet
    echo "\$CLASSIFICATION" | python3 -c '
import sys
import json
import yaml

try:
    data = json.load(sys.stdin)
    # Puppet expects YAML format
    print(yaml.dump(data, default_flow_style=False))
except Exception as e:
    print("Error parsing classification: " + str(e), file=sys.stderr)
    sys.exit(1)
'
else
    # Node not found or API error - return minimal classification
    echo "---"
    echo "environment: production"
    echo "classes: {}"
fi
EOF

chmod +x "$ENC_SCRIPT"
print_success "ENC script created and made executable"

# 3. Install PyYAML if not present
print_section "Checking Dependencies"

if ! python3 -c "import yaml" 2>/dev/null; then
    print_info "PyYAML not found, installing..."

    if command -v dnf &> /dev/null; then
        dnf install -y python3-pyyaml
    elif command -v yum &> /dev/null; then
        yum install -y python3-pyyaml
    elif command -v apt-get &> /dev/null; then
        apt-get update && apt-get install -y python3-yaml
    else
        print_error "Could not install PyYAML automatically"
        echo "Please install manually: pip3 install pyyaml"
        exit 1
    fi

    print_success "PyYAML installed"
else
    print_success "PyYAML is already installed"
fi

# 4. Test ENC script
print_section "Testing ENC Script"

TEST_NODE=$(hostname -f)
print_info "Testing with node: $TEST_NODE"

if ENC_OUTPUT=$("$ENC_SCRIPT" "$TEST_NODE" 2>&1); then
    print_success "ENC script executed successfully"
    echo ""
    echo "Output:"
    echo "$ENC_OUTPUT"
else
    print_error "ENC script failed"
    echo "$ENC_OUTPUT"
    exit 1
fi

# 5. Configure Puppet Server
print_section "Configuring Puppet Server"

if [ ! -f "$PUPPET_CONF" ]; then
    print_error "Puppet config not found: $PUPPET_CONF"
    exit 1
fi

print_info "Backing up puppet.conf..."
cp "$PUPPET_CONF" "${PUPPET_CONF}.backup.$(date +%Y%m%d_%H%M%S)"
print_success "Backup created"

# Check if [master] section exists
if ! grep -q "^\[master\]" "$PUPPET_CONF"; then
    print_info "Adding [master] section to puppet.conf"
    echo "" >> "$PUPPET_CONF"
    echo "[master]" >> "$PUPPET_CONF"
fi

# Add or update node_terminus
if grep -q "^\s*node_terminus\s*=" "$PUPPET_CONF"; then
    print_info "Updating existing node_terminus setting"
    sed -i.bak '/\[master\]/,/\[/ s|^\s*node_terminus\s*=.*|node_terminus = exec|' "$PUPPET_CONF"
else
    print_info "Adding node_terminus setting"
    sed -i.bak '/\[master\]/a node_terminus = exec' "$PUPPET_CONF"
fi

# Add or update external_nodes
if grep -q "^\s*external_nodes\s*=" "$PUPPET_CONF"; then
    print_info "Updating existing external_nodes setting"
    sed -i.bak "/\[master\]/,/\[/ s|^\s*external_nodes\s*=.*|external_nodes = ${ENC_SCRIPT}|" "$PUPPET_CONF"
else
    print_info "Adding external_nodes setting"
    sed -i.bak "/\[master\]/a external_nodes = ${ENC_SCRIPT}" "$PUPPET_CONF"
fi

print_success "Puppet Server configured to use ENC"

# Show the relevant config
echo ""
echo "Updated [master] section:"
sed -n '/\[master\]/,/\[/p' "$PUPPET_CONF" | grep -v '^\['

# 6. Restart Puppet Server
print_section "Restarting Puppet Server"

if confirm "Restart Puppet Server to apply changes?"; then
    echo ""
    if systemctl restart puppetserver; then
        print_success "Puppet Server restarted successfully"

        # Wait a few seconds for it to come up
        sleep 5

        if systemctl is-active puppetserver > /dev/null 2>&1; then
            print_success "Puppet Server is running"
        else
            print_error "Puppet Server failed to start"
            echo "Check logs: journalctl -u puppetserver -n 50"
            exit 1
        fi
    else
        print_error "Failed to restart Puppet Server"
        exit 1
    fi
else
    print_info "Skipping restart. Remember to restart manually:"
    echo "  ${BLUE}systemctl restart puppetserver${NC}"
fi

# 7. Remove environment setting from agent config
print_section "Cleaning Up Agent Configuration"

if grep -q "^\s*environment\s*=" "$PUPPET_CONF" | grep -A5 "\[agent\]"; then
    echo ""
    print_info "Found 'environment' setting in [agent] section"
    echo "With ENC enabled, this should be removed to let the ENC control environment assignment."
    echo ""

    if confirm "Remove environment setting from [agent] section?"; then
        sed -i.bak2 '/\[agent\]/,/\[/ { /^\s*environment\s*=/d; }' "$PUPPET_CONF"
        print_success "Environment setting removed from [agent] section"
    else
        print_info "Keeping environment setting (may override ENC)"
    fi
fi

# 8. Test full flow
print_section "Testing Full Flow"

TEST_NODE=$(hostname -f)
print_info "Testing classification for: $TEST_NODE"

echo ""
echo "1. Testing ENC script directly:"
"$ENC_SCRIPT" "$TEST_NODE" | head -10

echo ""
echo "2. Testing via Puppet Server API:"
curl -s "http://localhost:8080/api/v1/nodes/${TEST_NODE}/classification" | python3 -m json.tool 2>/dev/null || echo "(API test)"

echo ""
echo "3. Testing puppet agent (noop):"
puppet agent -t --noop 2>&1 | head -10

# 9. Summary
print_section "Setup Complete!"

echo ""
echo "ENC Configuration Summary:"
echo ""
echo "  ENC Script:      ${BLUE}${ENC_SCRIPT}${NC}"
echo "  Puppet Config:   ${BLUE}${PUPPET_CONF}${NC}"
echo "  WebUI URL:       ${BLUE}${WEBUI_URL}${NC}"
echo ""
echo "Next Steps:"
echo ""
echo "1. Verify node appears in OpenVox WebUI:"
echo "   ${BLUE}${WEBUI_URL}/nodes${NC}"
echo ""
echo "2. Configure node classification rules in OpenVox WebUI:"
echo "   - Create groups with environment filters"
echo "   - Add classification rules"
echo "   - Optionally pin specific nodes"
echo ""
echo "3. Run puppet agent WITHOUT --environment flag:"
echo "   ${BLUE}puppet agent -t${NC}"
echo "   (Let the ENC determine the environment)"
echo ""
echo "4. Check Puppet Server logs if issues occur:"
echo "   ${BLUE}tail -f /var/log/puppetlabs/puppetserver/puppetserver.log${NC}"
echo ""
echo "5. Test ENC directly anytime:"
echo "   ${BLUE}${ENC_SCRIPT} <certname>${NC}"
echo ""

print_section "Important Notes"

echo ""
echo "• The ENC now controls node classification and environment assignment"
echo "• Remove 'environment=' from agent puppet.conf to avoid conflicts"
echo "• Pinned nodes in WebUI will always use their group's environment"
echo "• Make sure all required environments exist in:"
echo "  ${BLUE}/etc/puppetlabs/code/environments/${NC}"
echo ""

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}ENC Setup Complete!${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
