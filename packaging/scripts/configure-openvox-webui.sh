#!/bin/bash
#
# OpenVox WebUI Post-Installation Configuration Script
#
# This script detects Puppet infrastructure and configures OpenVox WebUI
# to integrate with PuppetDB and Puppet CA.
#
# It can be run:
#   - Automatically during RPM/DEB installation (interactive mode)
#   - Manually by the administrator: /usr/share/openvox-webui/configure-openvox-webui.sh
#
# Options:
#   --non-interactive    Skip all prompts and use detected defaults
#   --force              Overwrite existing configuration
#   --help               Show this help message
#

set -e

# Configuration paths
CONFIG_FILE="/etc/openvox-webui/config.yaml"
CONFIG_BACKUP="/etc/openvox-webui/config.yaml.bak"
SSL_DIR="/etc/openvox-webui/ssl"
DATA_DIR="/var/lib/openvox-webui"
LOG_DIR="/var/log/openvox/webui"

# Puppet paths
PUPPET_CONFDIR="/etc/puppetlabs/puppet"
PUPPETDB_CONFDIR="/etc/puppetlabs/puppetdb"
PUPPETSERVER_CONFDIR="/etc/puppetlabs/puppetserver"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

# Script options
INTERACTIVE=true
FORCE=false

# Detection results
HAS_PUPPET=false
HAS_PUPPETSERVER=false
HAS_PUPPETDB=false
PUPPETDB_RUNNING=false
PUPPETSERVER_RUNNING=false

# PuppetDB connection info
PUPPETDB_HOST="localhost"
PUPPETDB_PORT="8081"
PUPPETDB_SSL_PORT="8081"
PUPPETDB_PLAINTEXT_PORT="8080"
PUPPETDB_USE_SSL=true

# Puppet CA info
PUPPET_CA_HOST="localhost"
PUPPET_CA_PORT="8140"

# SSL certificate paths (from Puppet)
PUPPET_SSL_DIR=""
PUPPET_CERT=""
PUPPET_KEY=""
PUPPET_CA_CERT=""

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

print_banner() {
    echo ""
    echo -e "${CYAN}${BOLD}╔══════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}${BOLD}║           OpenVox WebUI - Post-Installation Configuration        ║${NC}"
    echo -e "${CYAN}${BOLD}╚══════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

print_section() {
    echo ""
    echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BOLD}  $1${NC}"
    echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Configure OpenVox WebUI integration with Puppet infrastructure."
    echo ""
    echo "Options:"
    echo "  --non-interactive    Skip all prompts and use detected defaults"
    echo "  --force              Overwrite existing configuration"
    echo "  --help               Show this help message"
    echo ""
    echo "This script will:"
    echo "  1. Detect Puppet, PuppetServer, and PuppetDB installations"
    echo "  2. Configure PuppetDB connection (with your permission)"
    echo "  3. Set up SSL certificates for secure communication"
    echo "  4. Configure sane defaults for the web server and logging"
    echo ""
}

ask_yes_no() {
    local prompt="$1"
    local default="${2:-y}"

    if [[ "$INTERACTIVE" != "true" ]]; then
        [[ "$default" == "y" ]] && return 0 || return 1
    fi

    local yn_prompt
    if [[ "$default" == "y" ]]; then
        yn_prompt="[Y/n]"
    else
        yn_prompt="[y/N]"
    fi

    while true; do
        echo -en "${CYAN}${prompt} ${yn_prompt}: ${NC}"
        read -r response
        response=${response:-$default}
        case "$response" in
            [Yy]*) return 0 ;;
            [Nn]*) return 1 ;;
            *) echo "Please answer yes or no." ;;
        esac
    done
}

detect_puppet() {
    log_step "Detecting Puppet installation..."

    # Check for Puppet agent/client
    if command -v puppet >/dev/null 2>&1 || [ -d "$PUPPET_CONFDIR" ]; then
        HAS_PUPPET=true
        log_info "Puppet agent detected"

        # Try to get SSL directory from puppet config
        if command -v puppet >/dev/null 2>&1; then
            PUPPET_SSL_DIR=$(puppet config print ssldir 2>/dev/null || echo "/etc/puppetlabs/puppet/ssl")
        else
            PUPPET_SSL_DIR="/etc/puppetlabs/puppet/ssl"
        fi
    else
        log_warn "Puppet agent not detected"
    fi

    # Check for PuppetServer
    if [ -d "$PUPPETSERVER_CONFDIR" ] || systemctl list-unit-files | grep -q puppetserver; then
        HAS_PUPPETSERVER=true
        log_info "PuppetServer detected"

        # Check if running
        if systemctl is-active --quiet puppetserver 2>/dev/null; then
            PUPPETSERVER_RUNNING=true
            log_info "PuppetServer is running"
        else
            log_warn "PuppetServer is installed but not running"
        fi
    else
        log_warn "PuppetServer not detected"
    fi

    # Check for PuppetDB
    if [ -d "$PUPPETDB_CONFDIR" ] || systemctl list-unit-files | grep -q puppetdb; then
        HAS_PUPPETDB=true
        log_info "PuppetDB detected"

        # Check if running
        if systemctl is-active --quiet puppetdb 2>/dev/null; then
            PUPPETDB_RUNNING=true
            log_info "PuppetDB is running"
        else
            log_warn "PuppetDB is installed but not running"
        fi

        # Try to detect PuppetDB ports from config
        if [ -f "$PUPPETDB_CONFDIR/conf.d/jetty.ini" ]; then
            local ssl_port=$(grep -E "^ssl-port\s*=" "$PUPPETDB_CONFDIR/conf.d/jetty.ini" 2>/dev/null | sed 's/.*=\s*//' | tr -d ' ')
            local plain_port=$(grep -E "^port\s*=" "$PUPPETDB_CONFDIR/conf.d/jetty.ini" 2>/dev/null | sed 's/.*=\s*//' | tr -d ' ')

            [ -n "$ssl_port" ] && PUPPETDB_SSL_PORT="$ssl_port"
            [ -n "$plain_port" ] && PUPPETDB_PLAINTEXT_PORT="$plain_port"
        fi
    else
        log_warn "PuppetDB not detected"
    fi
}

detect_puppet_ssl_certs() {
    log_step "Detecting Puppet SSL certificates..."

    if [ -z "$PUPPET_SSL_DIR" ] || [ ! -d "$PUPPET_SSL_DIR" ]; then
        log_warn "Puppet SSL directory not found"
        return 1
    fi

    # Get the certname
    local certname
    if command -v puppet >/dev/null 2>&1; then
        certname=$(puppet config print certname 2>/dev/null)
    fi

    if [ -z "$certname" ]; then
        certname=$(hostname -f 2>/dev/null || hostname)
    fi

    # Check for certificate files
    local cert_path="$PUPPET_SSL_DIR/certs/${certname}.pem"
    local key_path="$PUPPET_SSL_DIR/private_keys/${certname}.pem"
    local ca_path="$PUPPET_SSL_DIR/certs/ca.pem"

    if [ -f "$cert_path" ] && [ -f "$key_path" ] && [ -f "$ca_path" ]; then
        PUPPET_CERT="$cert_path"
        PUPPET_KEY="$key_path"
        PUPPET_CA_CERT="$ca_path"
        log_info "Found Puppet SSL certificates:"
        log_info "  Certificate: $PUPPET_CERT"
        log_info "  Private key: $PUPPET_KEY"
        log_info "  CA cert:     $PUPPET_CA_CERT"
        return 0
    else
        log_warn "Puppet SSL certificates not found or incomplete"
        [ ! -f "$cert_path" ] && log_warn "  Missing: $cert_path"
        [ ! -f "$key_path" ] && log_warn "  Missing: $key_path"
        [ ! -f "$ca_path" ] && log_warn "  Missing: $ca_path"
        return 1
    fi
}

test_puppetdb_connection() {
    local url="$1"
    local use_ssl="$2"

    log_info "Testing PuppetDB connection: $url"

    local curl_opts="-s -o /dev/null -w %{http_code} --max-time 5"

    if [ "$use_ssl" == "true" ] && [ -n "$PUPPET_CERT" ]; then
        curl_opts="$curl_opts --cert $PUPPET_CERT --key $PUPPET_KEY --cacert $PUPPET_CA_CERT"
    elif [ "$use_ssl" == "true" ]; then
        curl_opts="$curl_opts -k"  # Skip SSL verification if no certs
    fi

    local status_code
    status_code=$(curl $curl_opts "${url}/pdb/query/v4/nodes" 2>/dev/null || echo "000")

    if [ "$status_code" == "200" ]; then
        log_info "PuppetDB connection successful!"
        return 0
    else
        log_warn "PuppetDB connection failed (HTTP $status_code)"
        return 1
    fi
}

configure_puppetdb_access() {
    print_section "PuppetDB Configuration"

    if [ "$HAS_PUPPETDB" != "true" ]; then
        echo -e "${YELLOW}╔══════════════════════════════════════════════════════════════════╗${NC}"
        echo -e "${YELLOW}║                         ⚠  WARNING                               ║${NC}"
        echo -e "${YELLOW}╠══════════════════════════════════════════════════════════════════╣${NC}"
        echo -e "${YELLOW}║  PuppetDB was not detected on this system.                       ║${NC}"
        echo -e "${YELLOW}║                                                                  ║${NC}"
        echo -e "${YELLOW}║  PuppetDB integration is HIGHLY RECOMMENDED for OpenVox WebUI.  ║${NC}"
        echo -e "${YELLOW}║  Without it, the following features will be UNAVAILABLE:        ║${NC}"
        echo -e "${YELLOW}║                                                                  ║${NC}"
        echo -e "${YELLOW}║    • Node inventory and status dashboard                         ║${NC}"
        echo -e "${YELLOW}║    • Fact browsing and searching                                 ║${NC}"
        echo -e "${YELLOW}║    • Report viewing and analysis                                 ║${NC}"
        echo -e "${YELLOW}║    • Resource exploration                                        ║${NC}"
        echo -e "${YELLOW}║    • Node classification based on facts                          ║${NC}"
        echo -e "${YELLOW}║    • Historical data and trends                                  ║${NC}"
        echo -e "${YELLOW}║                                                                  ║${NC}"
        echo -e "${YELLOW}║  You can still use OpenVox WebUI for:                            ║${NC}"
        echo -e "${YELLOW}║    • Facter template management                                  ║${NC}"
        echo -e "${YELLOW}║    • User and RBAC management                                    ║${NC}"
        echo -e "${YELLOW}║    • Basic Puppet CA operations (if PuppetServer is present)    ║${NC}"
        echo -e "${YELLOW}╚══════════════════════════════════════════════════════════════════╝${NC}"
        echo ""

        if ask_yes_no "Would you like to configure a remote PuppetDB connection?" "n"; then
            echo -en "${CYAN}Enter PuppetDB hostname [localhost]: ${NC}"
            read -r input_host
            PUPPETDB_HOST="${input_host:-localhost}"

            echo -en "${CYAN}Enter PuppetDB SSL port [8081]: ${NC}"
            read -r input_port
            PUPPETDB_PORT="${input_port:-8081}"
            PUPPETDB_USE_SSL=true
        else
            log_info "Skipping PuppetDB configuration"
            return 0
        fi
    fi

    if [ "$HAS_PUPPETSERVER" != "true" ]; then
        echo ""
        echo -e "${RED}╔══════════════════════════════════════════════════════════════════╗${NC}"
        echo -e "${RED}║                        ⚠  IMPORTANT                              ║${NC}"
        echo -e "${RED}╠══════════════════════════════════════════════════════════════════╣${NC}"
        echo -e "${RED}║  PuppetServer was not detected on this system.                   ║${NC}"
        echo -e "${RED}║                                                                  ║${NC}"
        echo -e "${RED}║  OpenVox WebUI should be installed on the SAME machine where    ║${NC}"
        echo -e "${RED}║  PuppetServer and Puppet CA are running for full functionality. ║${NC}"
        echo -e "${RED}║                                                                  ║${NC}"
        echo -e "${RED}║  Without local PuppetServer:                                     ║${NC}"
        echo -e "${RED}║    • Certificate signing/revoking will not work                  ║${NC}"
        echo -e "${RED}║    • CA management features will be limited                      ║${NC}"
        echo -e "${RED}║    • SSL certificate auto-detection will fail                    ║${NC}"
        echo -e "${RED}╚══════════════════════════════════════════════════════════════════╝${NC}"
        echo ""
    fi

    # Try to detect and copy SSL certificates
    if detect_puppet_ssl_certs; then
        echo ""
        echo -e "${GREEN}Found Puppet SSL certificates that can be used for PuppetDB access.${NC}"
        echo ""

        if ask_yes_no "Copy Puppet SSL certificates to OpenVox WebUI configuration?" "y"; then
            log_step "Copying SSL certificates..."

            # Create SSL directory if needed
            mkdir -p "$SSL_DIR"

            # Copy certificates with appropriate permissions
            cp "$PUPPET_CERT" "$SSL_DIR/client.pem"
            cp "$PUPPET_KEY" "$SSL_DIR/client.key"
            cp "$PUPPET_CA_CERT" "$SSL_DIR/ca.pem"

            # Set permissions
            chown root:openvox-webui "$SSL_DIR"/*.pem "$SSL_DIR"/*.key 2>/dev/null || true
            chmod 640 "$SSL_DIR"/*.pem "$SSL_DIR"/*.key 2>/dev/null || true

            log_info "SSL certificates copied successfully"

            # Update paths for config
            PUPPET_CERT="$SSL_DIR/client.pem"
            PUPPET_KEY="$SSL_DIR/client.key"
            PUPPET_CA_CERT="$SSL_DIR/ca.pem"
        fi
    fi

    # Configure PuppetDB access permissions if PuppetDB is local
    if [ "$HAS_PUPPETDB" == "true" ] && [ -f "$PUPPETDB_CONFDIR/conf.d/auth.conf" ]; then
        echo ""
        echo -e "${CYAN}PuppetDB may need authorization rules to allow OpenVox WebUI access.${NC}"
        echo ""

        if ask_yes_no "Would you like me to check and configure PuppetDB authorization?" "y"; then
            configure_puppetdb_auth
        fi
    fi

    # Test the connection
    local puppetdb_url
    if [ "$PUPPETDB_USE_SSL" == "true" ]; then
        puppetdb_url="https://${PUPPETDB_HOST}:${PUPPETDB_PORT}"
    else
        puppetdb_url="http://${PUPPETDB_HOST}:${PUPPETDB_PLAINTEXT_PORT}"
    fi

    if command -v curl >/dev/null 2>&1; then
        echo ""
        if ask_yes_no "Test PuppetDB connection now?" "y"; then
            test_puppetdb_connection "$puppetdb_url" "$PUPPETDB_USE_SSL"
        fi
    fi

    return 0
}

configure_puppetdb_auth() {
    log_step "Configuring PuppetDB authorization..."

    local auth_conf="$PUPPETDB_CONFDIR/conf.d/auth.conf"
    local certname

    if command -v puppet >/dev/null 2>&1; then
        certname=$(puppet config print certname 2>/dev/null)
    fi
    certname="${certname:-$(hostname -f 2>/dev/null || hostname)}"

    # Check if auth.conf exists and has the required rule
    if [ -f "$auth_conf" ]; then
        if grep -q "openvox-webui" "$auth_conf" 2>/dev/null; then
            log_info "PuppetDB authorization already configured for OpenVox WebUI"
            return 0
        fi

        echo ""
        echo -e "${YELLOW}The following authorization rule will be added to PuppetDB:${NC}"
        echo ""
        echo "  This allows the certificate '$certname' to query PuppetDB"
        echo "  for nodes, facts, reports, resources, and catalogs."
        echo ""

        if ask_yes_no "Add authorization rule to $auth_conf?" "y"; then
            # Backup the original
            cp "$auth_conf" "${auth_conf}.bak.$(date +%Y%m%d%H%M%S)"

            # Add the authorization rule
            # Note: This is a simplified approach - in production you might want
            # to use puppetlabs-puppetdb module or manual HOCON editing
            cat >> "$auth_conf" << EOF

# OpenVox WebUI access - added by installer
authorization: {
    version: 1
    rules: [
        {
            match-request: {
                path: "/"
                type: path
            }
            allow: ["$certname"]
            sort-order: 500
            name: "openvox-webui access"
        }
    ]
}
EOF

            log_info "Authorization rule added to PuppetDB"

            echo ""
            echo -e "${YELLOW}NOTE: You may need to restart PuppetDB for changes to take effect:${NC}"
            echo -e "${YELLOW}  systemctl restart puppetdb${NC}"
            echo ""

            if ask_yes_no "Restart PuppetDB now?" "n"; then
                systemctl restart puppetdb
                log_info "PuppetDB restarted"
            fi
        fi
    else
        log_warn "PuppetDB auth.conf not found at $auth_conf"
        log_warn "Manual configuration may be required"
    fi
}

generate_jwt_secret() {
    # Generate a secure random JWT secret
    if command -v openssl >/dev/null 2>&1; then
        openssl rand -base64 48 | tr -d '\n'
    elif [ -f /dev/urandom ]; then
        head -c 48 /dev/urandom | base64 | tr -d '\n'
    else
        # Fallback - not ideal but better than default
        echo "$(date +%s%N)$(hostname)$$" | sha256sum | head -c 64
    fi
}

configure_defaults() {
    print_section "Configuring Default Settings"

    log_step "Setting up sane defaults for web server and logging..."

    # Backup existing config if present
    if [ -f "$CONFIG_FILE" ] && [ "$FORCE" != "true" ]; then
        cp "$CONFIG_FILE" "$CONFIG_BACKUP"
        log_info "Backed up existing config to $CONFIG_BACKUP"
    fi

    # Generate a new JWT secret
    local jwt_secret
    jwt_secret=$(generate_jwt_secret)

    # Determine PuppetDB URL
    local puppetdb_url
    if [ "$PUPPETDB_USE_SSL" == "true" ]; then
        puppetdb_url="https://${PUPPETDB_HOST}:${PUPPETDB_PORT}"
    else
        puppetdb_url="http://${PUPPETDB_HOST}:${PUPPETDB_PLAINTEXT_PORT}"
    fi

    # Create the configuration file
    cat > "$CONFIG_FILE" << EOF
# OpenVox WebUI Configuration
# Generated by post-install script on $(date)
# Manual edits will be preserved on package upgrades

# Server settings
server:
  # Listen on all interfaces - change to 127.0.0.1 if using a reverse proxy
  host: "0.0.0.0"
  port: 5051

  # Frontend static files
  serve_frontend: true
  static_dir: "/usr/share/openvox-webui/static"

  # TLS/HTTPS configuration (recommended for production)
  # Uncomment and configure if you want HTTPS directly (without reverse proxy)
  # tls:
  #   cert_file: "/etc/openvox-webui/ssl/server.crt"
  #   key_file: "/etc/openvox-webui/ssl/server.key"
  #   min_version: "1.2"

# PuppetDB connection settings
puppetdb:
  url: "$puppetdb_url"
  timeout_secs: 30
  ssl_verify: true
EOF

    # Add SSL settings if we have certificates
    if [ -n "$PUPPET_CERT" ] && [ -f "$PUPPET_CERT" ]; then
        cat >> "$CONFIG_FILE" << EOF
  ssl_cert: "$SSL_DIR/client.pem"
  ssl_key: "$SSL_DIR/client.key"
  ssl_ca: "$SSL_DIR/ca.pem"
EOF
    else
        cat >> "$CONFIG_FILE" << EOF
  # Uncomment and set paths for SSL client authentication
  # ssl_cert: "/etc/openvox-webui/ssl/client.pem"
  # ssl_key: "/etc/openvox-webui/ssl/client.key"
  # ssl_ca: "/etc/openvox-webui/ssl/ca.pem"
EOF
    fi

    cat >> "$CONFIG_FILE" << EOF

# Puppet CA connection settings
puppet_ca:
  url: "https://${PUPPET_CA_HOST}:${PUPPET_CA_PORT}"
  timeout_secs: 30
  ssl_verify: true
EOF

    if [ -n "$PUPPET_CERT" ] && [ -f "$PUPPET_CERT" ]; then
        cat >> "$CONFIG_FILE" << EOF
  ssl_cert: "$SSL_DIR/client.pem"
  ssl_key: "$SSL_DIR/client.key"
  ssl_ca: "$SSL_DIR/ca.pem"
EOF
    else
        cat >> "$CONFIG_FILE" << EOF
  # ssl_cert: "/etc/openvox-webui/ssl/ca_client.pem"
  # ssl_key: "/etc/openvox-webui/ssl/ca_client.key"
  # ssl_ca: "/etc/openvox-webui/ssl/ca.pem"
EOF
    fi

    cat >> "$CONFIG_FILE" << EOF

# Authentication settings
auth:
  # IMPORTANT: This is a randomly generated secret. Keep it safe!
  jwt_secret: "$jwt_secret"
  token_expiry_hours: 24
  refresh_token_expiry_days: 7
  bcrypt_cost: 12
  password_min_length: 8

# Database settings (SQLite)
database:
  url: "sqlite://$DATA_DIR/openvox.db"
  max_connections: 10
  min_connections: 1
  connect_timeout_secs: 30
  idle_timeout_secs: 600

# Logging configuration - production defaults
logging:
  level: "info"
  format: "json"  # JSON format for easier log parsing
  target: "file"  # Log to files in production
  log_dir: "$LOG_DIR"
  log_prefix: "openvox-webui"
  daily_rotation: true
  max_log_files: 30  # Keep 30 days of logs

# Cache configuration
cache:
  enabled: true
  node_ttl_secs: 300
  fact_ttl_secs: 300
  report_ttl_secs: 60
  resource_ttl_secs: 600
  catalog_ttl_secs: 600
  max_entries: 10000
  sync_interval_secs: 0

# Dashboard settings
dashboard:
  default_time_range: "24h"
  refresh_interval_secs: 60
  nodes_per_page: 50
  reports_per_page: 25
  show_inactive_nodes: true
  inactive_threshold_hours: 24
  theme: "system"  # Follow system dark/light mode preference

# RBAC configuration
rbac:
  default_role: "viewer"
  session_timeout_minutes: 480
  max_failed_logins: 5
  lockout_duration_minutes: 30
EOF

    # Set proper permissions
    chown root:openvox-webui "$CONFIG_FILE"
    chmod 640 "$CONFIG_FILE"

    log_info "Configuration file created: $CONFIG_FILE"
}

print_summary() {
    print_section "Installation Summary"

    echo -e "${GREEN}OpenVox WebUI has been configured!${NC}"
    echo ""
    echo "Configuration summary:"
    echo "  • Config file:     $CONFIG_FILE"
    echo "  • Data directory:  $DATA_DIR"
    echo "  • Log directory:   $LOG_DIR"
    echo "  • Web interface:   http://$(hostname -f 2>/dev/null || hostname):5051"
    echo ""

    if [ "$HAS_PUPPETDB" == "true" ]; then
        echo -e "  • PuppetDB:        ${GREEN}Configured${NC}"
    else
        echo -e "  • PuppetDB:        ${YELLOW}Not configured (limited functionality)${NC}"
    fi

    if [ "$HAS_PUPPETSERVER" == "true" ]; then
        echo -e "  • Puppet CA:       ${GREEN}Available${NC}"
    else
        echo -e "  • Puppet CA:       ${YELLOW}Not available${NC}"
    fi

    echo ""
    echo -e "${BOLD}Next steps:${NC}"
    echo "  1. Review the configuration: $CONFIG_FILE"
    echo "  2. Start the service:        systemctl start openvox-webui"
    echo "  3. Enable on boot:           systemctl enable openvox-webui"
    echo "  4. Access the web interface: http://$(hostname -f 2>/dev/null || hostname):5051"
    echo ""
    echo -e "${YELLOW}Default admin credentials (change immediately!):${NC}"
    echo "  Username: admin"
    echo "  Password: admin"
    echo ""

    if [ "$HAS_PUPPETDB" != "true" ] || [ "$HAS_PUPPETSERVER" != "true" ]; then
        echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo -e "${YELLOW}  To reconfigure later, run:${NC}"
        echo -e "${YELLOW}    /usr/share/openvox-webui/scripts/configure-openvox-webui.sh${NC}"
        echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo ""
    fi
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --non-interactive)
            INTERACTIVE=false
            shift
            ;;
        --force)
            FORCE=true
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Main execution
main() {
    print_banner

    # Check if running as root
    if [ "$(id -u)" -ne 0 ]; then
        log_error "This script must be run as root"
        exit 1
    fi

    # Check if config already exists and we're not forcing
    if [ -f "$CONFIG_FILE" ] && [ "$FORCE" != "true" ]; then
        if grep -q "Generated by post-install script" "$CONFIG_FILE" 2>/dev/null; then
            log_info "Configuration already exists and appears to be generated by this script"
            if ! ask_yes_no "Reconfigure anyway?" "n"; then
                log_info "Keeping existing configuration"
                exit 0
            fi
        fi
    fi

    # Detect Puppet infrastructure
    detect_puppet

    # Configure PuppetDB access
    configure_puppetdb_access

    # Configure defaults
    configure_defaults

    # Print summary
    print_summary
}

main "$@"
