#!/bin/bash
# Puppet Environment Diagnostic Script
# Diagnoses environment mismatches between Puppet agent and server

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PUPPET_SERVER="${PUPPET_SERVER:-localhost}"
PUPPET_PORT="${PUPPET_PORT:-8140}"
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

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Puppet Environment Diagnostic Tool${NC}"
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

# 1. Check Puppet agent configuration
print_section "Puppet Agent Configuration"

if [ -f /etc/puppetlabs/puppet/puppet.conf ]; then
    print_success "Found puppet.conf"

    echo ""
    echo "Environment setting in puppet.conf:"
    grep -E "^\s*environment\s*=" /etc/puppetlabs/puppet/puppet.conf || echo "  (not set in config file)"

    echo ""
    echo "Server setting:"
    grep -E "^\s*server\s*=" /etc/puppetlabs/puppet/puppet.conf || echo "  (not set, using default)"

    echo ""
    echo "Full [main] section:"
    sed -n '/\[main\]/,/\[/p' /etc/puppetlabs/puppet/puppet.conf | grep -v '^\[' | grep -v '^$' || echo "  (empty)"

    echo ""
    echo "Full [agent] section:"
    sed -n '/\[agent\]/,/\[/p' /etc/puppetlabs/puppet/puppet.conf | grep -v '^\[' | grep -v '^$' || echo "  (empty)"
else
    print_error "puppet.conf not found at /etc/puppetlabs/puppet/puppet.conf"
fi

# 2. Check ENC (if OpenVox WebUI is being used)
print_section "External Node Classifier (ENC) Configuration"

if [ -f /etc/puppetlabs/puppet/puppet.conf ]; then
    ENC_SCRIPT=$(grep -E "^\s*node_terminus\s*=" /etc/puppetlabs/puppet/puppet.conf | awk '{print $3}')
    EXTERNAL_NODES=$(grep -E "^\s*external_nodes\s*=" /etc/puppetlabs/puppet/puppet.conf | awk '{print $3}')

    if [ -n "$ENC_SCRIPT" ] || [ -n "$EXTERNAL_NODES" ]; then
        print_info "ENC is configured"
        echo "  node_terminus: ${ENC_SCRIPT:-not set}"
        echo "  external_nodes: ${EXTERNAL_NODES:-not set}"

        if [ -f "$EXTERNAL_NODES" ]; then
            print_success "ENC script exists: $EXTERNAL_NODES"

            # Test ENC script
            echo ""
            echo "Testing ENC script with certname: $CERTNAME"
            if $EXTERNAL_NODES "$CERTNAME" 2>&1; then
                print_success "ENC script executed successfully"
            else
                print_error "ENC script failed to execute"
            fi
        else
            print_error "ENC script not found: $EXTERNAL_NODES"
        fi
    else
        print_info "ENC is not configured (using node definitions)"
    fi
fi

# 3. Query OpenVox WebUI API for node classification
print_section "OpenVox WebUI Classification"

print_info "Querying: ${WEBUI_URL}/api/v1/nodes/${CERTNAME}/classification"

CLASSIFICATION=$(curl -k -s "${WEBUI_URL}/api/v1/nodes/${CERTNAME}/classification" 2>/dev/null || echo "")

if [ -n "$CLASSIFICATION" ]; then
    print_success "Retrieved classification from OpenVox WebUI"
    echo ""
    echo "$CLASSIFICATION" | python3 -m json.tool 2>/dev/null || echo "$CLASSIFICATION"

    # Extract environment from classification
    WEBUI_ENV=$(echo "$CLASSIFICATION" | grep -o '"environment":\s*"[^"]*"' | cut -d'"' -f4)
    if [ -n "$WEBUI_ENV" ]; then
        echo ""
        print_info "Environment from WebUI: ${WEBUI_ENV}"
    fi
else
    print_error "Could not retrieve classification from OpenVox WebUI"
    print_info "Make sure OpenVox WebUI is running and accessible"
fi

# 4. Check available environments on Puppet Server
print_section "Available Environments on Puppet Server"

ENVIRONMENTS_DIR="/etc/puppetlabs/code/environments"
if [ -d "$ENVIRONMENTS_DIR" ]; then
    print_success "Found environments directory: $ENVIRONMENTS_DIR"
    echo ""
    echo "Available environments:"
    for env in "$ENVIRONMENTS_DIR"/*; do
        if [ -d "$env" ]; then
            env_name=$(basename "$env")
            if [ -f "$env/environment.conf" ]; then
                echo "  ✓ $env_name (has environment.conf)"
            else
                echo "  • $env_name (no environment.conf)"
            fi
        fi
    done
else
    print_error "Environments directory not found: $ENVIRONMENTS_DIR"
fi

# 5. Check Puppet Server logs for environment issues
print_section "Recent Puppet Server Logs (Environment Related)"

PUPPET_LOG="/var/log/puppetlabs/puppetserver/puppetserver.log"
if [ -f "$PUPPET_LOG" ]; then
    print_success "Found Puppet Server log: $PUPPET_LOG"
    echo ""
    echo "Recent environment-related messages:"
    tail -100 "$PUPPET_LOG" | grep -i environment | tail -10 || echo "  (no recent environment messages)"
else
    print_info "Puppet Server log not found at: $PUPPET_LOG"
    print_info "You may need to check logs on the Puppet Server"
fi

# 6. Test catalog compilation with different environments
print_section "Catalog Compilation Test"

TEST_ENVS=("production" "pserver")

for env in "${TEST_ENVS[@]}"; do
    echo ""
    print_info "Testing environment: $env"

    # Check if environment directory exists
    if [ -d "${ENVIRONMENTS_DIR}/${env}" ]; then
        print_success "Environment directory exists"

        # Try to compile a catalog (dry-run)
        echo "  Attempting catalog compilation..."
        CATALOG_RESULT=$(puppet agent -t --environment="$env" --noop --test 2>&1 | head -20)

        if echo "$CATALOG_RESULT" | grep -q "Catalog compiled"; then
            print_success "Catalog compiled successfully for $env"
        else
            print_error "Catalog compilation failed or had issues"
            echo "$CATALOG_RESULT" | grep -i "error\|warning\|not found" | sed 's/^/    /'
        fi
    else
        print_error "Environment directory does not exist: ${ENVIRONMENTS_DIR}/${env}"
    fi
done

# 7. Summary and Recommendations
print_section "Diagnosis Summary"

echo ""
echo "Common Issues and Solutions:"
echo ""
echo "1. ${YELLOW}Environment mismatch between agent and ENC:${NC}"
echo "   - The ENC (OpenVox WebUI) returns an environment that doesn't exist"
echo "   - Solution: Create the environment directory on Puppet Server"
echo "   - Command: mkdir -p /etc/puppetlabs/code/environments/pserver"
echo ""
echo "2. ${YELLOW}Agent environment setting conflicts with ENC:${NC}"
echo "   - puppet.conf has 'environment=X' but ENC returns 'environment=Y'"
echo "   - Solution: Remove 'environment' from puppet.conf [agent] section"
echo "   - Let the ENC control the environment assignment"
echo ""
echo "3. ${YELLOW}Pinned nodes not respecting environment filter:${NC}"
echo "   - Node is pinned to a group with a different environment"
echo "   - Solution: Check node classification in OpenVox WebUI"
echo "   - URL: ${WEBUI_URL}/nodes/${CERTNAME}"
echo ""
echo "4. ${YELLOW}ENC script not returning proper YAML:${NC}"
echo "   - The ENC must return valid YAML with 'environment' key"
echo "   - Solution: Test ENC script: /path/to/enc.sh ${CERTNAME}"
echo ""

print_section "Next Steps"

echo ""
echo "1. Review the classification from OpenVox WebUI above"
echo "2. Verify the environment exists in: $ENVIRONMENTS_DIR"
echo "3. Check if the node is correctly classified in OpenVox WebUI"
echo "4. If needed, create the environment directory:"
echo "   ${BLUE}mkdir -p /etc/puppetlabs/code/environments/pserver${NC}"
echo "5. Run puppet agent again:"
echo "   ${BLUE}puppet agent -t --environment=pserver${NC}"
echo ""

print_section "Useful Commands"

echo ""
echo "# Test ENC script directly:"
echo "sudo /opt/openvox/enc.sh ${CERTNAME}"
echo ""
echo "# Query OpenVox API:"
echo "curl -k ${WEBUI_URL}/api/v1/nodes/${CERTNAME}/classification"
echo ""
echo "# View node in OpenVox WebUI:"
echo "${WEBUI_URL}/nodes/${CERTNAME}"
echo ""
echo "# Check Puppet Server logs:"
echo "sudo tail -f /var/log/puppetlabs/puppetserver/puppetserver.log"
echo ""

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}Diagnostic complete!${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
