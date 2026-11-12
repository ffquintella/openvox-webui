#!/bin/bash
#
# Puppet Module Build and Publish Script
#
# This script validates, tests, builds, and publishes the openvox-webui
# Puppet module to Puppet Forge.
#
# Requirements:
# - Regent (https://github.com/ffquintella/regent) installed
# - Puppet Forge API token ($FORGE_TOKEN, $REGENT_FORGE_TOKEN, ~/.regent/forge_token or ~/.puppetlabs/token)
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

# Check if running from repository root
if [[ ! -d "$MODULE_DIR" ]] || [[ ! -f "$MODULE_DIR/metadata.json" ]]; then
    fatal "Must run from repository root. Module directory not found: $MODULE_DIR"
fi

cd "$MODULE_DIR"

info "Starting Puppet module build and publish process..."

# Step 1: Check for required tools
info "Checking for required tools..."

if ! command -v regent &> /dev/null; then
    fatal "Regent not found. Install with: cargo install regent (see https://github.com/ffquintella/regent)"
fi

if ! command -v jq &> /dev/null; then
    fatal "jq not found. Install it (e.g. brew install jq)"
fi

success "Required tools found"

# Step 2: Check Git status
info "Checking Git repository status..."

if [[ -n $(git status --porcelain) ]]; then
    warning "Git repository has uncommitted changes"
    git status --short
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        fatal "Aborted by user"
    fi
else
    success "Git repository clean"
fi

# Step 3: Get version
if [[ -z "$VERSION" ]]; then
    VERSION=$(jq -r '.version' metadata.json)
    info "Using version from metadata.json: $VERSION"
else
    info "Using specified version: $VERSION"
    jq ".version = \"$VERSION\"" metadata.json > metadata.json.tmp
    mv metadata.json.tmp metadata.json
    success "Updated metadata.json to version $VERSION"
fi

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9]+)?$ ]]; then
    fatal "Invalid version format: $VERSION (expected: X.Y.Z or X.Y.Z-prerelease)"
fi

# Step 4: Validate module (syntax, metadata, templates)
info "Validating module with regent..."
if ! regent validate .; then
    fatal "Module validation failed"
fi
success "Validation passed"

# Step 5: Run tests (unless skipped)
if [[ "$SKIP_TESTS" == "false" ]]; then
    info "Running tests with regent..."
    if ! regent test .; then
        fatal "Tests failed"
    fi
    success "Tests passed"
else
    warning "Skipping tests (not recommended for production releases)"
fi

# Step 6: Build module package
info "Building module package..."

if [[ -d "$BUILD_DIR" ]]; then
    rm -rf "$BUILD_DIR"
fi

if ! regent build . --output "$BUILD_DIR"; then
    fatal "Module build failed"
fi

PACKAGE_FILE=$(find "$BUILD_DIR" -name "*${MODULE_NAME}*${VERSION}*.tar.gz" | head -1)

if [[ -z "$PACKAGE_FILE" || ! -f "$PACKAGE_FILE" ]]; then
    fatal "Built package not found: ${FORGE_USER}-${MODULE_NAME}-${VERSION}.tar.gz"
fi

PACKAGE_SIZE=$(du -h "$PACKAGE_FILE" | cut -f1)
success "Module package built: $PACKAGE_FILE ($PACKAGE_SIZE)"

info "Package contents (first 20 files):"
tar -tzf "$PACKAGE_FILE" | head -20

# Step 7: Publish to Forge (unless dry-run)
if [[ "$DRY_RUN" == "true" ]]; then
    regent publish . --file "$PACKAGE_FILE" --no-build --dry-run
    success "DRY RUN MODE: Build successful but not publishing to Forge"
    info "Package ready: $PACKAGE_FILE"
    exit 0
fi

get_forge_token() {
    if [[ -n "$FORGE_TOKEN" ]]; then
        echo "$FORGE_TOKEN"
        return
    fi
    if [[ -n "$REGENT_FORGE_TOKEN" ]]; then
        echo "$REGENT_FORGE_TOKEN"
        return
    fi
    if [[ -f "$HOME/.regent/forge_token" ]]; then
        head -n1 "$HOME/.regent/forge_token"
        return
    fi
    if [[ -f "$HOME/.puppetlabs/token" ]]; then
        head -n1 "$HOME/.puppetlabs/token"
        return
    fi
}

info "Checking Forge credentials..."

FORGE_AUTH_TOKEN=$(get_forge_token)
if [[ -z "$FORGE_AUTH_TOKEN" ]]; then
    warning "No Forge API token found."
    info "You can create one at: https://forge.puppet.com/settings/tokens"
    read -rsp "Forge API token: " FORGE_AUTH_TOKEN
    echo
fi

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

info "Publishing to Puppet Forge with regent..."
if ! REGENT_FORGE_TOKEN="$FORGE_AUTH_TOKEN" regent publish . --file "$PACKAGE_FILE" --no-build; then
    fatal "Module publication failed"
fi
success "Module published successfully!"

# Tag release in Git
info "Creating Git tag puppet-v${VERSION}..."
cd ..
if git tag -a "puppet-v${VERSION}" -m "Puppet module release ${VERSION}"; then
    success "Git tag created: puppet-v${VERSION}"
else
    warning "Failed to create Git tag (tag may already exist)"
fi

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
echo ""
