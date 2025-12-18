#!/bin/bash
#
# Puppet Module Build and Publish Script
#
# This script validates, tests, builds, and publishes the openvox-webui
# Puppet module to Puppet Forge.
#
# Requirements:
# - Puppet Development Kit (PDK) installed
# - Puppet Forge account configured (puppet module --forge-api-key)
# - Git repository in clean state
#
# Usage:
#   ./scripts/publish-puppet-module.sh [OPTIONS]
#
# Options:
#   --dry-run       Run tests and build but don't publish
#   --skip-tests    Skip running tests (not recommended)
#   --version VER   Set version to publish (default: from metadata.json)
#   -h, --help      Show this help message

set -e

# Configuration
MODULE_DIR="puppet"
MODULE_NAME="openvox_webui"
FORGE_USER="ffquintella"
BUILD_DIR="pkg"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Parse command line arguments
DRY_RUN=false
SKIP_TESTS=false
VERSION=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --skip-tests)
            SKIP_TESTS=true
            shift
            ;;
        --version)
            VERSION="$2"
            shift 2
            ;;
        -h|--help)
            grep '^#' "$0" | grep -v '#!/bin/bash' | sed 's/^# //' | sed 's/^#//'
            exit 0
            ;;
        *)
            echo -e "${RED}Error: Unknown option $1${NC}"
            exit 1
            ;;
    esac
done

# Helper functions
info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $*"
}

error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

fatal() {
    error "$*"
    exit 1
}

# Flags
PDK_COMPATIBLE=true

# Check if running from repository root
if [[ ! -d "$MODULE_DIR" ]] || [[ ! -f "$MODULE_DIR/metadata.json" ]]; then
    fatal "Must run from repository root. Module directory not found: $MODULE_DIR"
fi

cd "$MODULE_DIR"

info "Starting Puppet module build and publish process..."

# Step 1: Check for required tools
info "Checking for required tools..."

if ! command -v pdk &> /dev/null; then
    fatal "PDK (Puppet Development Kit) not found. Install from: https://puppet.com/try-puppet/puppet-development-kit"
fi

# Prefer system puppet; fall back to PDK's embedded puppet via bundle exec
if command -v puppet &> /dev/null; then
    PUPPET_CMD=(puppet)
elif pdk bundle exec puppet --version >/dev/null 2>&1; then
    PUPPET_CMD=(pdk bundle exec puppet)
else
    fatal "Puppet not found (tried system puppet and 'pdk bundle exec puppet'). Install Puppet or PDK."
fi

success "Required tools found"

# Step 2: Validate module structure
info "Validating module structure..."

METADATA_LOG=$(mktemp)
if ! pdk validate metadata --parallel >"$METADATA_LOG" 2>&1; then
    if grep -qi "not PDK compatible" "$METADATA_LOG"; then
        warning "Module is not PDK compatible; falling back to Puppet CLI tooling for build/test"
        PDK_COMPATIBLE=false
    else
        cat "$METADATA_LOG"
        rm -f "$METADATA_LOG"
        fatal "Module metadata validation failed"
    fi
fi
rm -f "$METADATA_LOG"

if [[ "$PDK_COMPATIBLE" == "true" ]]; then
    success "Module structure validated with PDK"
else
    success "Module structure checked (non-PDK module; continuing with Puppet CLI tooling)"
fi

# Step 3: Check Git status
info "Checking Git repository status..."

if [[ -n $(git status --porcelain) ]]; then
    warning "Git repository has uncommitted changes"
    git status --short
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        fatal "Aborted by user"
    fi
fi

success "Git repository clean"

# Step 4: Get version
if [[ -z "$VERSION" ]]; then
    VERSION=$(jq -r '.version' metadata.json)
    info "Using version from metadata.json: $VERSION"
else
    info "Using specified version: $VERSION"
    # Update metadata.json with specified version
    jq ".version = \"$VERSION\"" metadata.json > metadata.json.tmp
    mv metadata.json.tmp metadata.json
    success "Updated metadata.json to version $VERSION"
fi

# Validate version format (semantic versioning)
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9]+)?$ ]]; then
    fatal "Invalid version format: $VERSION (expected: X.Y.Z or X.Y.Z-prerelease)"
fi

# Step 5: Check if version already exists on Forge
info "Checking if version $VERSION already exists on Forge..."

if "${PUPPET_CMD[@]}" module list --modulepath=. 2>/dev/null | grep -q "$MODULE_NAME.*$VERSION"; then
    warning "Version $VERSION may already exist"
fi

# Step 6: Run linting
info "Running puppet-lint..."

if [[ "$PDK_COMPATIBLE" == "true" ]]; then
    if ! pdk validate puppet --parallel; then
        fatal "Puppet validation (lint) failed"
    fi
    success "Linting passed"
else
    warning "Skipping PDK lint (module not PDK compatible); parser/epp validation will still run"
fi

# Step 7: Run tests (unless skipped)
if [[ "$SKIP_TESTS" == "false" ]]; then
    if [[ "$PDK_COMPATIBLE" == "true" ]]; then
        info "Running unit tests with rspec-puppet..."
        
        # Check if spec tests exist
        if [[ -d "spec" ]] && [[ -f "spec/spec_helper.rb" ]]; then
            if ! pdk test unit --parallel; then
                fatal "Unit tests failed"
            fi
            success "Unit tests passed"
        else
            warning "No spec tests found, skipping"
        fi
    else
        warning "Skipping PDK unit tests (module not PDK compatible)"
    fi
    
    # Run syntax validation
    info "Running syntax validation..."
    if ! "${PUPPET_CMD[@]}" parser validate manifests/*.pp; then
        fatal "Syntax validation failed"
    fi
    success "Syntax validation passed"
    
    # Validate templates
    info "Validating EPP templates..."
    for template in templates/*.epp; do
        if [[ -f "$template" ]]; then
            if ! "${PUPPET_CMD[@]}" epp validate "$template"; then
                fatal "Template validation failed: $template"
            fi
        fi
    done
    success "Template validation passed"
else
    warning "Skipping tests (not recommended for production releases)"
fi

# Step 8: Build module package
info "Building module package..."

# Clean previous builds
if [[ -d "$BUILD_DIR" ]]; then
    rm -rf "$BUILD_DIR"
fi

if [[ "$PDK_COMPATIBLE" == "true" ]]; then
    if ! pdk build --force; then
        fatal "Module build failed"
    fi
else
    BUILD_LOG=$(mktemp)
    if "${PUPPET_CMD[@]}" module build --target-dir "$BUILD_DIR" >"$BUILD_LOG" 2>&1; then
        :
    else
        if grep -qi "no .*build" "$BUILD_LOG"; then
            warning "Puppet CLI does not support 'module build'; attempting pdk build even though module is not PDK compatible"
            if ! pdk build --force; then
                cat "$BUILD_LOG"
                rm -f "$BUILD_LOG"
                fatal "Module build failed (Puppet CLI lacks build and PDK build failed)"
            fi
        else
            cat "$BUILD_LOG"
            rm -f "$BUILD_LOG"
            fatal "Module build failed"
        fi
    fi
    rm -f "$BUILD_LOG"
fi

# Find the built package
PACKAGE_FILE=$(find "$BUILD_DIR" -name "*${MODULE_NAME}*${VERSION}*.tar.gz" | head -1)

if [[ ! -f "$PACKAGE_FILE" ]]; then
    fatal "Built package not found: ${FORGE_USER}-${MODULE_NAME}-${VERSION}.tar.gz"
fi

PACKAGE_SIZE=$(du -h "$PACKAGE_FILE" | cut -f1)
success "Module package built: $PACKAGE_FILE ($PACKAGE_SIZE)"

# Step 9: Inspect package contents
info "Package contents:"
tar -tzf "$PACKAGE_FILE" | head -20
echo "... (showing first 20 files)"

# Step 10: Publish to Forge (unless dry-run)
if [[ "$DRY_RUN" == "true" ]]; then
    success "DRY RUN MODE: Build successful but not publishing to Forge"
    info "Package ready: $PACKAGE_FILE"
    exit 0
fi

get_forge_token() {
    if [[ -n "$FORGE_TOKEN" ]]; then
        echo "$FORGE_TOKEN"
        return
    fi
    if [[ -f "$HOME/.puppetlabs/token" ]]; then
        head -n1 "$HOME/.puppetlabs/token"
        return
    fi
    if [[ -f "$HOME/.puppetlabs/puppet-forge.conf" ]]; then
        # shellcheck disable=SC2002
        cat "$HOME/.puppetlabs/puppet-forge.conf" | grep -o '"token"[[:space:]]*:[[:space:]]*"[^"]*"' | head -n1 | cut -d'"' -f4
        return
    fi
}

# Check for Forge credentials
info "Checking Forge credentials..."

FORGE_AUTH_TOKEN=$(get_forge_token)
if [[ -z "$FORGE_AUTH_TOKEN" ]]; then
    warning "No Forge API token found."
    info "You can create one at: https://forge.puppet.com/settings/tokens"
    read -rsp "Forge API token: " FORGE_AUTH_TOKEN
    echo
fi

# Confirm publication
echo ""
warning "About to publish to Puppet Forge:"
info "  Module: ${FORGE_USER}/${MODULE_NAME}"
info "  Version: $VERSION"
info "  Package: $PACKAGE_FILE"
echo ""
read -p "Proceed with publication? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    info "Aborted by user"
    exit 0
fi

# Publish to Forge (always via Forge API; puppet module upload is not supported)
info "Publishing to Puppet Forge via API..."

CURL_AUTH=(-H "Authorization: Bearer ${FORGE_AUTH_TOKEN}")

FORGE_RESPONSE=$(mktemp)
if curl -sS -w "%{http_code}" -o "$FORGE_RESPONSE" -X POST \
    "${CURL_AUTH[@]}" \
    -F "file=@${PACKAGE_FILE}" \
    https://forgeapi.puppet.com/v3/releases | {
        read -r status
        if [[ "$status" == "200" || "$status" == "201" ]]; then
            success "Module published successfully via Forge API!"
            info "View on Forge: https://forge.puppet.com/modules/${FORGE_USER}/${MODULE_NAME}"
        else
            echo "HTTP $status"
            cat "$FORGE_RESPONSE"
            rm -f "$FORGE_RESPONSE"
            exit 1
        fi
    }; then
    :
else
    fatal "Module publication failed via Forge API"
fi
rm -f "$FORGE_RESPONSE"

# Tag release in Git
info "Creating Git tag v${VERSION}..."
cd ..
if git tag -a "puppet-v${VERSION}" -m "Puppet module release ${VERSION}"; then
    success "Git tag created: puppet-v${VERSION}"
    info "Push tag with: git push origin puppet-v${VERSION}"
else
    warning "Failed to create Git tag (tag may already exist)"
fi

# Step 11: Verify publication
info "Verifying publication..."
sleep 5  # Give Forge a moment to index

if "${PUPPET_CMD[@]}" module search "$MODULE_NAME" | grep -q "${FORGE_USER}/${MODULE_NAME}"; then
    success "Module verified on Forge"
else
    warning "Could not verify module on Forge (may take a few minutes to appear)"
fi

# Final summary
echo ""
success "========================================="
success "  Puppet Module Published Successfully!"
success "========================================="
info "Module: ${FORGE_USER}/${MODULE_NAME}"
info "Version: $VERSION"
info "Forge URL: https://forge.puppet.com/modules/${FORGE_USER}/${MODULE_NAME}"
echo ""
info "Next steps:"
info "1. Push Git tag: git push origin puppet-v${VERSION}"
info "2. Create GitHub release"
info "3. Update documentation if needed"
info "4. Announce release to users"
echo ""
