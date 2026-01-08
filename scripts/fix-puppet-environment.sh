#!/bin/bash
# Puppet Environment Auto-Fix Script
# Automatically fixes common environment mismatch issues

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PUPPET_SERVER="${PUPPET_SERVER:-localhost}"
ENVIRONMENTS_DIR="/etc/puppetlabs/code/environments"
PUPPET_CONF="/etc/puppetlabs/puppet/puppet.conf"
CERTNAME="${1:-$(hostname -f)}"

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
echo -e "${BLUE}   Puppet Environment Auto-Fix Tool${NC}"
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

# 1. Get environment from OpenVox WebUI
print_section "Fetching Node Classification"

print_info "Querying: ${WEBUI_URL}/api/v1/nodes/${CERTNAME}/classification"

CLASSIFICATION=$(curl -k -s "${WEBUI_URL}/api/v1/nodes/${CERTNAME}/classification" 2>/dev/null || echo "")

if [ -z "$CLASSIFICATION" ]; then
    print_error "Could not retrieve classification from OpenVox WebUI"
    echo ""
    echo "Possible causes:"
    echo "  1. OpenVox WebUI is not running"
    echo "  2. Node is not in PuppetDB yet"
    echo "  3. Network connectivity issues"
    echo ""
    echo "Please ensure OpenVox WebUI is running and try again."
    exit 1
fi

print_success "Retrieved classification"

# Extract environment
WEBUI_ENV=$(echo "$CLASSIFICATION" | grep -o '"environment":\s*"[^"]*"' | cut -d'"' -f4)

if [ -z "$WEBUI_ENV" ]; then
    print_error "No environment found in classification"
    echo "$CLASSIFICATION"
    exit 1
fi

print_success "Node environment: ${WEBUI_ENV}"

# 2. Check if environment exists
print_section "Checking Environment Directory"

ENV_PATH="${ENVIRONMENTS_DIR}/${WEBUI_ENV}"

if [ -d "$ENV_PATH" ]; then
    print_success "Environment directory exists: $ENV_PATH"
else
    print_error "Environment directory does not exist: $ENV_PATH"
    echo ""

    if confirm "Create environment directory for '${WEBUI_ENV}'?"; then
        echo ""
        print_info "Creating environment directory..."

        # Create environment directory
        mkdir -p "$ENV_PATH"
        mkdir -p "$ENV_PATH/manifests"
        mkdir -p "$ENV_PATH/modules"
        mkdir -p "$ENV_PATH/data"

        # Create a basic site.pp
        cat > "$ENV_PATH/manifests/site.pp" <<EOF
# Default site.pp for ${WEBUI_ENV} environment
# Managed by OpenVox WebUI

node default {
  # Classification is handled by ENC (OpenVox WebUI)
  # Add any environment-specific defaults here
}
EOF

        # Create environment.conf
        cat > "$ENV_PATH/environment.conf" <<EOF
# Environment configuration for ${WEBUI_ENV}
# Managed by OpenVox WebUI

modulepath = modules:\$basemodulepath
manifest = manifests/site.pp
EOF

        # Set proper permissions
        chown -R puppet:puppet "$ENV_PATH" 2>/dev/null || true
        chmod -R 755 "$ENV_PATH"

        print_success "Environment directory created: $ENV_PATH"
        print_success "Created manifests/site.pp"
        print_success "Created environment.conf"
    else
        print_error "Cannot proceed without environment directory"
        exit 1
    fi
fi

# 3. Check puppet.conf for conflicting environment setting
print_section "Checking puppet.conf Configuration"

if [ -f "$PUPPET_CONF" ]; then
    # Check for environment setting in [agent] section
    AGENT_ENV=$(sed -n '/\[agent\]/,/\[/p' "$PUPPET_CONF" | grep -E "^\s*environment\s*=" | awk -F= '{print $2}' | tr -d ' ')

    if [ -n "$AGENT_ENV" ]; then
        print_error "Found environment setting in [agent] section: $AGENT_ENV"
        echo ""
        echo "This can conflict with ENC-based classification."
        echo ""

        if confirm "Remove environment setting from puppet.conf [agent] section?"; then
            echo ""
            print_info "Backing up puppet.conf..."
            cp "$PUPPET_CONF" "${PUPPET_CONF}.backup.$(date +%Y%m%d_%H%M%S)"

            print_info "Removing environment setting from [agent] section..."

            # Remove environment line from [agent] section
            sed -i '/\[agent\]/,/\[/ { /^\s*environment\s*=/d; }' "$PUPPET_CONF"

            print_success "Environment setting removed from puppet.conf"
            print_info "Backup saved: ${PUPPET_CONF}.backup.*"
        else
            print_info "Keeping environment setting in puppet.conf"
            print_info "Note: This may override ENC classification"
        fi
    else
        print_success "No conflicting environment setting in [agent] section"
    fi
else
    print_error "puppet.conf not found: $PUPPET_CONF"
fi

# 4. Test catalog compilation
print_section "Testing Catalog Compilation"

print_info "Attempting to compile catalog for environment: ${WEBUI_ENV}"
echo ""

# Run puppet agent in noop mode to test
set +e
CATALOG_OUTPUT=$(puppet agent -t --environment="${WEBUI_ENV}" --noop 2>&1)
CATALOG_EXIT=$?
set -e

if [ $CATALOG_EXIT -eq 0 ] || echo "$CATALOG_OUTPUT" | grep -q "Catalog compiled"; then
    print_success "Catalog compiled successfully!"

    # Check if environment was changed by server
    if echo "$CATALOG_OUTPUT" | grep -q "doesn't match server specified environment"; then
        print_error "Server is forcing a different environment"
        echo ""
        echo "This usually means:"
        echo "  1. The ENC is returning a different environment"
        echo "  2. There's a mismatch between what the ENC returns and what exists"
        echo ""
        echo "Catalog output:"
        echo "$CATALOG_OUTPUT" | grep -i environment
    else
        print_success "Environment '${WEBUI_ENV}' accepted by server"
    fi
else
    print_error "Catalog compilation had issues"
    echo ""
    echo "Output:"
    echo "$CATALOG_OUTPUT" | head -20
fi

# 5. Summary
print_section "Summary and Next Steps"

echo ""
echo "Changes made:"
echo ""

if [ -d "$ENV_PATH" ] && [ ! -d "$ENV_PATH.old" ]; then
    echo "  ✓ Created environment directory: $ENV_PATH"
fi

if [ -f "${PUPPET_CONF}.backup."* ] 2>/dev/null; then
    echo "  ✓ Updated puppet.conf (backup created)"
fi

echo ""
echo "Recommended actions:"
echo ""
echo "1. Verify the environment in OpenVox WebUI:"
echo "   ${BLUE}http://${PUPPET_SERVER}:8080/nodes/${CERTNAME}${NC}"
echo ""
echo "2. Check the node's group assignments and rules"
echo ""
echo "3. Populate the environment with your Puppet modules:"
echo "   ${BLUE}cd $ENV_PATH/modules${NC}"
echo "   ${BLUE}# Copy or link your modules here${NC}"
echo ""
echo "4. Run Puppet agent without --environment flag:"
echo "   ${BLUE}puppet agent -t${NC}"
echo "   (Let the ENC determine the environment)"
echo ""
echo "5. If issues persist, check the ENC script output:"
echo "   ${BLUE}/opt/openvox/enc.sh ${CERTNAME}${NC}"
echo ""

print_section "Important Notes"

echo ""
echo "• The environment should now be properly configured"
echo "• Make sure to deploy your Puppet code to: $ENV_PATH"
echo "• The ENC (OpenVox WebUI) controls environment assignment"
echo "• Pinned nodes may ignore environment filters - check group settings"
echo ""

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}Auto-fix complete!${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
