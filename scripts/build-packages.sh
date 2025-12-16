#!/bin/bash
#
# Build RPM and DEB packages for OpenVox WebUI
#
set -e

VERSION="${VERSION:-0.1.0}"
RELEASE="${RELEASE:-1}"
BUILD_DIR="${BUILD_DIR:-$(pwd)/build}"
SOURCE_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

usage() {
    echo "Usage: $0 [OPTIONS] [TARGETS]"
    echo ""
    echo "Options:"
    echo "  -h, --help     Show this help message"
    echo "  -v VERSION     Set package version (default: $VERSION)"
    echo "  -r RELEASE     Set release number (default: $RELEASE)"
    echo "  -o OUTPUT_DIR  Set output directory (default: ./build)"
    echo ""
    echo "Targets:"
    echo "  all            Build all packages (default)"
    echo "  rpm            Build RPM package only"
    echo "  deb            Build DEB package only"
    echo "  source         Create source tarball only"
    echo ""
    echo "Examples:"
    echo "  $0                    # Build all packages"
    echo "  $0 rpm                # Build RPM only"
    echo "  $0 -v 0.2.0 deb       # Build DEB with version 0.2.0"
}

check_dependencies() {
    local missing=()

    # Common dependencies
    command -v cargo >/dev/null 2>&1 || missing+=("cargo (Rust)")
    command -v npm >/dev/null 2>&1 || missing+=("npm (Node.js)")

    # RPM dependencies
    if [[ "$BUILD_RPM" == "true" ]]; then
        command -v rpmbuild >/dev/null 2>&1 || missing+=("rpmbuild (rpm-build)")
    fi

    # DEB dependencies
    if [[ "$BUILD_DEB" == "true" ]]; then
        command -v dpkg-buildpackage >/dev/null 2>&1 || missing+=("dpkg-buildpackage (dpkg-dev)")
        command -v debhelper >/dev/null 2>&1 || missing+=("debhelper")
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing dependencies:"
        for dep in "${missing[@]}"; do
            echo "  - $dep"
        done
        exit 1
    fi
}

create_source_tarball() {
    log_info "Creating source tarball..."

    local tarball_name="openvox-webui-${VERSION}"
    local tarball_dir="${BUILD_DIR}/source"

    mkdir -p "$tarball_dir"

    # Create a clean copy of the source
    git archive --format=tar --prefix="${tarball_name}/" HEAD | \
        gzip > "${tarball_dir}/${tarball_name}.tar.gz"

    log_info "Source tarball created: ${tarball_dir}/${tarball_name}.tar.gz"
}

build_rpm() {
    log_info "Building RPM package..."

    local rpm_build_dir="${BUILD_DIR}/rpm"

    # Create RPM build directory structure
    mkdir -p "${rpm_build_dir}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

    # Copy source tarball
    cp "${BUILD_DIR}/source/openvox-webui-${VERSION}.tar.gz" \
       "${rpm_build_dir}/SOURCES/"

    # Copy spec file
    cp "${SOURCE_DIR}/packaging/rpm/openvox-webui.spec" \
       "${rpm_build_dir}/SPECS/"

    # Update version in spec file
    sed -i "s/%define version.*/%define version ${VERSION}/" \
        "${rpm_build_dir}/SPECS/openvox-webui.spec"
    sed -i "s/%define release.*/%define release ${RELEASE}%{?dist}/" \
        "${rpm_build_dir}/SPECS/openvox-webui.spec"

    # Build RPM
    rpmbuild --define "_topdir ${rpm_build_dir}" \
             -ba "${rpm_build_dir}/SPECS/openvox-webui.spec"

    # Copy output
    mkdir -p "${BUILD_DIR}/output"
    cp "${rpm_build_dir}"/RPMS/*/*.rpm "${BUILD_DIR}/output/" 2>/dev/null || true
    cp "${rpm_build_dir}"/SRPMS/*.rpm "${BUILD_DIR}/output/" 2>/dev/null || true

    log_info "RPM packages built successfully"
}

build_deb() {
    log_info "Building DEB package..."

    local deb_build_dir="${BUILD_DIR}/deb"
    local source_name="openvox-webui-${VERSION}"

    mkdir -p "$deb_build_dir"

    # Extract source
    cd "$deb_build_dir"
    tar xzf "${BUILD_DIR}/source/${source_name}.tar.gz"

    # Copy debian directory
    cp -r "${SOURCE_DIR}/packaging/deb/debian" "${source_name}/"

    # Update version in changelog
    sed -i "s/openvox-webui ([^)]*)/openvox-webui (${VERSION}-${RELEASE})/" \
        "${source_name}/debian/changelog"

    # Build package
    cd "${source_name}"
    dpkg-buildpackage -us -uc -b

    # Copy output
    mkdir -p "${BUILD_DIR}/output"
    cp "${deb_build_dir}"/*.deb "${BUILD_DIR}/output/" 2>/dev/null || true

    log_info "DEB packages built successfully"
}

# Parse arguments
BUILD_RPM=false
BUILD_DEB=false
BUILD_SOURCE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            usage
            exit 0
            ;;
        -v)
            VERSION="$2"
            shift 2
            ;;
        -r)
            RELEASE="$2"
            shift 2
            ;;
        -o)
            BUILD_DIR="$2"
            shift 2
            ;;
        all)
            BUILD_RPM=true
            BUILD_DEB=true
            BUILD_SOURCE=true
            shift
            ;;
        rpm)
            BUILD_RPM=true
            BUILD_SOURCE=true
            shift
            ;;
        deb)
            BUILD_DEB=true
            BUILD_SOURCE=true
            shift
            ;;
        source)
            BUILD_SOURCE=true
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Default to building all
if [[ "$BUILD_RPM" == "false" && "$BUILD_DEB" == "false" && "$BUILD_SOURCE" == "false" ]]; then
    BUILD_RPM=true
    BUILD_DEB=true
    BUILD_SOURCE=true
fi

log_info "OpenVox WebUI Package Builder"
log_info "Version: ${VERSION}-${RELEASE}"
log_info "Build directory: ${BUILD_DIR}"

# Check dependencies
check_dependencies

# Create build directory
mkdir -p "$BUILD_DIR"

# Build steps
if [[ "$BUILD_SOURCE" == "true" ]]; then
    create_source_tarball
fi

if [[ "$BUILD_RPM" == "true" ]]; then
    build_rpm
fi

if [[ "$BUILD_DEB" == "true" ]]; then
    build_deb
fi

log_info "Build complete! Packages available in: ${BUILD_DIR}/output/"
ls -la "${BUILD_DIR}/output/" 2>/dev/null || true
