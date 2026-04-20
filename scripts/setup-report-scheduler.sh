#!/bin/bash
#
# Setup script for OpenVox WebUI scheduled report generation
#
# This script sets up cron to run scheduled reports.
# It should be run as root or with sudo.
#
# Usage:
#   sudo ./scripts/setup-report-scheduler.sh [OPTIONS]
#
# Options:
#   --interval <minutes>  Run interval in minutes (default: 1)
#   --config <path>       Path to config file (default: /etc/openvox-webui/config.yaml)
#   --user <user>         User to run as (default: openvox)
#   --remove              Remove the cron job
#   --status              Show current cron job status

set -e

# Default values
INTERVAL=1
CONFIG_PATH="/etc/openvox-webui/config.yaml"
RUN_USER="openvox"
BINARY_PATH="/usr/local/bin/run-scheduled-reports"
LOG_PATH="/var/log/openvox-webui/scheduled-reports.log"
CRON_ID="openvox-scheduled-reports"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --interval)
            INTERVAL="$2"
            shift 2
            ;;
        --config)
            CONFIG_PATH="$2"
            shift 2
            ;;
        --user)
            RUN_USER="$2"
            shift 2
            ;;
        --remove)
            REMOVE=1
            shift
            ;;
        --status)
            STATUS=1
            shift
            ;;
        --help|-h)
            echo "Setup script for OpenVox WebUI scheduled report generation"
            echo ""
            echo "Usage: sudo $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --interval <minutes>  Run interval in minutes (default: 1)"
            echo "  --config <path>       Path to config file (default: /etc/openvox-webui/config.yaml)"
            echo "  --user <user>         User to run as (default: openvox)"
            echo "  --remove              Remove the cron job"
            echo "  --status              Show current cron job status"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check if running as root
if [[ $EUID -ne 0 ]] && [[ -z "$STATUS" ]]; then
    echo "This script must be run as root (use sudo)" >&2
    exit 1
fi

# Show status
if [[ -n "$STATUS" ]]; then
    echo "Checking cron job status..."
    if crontab -l -u "$RUN_USER" 2>/dev/null | grep -q "$CRON_ID"; then
        echo "Cron job is installed:"
        crontab -l -u "$RUN_USER" | grep "$CRON_ID"
    else
        echo "No cron job found for $RUN_USER"
    fi
    exit 0
fi

# Remove cron job
if [[ -n "$REMOVE" ]]; then
    echo "Removing scheduled reports cron job..."
    (crontab -l -u "$RUN_USER" 2>/dev/null | grep -v "$CRON_ID") | crontab -u "$RUN_USER" - || true
    echo "Cron job removed."
    exit 0
fi

# Validate configuration
if [[ ! -f "$CONFIG_PATH" ]]; then
    echo "Error: Configuration file not found: $CONFIG_PATH" >&2
    exit 1
fi

if [[ ! -f "$BINARY_PATH" ]]; then
    echo "Warning: Binary not found at $BINARY_PATH"
    echo "Make sure to install the binary first with:"
    echo "  cargo build --release"
    echo "  sudo cp target/release/run-scheduled-reports $BINARY_PATH"
fi

# Create log directory
mkdir -p "$(dirname "$LOG_PATH")"
chown "$RUN_USER:$RUN_USER" "$(dirname "$LOG_PATH")" || true

# Build cron expression
if [[ "$INTERVAL" == "1" ]]; then
    CRON_EXPR="* * * * *"
else
    CRON_EXPR="*/$INTERVAL * * * *"
fi

# Build cron entry
CRON_ENTRY="$CRON_EXPR $BINARY_PATH --config $CONFIG_PATH >> $LOG_PATH 2>&1 # $CRON_ID"

echo "Setting up scheduled reports cron job..."
echo "  Interval: every $INTERVAL minute(s)"
echo "  Config: $CONFIG_PATH"
echo "  User: $RUN_USER"
echo "  Log: $LOG_PATH"
echo ""
echo "Cron entry:"
echo "  $CRON_ENTRY"
echo ""

# Install cron job
# Remove existing entry if present, then add new one
(crontab -l -u "$RUN_USER" 2>/dev/null | grep -v "$CRON_ID"; echo "$CRON_ENTRY") | crontab -u "$RUN_USER" -

echo "Cron job installed successfully!"
echo ""
echo "To verify, run:"
echo "  sudo $0 --status"
echo ""
echo "To remove, run:"
echo "  sudo $0 --remove"
echo ""
echo "To view logs:"
echo "  tail -f $LOG_PATH"
