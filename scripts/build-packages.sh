#!/bin/bash
#
# Build RPM and DEB packages for OpenVox WebUI
#
# This script creates native Linux packages (RPM and DEB) for distribution.
# It uses Docker by default to ensure consistent amd64/x86_64 builds regardless
# of the host architecture (supports building on Apple Silicon, ARM servers, etc.)
#
# When using Docker (default), the full build process happens inside the container:
#   - Frontend is built with npm
#   - Backend is compiled with Rust/Cargo
#   - Package is created with rpmbuild/dpkg-buildpackage
#
# Usage:
#   ./build-packages.sh [OPTIONS] [TARGETS]
#
# Examples:
#   ./build-packages.sh                    # Build all packages using Docker (default)
#   ./build-packages.sh rpm                # Build RPM only (using Docker)
#   ./build-packages.sh -v 1.0.0 deb       # Build DEB with explicit version
#   ./build-packages.sh --no-docker rpm    # Build RPM natively (requires local tools)
#
set -e

SOURCE_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Get version from Cargo.toml if not specified
get_version_from_cargo() {
    grep '^version' "${SOURCE_DIR}/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/'
}

VERSION="${VERSION:-$(get_version_from_cargo)}"
RELEASE="${RELEASE:-1}"
BUILD_DIR="${BUILD_DIR:-$(pwd)/build}"
# Always build for amd64/x86_64 architecture (Docker handles cross-compilation)
ARCH="${ARCH:-x86_64}"
# Docker is enabled by default for package builds
USE_DOCKER="${USE_DOCKER:-true}"

# Force amd64/x86_64 architecture for package builds
DEB_ARCH="amd64"
RPM_ARCH="x86_64"
DOCKER_PLATFORM="linux/amd64"
RUST_TARGET="x86_64-unknown-linux-gnu"

# Cache directories for incremental builds
CACHE_DIR="${CACHE_DIR:-${HOME}/.cache/openvox-webui-build}"
CARGO_CACHE_DIR="${CACHE_DIR}/cargo"
NPM_CACHE_DIR="${CACHE_DIR}/npm"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
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

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

usage() {
    echo "Usage: $0 [OPTIONS] [TARGETS]"
    echo ""
    echo "Build native Linux packages for OpenVox WebUI."
    echo ""
    echo "Options:"
    echo "  -h, --help       Show this help message"
    echo "  -v VERSION       Set package version (default: from Cargo.toml)"
    echo "  -r RELEASE       Set release number (default: 1)"
    echo "  -o OUTPUT_DIR    Set output directory (default: ./build)"
    echo "  -a ARCH          Set target architecture (default: $(uname -m))"
    echo "  --docker         Use Docker for building (default: enabled)"
    echo "  --no-docker      Disable Docker and build natively (requires local tools)"
    echo "  --clean          Clean build directory before building"
    echo "  --clean-cache    Clean the incremental build cache (Cargo + npm)"
    echo "  --sign           Sign packages after building (requires GPG key)"
    echo ""
    echo "Targets:"
    echo "  all              Build all packages (default)"
    echo "  rpm              Build RPM package only"
    echo "  deb              Build DEB package only"
    echo "  source           Create source tarball only"
    echo "  binary           Build binary tarball only (no package manager)"
    echo ""
    echo "Examples:"
    echo "  $0                       # Build all packages (Docker builds everything)"
    echo "  $0 rpm                   # Build RPM only"
    echo "  $0 -v 1.0.0 deb          # Build DEB with version 1.0.0"
    echo "  $0 --no-docker rpm       # Build RPM natively (requires local Rust/Node.js)"
    echo "  $0 --clean --sign all    # Clean build, then build and sign all"
    echo "  $0 --clean-cache rpm     # Clean cache and rebuild RPM from scratch"
    echo ""
    echo "Environment Variables:"
    echo "  VERSION          Package version (overrides -v)"
    echo "  RELEASE          Release number (overrides -r)"
    echo "  BUILD_DIR        Build directory (overrides -o)"
    echo "  CACHE_DIR        Cache directory for incremental builds (default: ~/.cache/openvox-webui-build)"
    echo "  GPG_KEY_ID       GPG key ID for signing packages"
    echo ""
    echo "Note: Docker builds use incremental compilation with cached Cargo and npm directories."
    echo "      First build may take 10-15 minutes; subsequent builds are much faster."
    echo "      Cache location: \${CACHE_DIR:-~/.cache/openvox-webui-build}"
}

check_dependencies() {
    local missing=()

    # Common dependencies
    command -v cargo >/dev/null 2>&1 || missing+=("cargo (Rust)")
    command -v npm >/dev/null 2>&1 || missing+=("npm (Node.js)")
    command -v git >/dev/null 2>&1 || missing+=("git")

    # RPM dependencies
    if [[ "$BUILD_RPM" == "true" ]] && [[ "$USE_DOCKER" == "false" ]]; then
        command -v rpmbuild >/dev/null 2>&1 || missing+=("rpmbuild (rpm-build)")
    fi

    # DEB dependencies
    if [[ "$BUILD_DEB" == "true" ]] && [[ "$USE_DOCKER" == "false" ]]; then
        command -v dpkg-buildpackage >/dev/null 2>&1 || missing+=("dpkg-buildpackage (dpkg-dev)")
    fi

    # Docker dependency
    if [[ "$USE_DOCKER" == "true" ]]; then
        command -v docker >/dev/null 2>&1 || missing+=("docker")
    fi

    # Signing dependency
    if [[ "$SIGN_PACKAGES" == "true" ]]; then
        command -v gpg >/dev/null 2>&1 || missing+=("gpg")
        if [[ "$BUILD_RPM" == "true" ]]; then
            command -v rpm >/dev/null 2>&1 || missing+=("rpm (for signing)")
        fi
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing dependencies:"
        for dep in "${missing[@]}"; do
            echo "  - $dep"
        done
        exit 1
    fi
}

clean_build_dir() {
    log_info "Cleaning build directory: ${BUILD_DIR}"
    rm -rf "${BUILD_DIR}"
    mkdir -p "${BUILD_DIR}"
}

clean_cache_dir() {
    log_info "Cleaning incremental build cache: ${CACHE_DIR}"
    rm -rf "${CACHE_DIR}"
    log_info "Cache cleaned. Next build will be a full rebuild."
}

build_frontend() {
    log_step "Building frontend..."
    cd "${SOURCE_DIR}/frontend"

    # Install dependencies
    npm ci --prefer-offline

    # Build production bundle
    npm run build

    cd "${SOURCE_DIR}"
    log_info "Frontend built successfully"
}

build_backend() {
    log_step "Building backend..."
    cd "${SOURCE_DIR}"

    local target_flag=""
    if [[ -n "${CARGO_TARGET}" ]]; then
        target_flag="--target ${CARGO_TARGET}"
        log_info "Cross-compiling for target: ${CARGO_TARGET}"
    fi

    # Build release binary
    cargo build --release ${target_flag}

    log_info "Backend built successfully"
}

create_source_tarball() {
    log_step "Creating source tarball..."

    local tarball_name="openvox-webui-${VERSION}"
    local tarball_dir="${BUILD_DIR}/source"

    mkdir -p "$tarball_dir"

    # Create a clean copy of the source
    git archive --format=tar --prefix="${tarball_name}/" HEAD | \
        gzip > "${tarball_dir}/${tarball_name}.tar.gz"

    # Also create a checksum
    cd "$tarball_dir"
    sha256sum "${tarball_name}.tar.gz" > "${tarball_name}.tar.gz.sha256"
    cd "${SOURCE_DIR}"

    log_info "Source tarball created: ${tarball_dir}/${tarball_name}.tar.gz"
}

build_binary_tarball() {
    log_step "Building binary tarball..."

    local tarball_name="openvox-webui-${VERSION}-linux-${ARCH}"
    local staging_dir="${BUILD_DIR}/binary/${tarball_name}"

    mkdir -p "$staging_dir"/{bin,config,static,systemd}

    # Build if not already built
    if [[ ! -f "${SOURCE_DIR}/target/release/openvox-webui" ]]; then
        build_frontend
        build_backend
    fi

    # Copy binary
    cp "${SOURCE_DIR}/target/release/openvox-webui" "$staging_dir/bin/"
    chmod 755 "$staging_dir/bin/openvox-webui"

    # Copy frontend assets
    cp -r "${SOURCE_DIR}/frontend/dist/"* "$staging_dir/static/"

    # Copy configuration
    cp "${SOURCE_DIR}/config/config.example.yaml" "$staging_dir/config/config.yaml"

    # Copy systemd unit
    cp "${SOURCE_DIR}/packaging/systemd/openvox-webui.service" "$staging_dir/systemd/"

    # Create install script
    cat > "$staging_dir/install.sh" << 'INSTALL_EOF'
#!/bin/bash
set -e

INSTALL_PREFIX="${INSTALL_PREFIX:-/usr/local}"
DATA_DIR="${DATA_DIR:-/var/lib/openvox-webui}"
CONFIG_DIR="${CONFIG_DIR:-/etc/openvox-webui}"
LOG_DIR="${LOG_DIR:-/var/log/openvox/webui}"

echo "Installing OpenVox WebUI..."

# Create user
if ! getent group openvox-webui >/dev/null; then
    groupadd -r openvox-webui
fi
if ! getent passwd openvox-webui >/dev/null; then
    useradd -r -g openvox-webui -d "$DATA_DIR" -s /sbin/nologin \
        -c "OpenVox WebUI service account" openvox-webui
fi

# Create directories
mkdir -p "$INSTALL_PREFIX/bin"
mkdir -p "$INSTALL_PREFIX/share/openvox-webui/static"
mkdir -p "$CONFIG_DIR"
mkdir -p "$DATA_DIR"
mkdir -p "$LOG_DIR"

# Install files
cp bin/openvox-webui "$INSTALL_PREFIX/bin/"
cp -r static/* "$INSTALL_PREFIX/share/openvox-webui/static/"
if [[ ! -f "$CONFIG_DIR/config.yaml" ]]; then
    cp config/config.yaml "$CONFIG_DIR/"
fi

# Install systemd unit
if [[ -d /etc/systemd/system ]]; then
    cp systemd/openvox-webui.service /etc/systemd/system/
    sed -i "s|/usr/bin|$INSTALL_PREFIX/bin|g" /etc/systemd/system/openvox-webui.service
    systemctl daemon-reload
fi

# Set permissions
chown -R openvox-webui:openvox-webui "$DATA_DIR" "$LOG_DIR"
chown root:openvox-webui "$CONFIG_DIR/config.yaml"
chmod 640 "$CONFIG_DIR/config.yaml"

echo "Installation complete!"
echo ""
echo "Next steps:"
echo "  1. Edit $CONFIG_DIR/config.yaml"
echo "  2. Start service: systemctl start openvox-webui"
echo "  3. Enable on boot: systemctl enable openvox-webui"
INSTALL_EOF
    chmod 755 "$staging_dir/install.sh"

    # Create README
    cat > "$staging_dir/README.txt" << README_EOF
OpenVox WebUI ${VERSION}
========================

This is a pre-built binary distribution of OpenVox WebUI.

Quick Install:
  sudo ./install.sh

Manual Install:
  1. Copy bin/openvox-webui to /usr/local/bin/
  2. Copy static/* to /usr/local/share/openvox-webui/static/
  3. Copy config/config.yaml to /etc/openvox-webui/
  4. Copy systemd/openvox-webui.service to /etc/systemd/system/
  5. Create user: useradd -r -s /sbin/nologin openvox-webui
  6. Create directories: /var/lib/openvox-webui, /var/log/openvox/webui

For more information, see: https://github.com/ffquintella/openvox-webui
README_EOF

    # Create tarball
    cd "${BUILD_DIR}/binary"
    tar czf "${tarball_name}.tar.gz" "${tarball_name}"
    sha256sum "${tarball_name}.tar.gz" > "${tarball_name}.tar.gz.sha256"

    mkdir -p "${BUILD_DIR}/output"
    mv "${tarball_name}.tar.gz" "${tarball_name}.tar.gz.sha256" "${BUILD_DIR}/output/"

    cd "${SOURCE_DIR}"
    log_info "Binary tarball created: ${BUILD_DIR}/output/${tarball_name}.tar.gz"
}

build_rpm() {
    log_step "Building RPM package..."

    local rpm_build_dir="${BUILD_DIR}/rpm"

    # Create RPM build directory structure
    mkdir -p "${rpm_build_dir}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

    # Copy source tarball
    cp "${BUILD_DIR}/source/openvox-webui-${VERSION}.tar.gz" \
       "${rpm_build_dir}/SOURCES/"

    # Copy spec file and update version
    sed -e "s/%define version.*/%define version ${VERSION}/" \
        -e "s/%define release.*/%define release ${RELEASE}%{?dist}/" \
        "${SOURCE_DIR}/packaging/rpm/openvox-webui.spec" \
        > "${rpm_build_dir}/SPECS/openvox-webui.spec"

    # Build RPM
    rpmbuild --define "_topdir ${rpm_build_dir}" \
             -ba "${rpm_build_dir}/SPECS/openvox-webui.spec"

    # Copy output
    mkdir -p "${BUILD_DIR}/output"
    find "${rpm_build_dir}/RPMS" -name "*.rpm" -exec cp {} "${BUILD_DIR}/output/" \;
    find "${rpm_build_dir}/SRPMS" -name "*.rpm" -exec cp {} "${BUILD_DIR}/output/" \;

    # Create checksums
    cd "${BUILD_DIR}/output"
    for rpm in *.rpm; do
        [[ -f "$rpm" ]] && sha256sum "$rpm" > "${rpm}.sha256"
    done
    cd "${SOURCE_DIR}"

    log_info "RPM packages built successfully"
}

build_deb() {
    log_step "Building DEB package..."

    local deb_build_dir="${BUILD_DIR}/deb"
    local source_name="openvox-webui-${VERSION}"

    mkdir -p "$deb_build_dir"

    # Extract source
    cd "$deb_build_dir"
    tar xzf "${BUILD_DIR}/source/${source_name}.tar.gz"

    # Copy debian directory
    cp -r "${SOURCE_DIR}/packaging/deb/debian" "${source_name}/"

    # Update version in changelog
    local date_str=$(date -R)
    cat > "${source_name}/debian/changelog" << CHANGELOG_EOF
openvox-webui (${VERSION}-${RELEASE}) unstable; urgency=medium

  * Release version ${VERSION}

 -- OpenVox Team <team@openvox.io>  ${date_str}
CHANGELOG_EOF

    # Build package
    cd "${source_name}"
    dpkg-buildpackage -us -uc -b

    # Copy output
    mkdir -p "${BUILD_DIR}/output"
    find "${deb_build_dir}" -maxdepth 1 -name "*.deb" -exec cp {} "${BUILD_DIR}/output/" \;
    find "${deb_build_dir}" -maxdepth 1 -name "*.changes" -exec cp {} "${BUILD_DIR}/output/" \;

    # Create checksums
    cd "${BUILD_DIR}/output"
    for deb in *.deb; do
        [[ -f "$deb" ]] && sha256sum "$deb" > "${deb}.sha256"
    done
    cd "${SOURCE_DIR}"

    log_info "DEB packages built successfully"
}

build_rpm_docker() {
    log_step "Building RPM package using Docker (platform: ${DOCKER_PLATFORM})..."

    # Create cache directories if they don't exist
    mkdir -p "${CARGO_CACHE_DIR}/registry"
    mkdir -p "${CARGO_CACHE_DIR}/git"
    mkdir -p "${CARGO_CACHE_DIR}/target-rpm"
    mkdir -p "${NPM_CACHE_DIR}"

    # Create Dockerfile for RPM build (includes Rust and Node.js for compilation)
    local dockerfile="${BUILD_DIR}/Dockerfile.rpm"
    cat > "$dockerfile" << 'DOCKERFILE_EOF'
FROM --platform=linux/amd64 rockylinux:9

RUN dnf install -y \
    rpm-build \
    rpmdevtools \
    gcc \
    make \
    openssl-devel \
    sqlite-devel \
    git \
    && dnf clean all

# Install Node.js 20 from NodeSource
RUN curl -fsSL https://rpm.nodesource.com/setup_20.x | bash - \
    && dnf install -y nodejs \
    && dnf clean all

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build
DOCKERFILE_EOF

    # Build image with platform specification
    log_info "Building Docker image for RPM (this may take a while on first run)..."
    docker build --platform "${DOCKER_PLATFORM}" -t openvox-webui-rpm-builder -f "$dockerfile" "${BUILD_DIR}"

    # Run build and packaging in Docker container with cached volumes
    log_info "Running RPM build in Docker container (with incremental build cache)..."
    log_info "Cache directory: ${CACHE_DIR}"
    docker run --rm \
        --platform "${DOCKER_PLATFORM}" \
        -v "${SOURCE_DIR}:/source:ro" \
        -v "${BUILD_DIR}:/output" \
        -v "${CARGO_CACHE_DIR}/registry:/root/.cargo/registry" \
        -v "${CARGO_CACHE_DIR}/git:/root/.cargo/git" \
        -v "${CARGO_CACHE_DIR}/target-rpm:/build/target" \
        -v "${NPM_CACHE_DIR}:/root/.npm" \
        -e VERSION="${VERSION}" \
        -e RELEASE="${RELEASE}" \
        openvox-webui-rpm-builder \
        /bin/bash -c '
            set -e

            # Work in /build which has the cached target directory mounted
            cd /build

            # Copy source excluding target/ and node_modules/ to save space
            # But keep the structure so cargo can use the cached target/
            rm -rf src frontend/src packaging config migrations tests puppet scripts Cargo.toml Cargo.lock 2>/dev/null || true
            cp -r /source/src /source/Cargo.toml /source/Cargo.lock /source/packaging /source/config /source/migrations /source/tests /source/puppet /source/scripts .
            cp -r /source/frontend .
            rm -rf frontend/node_modules frontend/dist 2>/dev/null || true

            # Build frontend
            echo "Building frontend..."
            cd frontend
            npm ci --cache /root/.npm --prefer-offline 2>/dev/null || npm ci --cache /root/.npm
            npm run build
            cd ..

            # Build backend (uses cached target/ directory)
            echo "Building backend (incremental)..."
            cargo build --release

            # Create source tarball for rpmbuild
            mkdir -p /tmp/openvox-webui-${VERSION}
            cp -r src frontend/dist packaging config migrations puppet Cargo.toml Cargo.lock /tmp/openvox-webui-${VERSION}/
            # Copy documentation files
            cp /source/README.md /source/CHANGELOG.md /source/ROADMAP.md /source/LICENSE /tmp/openvox-webui-${VERSION}/ 2>/dev/null || true
            mkdir -p /tmp/openvox-webui-${VERSION}/target/release
            cp target/release/openvox-webui /tmp/openvox-webui-${VERSION}/target/release/
            cp target/release/run-scheduled-reports /tmp/openvox-webui-${VERSION}/target/release/
            mkdir -p /tmp/openvox-webui-${VERSION}/frontend
            cp -r frontend/dist /tmp/openvox-webui-${VERSION}/frontend/

            cd /tmp
            tar czf openvox-webui-${VERSION}.tar.gz openvox-webui-${VERSION}

            # Setup rpmbuild directories
            mkdir -p /root/rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
            cp openvox-webui-${VERSION}.tar.gz /root/rpmbuild/SOURCES/

            # Update and copy spec file
            sed -e "s/%define version.*/%define version ${VERSION}/" \
                -e "s/%define release.*/%define release ${RELEASE}%{?dist}/" \
                /tmp/openvox-webui-${VERSION}/packaging/rpm/openvox-webui.spec \
                > /root/rpmbuild/SPECS/openvox-webui.spec

            # Patch spec file to skip the build phase since we already compiled above
            # Replace %build section content with no-op (code already built in Docker)
            sed -i "/%build/,/%install/{/%build/!{/%install/!d}}" /root/rpmbuild/SPECS/openvox-webui.spec
            sed -i "s/^%build$/%build\n# Pre-built in Docker - skipping recompilation/" /root/rpmbuild/SPECS/openvox-webui.spec

            # Build RPM (--nodeps skips BuildRequires check since we compiled manually)
            echo "Building RPM package..."
            rpmbuild -bb --nodeps /root/rpmbuild/SPECS/openvox-webui.spec

            # Copy output
            mkdir -p /output/output
            find /root/rpmbuild/RPMS -name "*.rpm" -exec cp {} /output/output/ \;

            # Create checksums
            cd /output/output
            for rpm in *.rpm; do
                [ -f "$rpm" ] && sha256sum "$rpm" > "${rpm}.sha256"
            done

            echo "RPM build complete!"
        '

    log_info "RPM packages built successfully using Docker"
}

build_deb_docker() {
    log_step "Building DEB package using Docker (platform: ${DOCKER_PLATFORM})..."

    # Create cache directories if they don't exist
    mkdir -p "${CARGO_CACHE_DIR}/registry"
    mkdir -p "${CARGO_CACHE_DIR}/git"
    mkdir -p "${CARGO_CACHE_DIR}/target-deb"
    mkdir -p "${NPM_CACHE_DIR}"

    # Create Dockerfile for DEB build (includes Rust and Node.js for compilation)
    local dockerfile="${BUILD_DIR}/Dockerfile.deb"
    cat > "$dockerfile" << 'DOCKERFILE_EOF'
FROM --platform=linux/amd64 debian:bookworm

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    build-essential \
    debhelper \
    devscripts \
    dpkg-dev \
    fakeroot \
    libssl-dev \
    libsqlite3-dev \
    pkg-config \
    curl \
    ca-certificates \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 20 from NodeSource
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build
DOCKERFILE_EOF

    # Build image with platform specification
    log_info "Building Docker image for DEB (this may take a while on first run)..."
    docker build --platform "${DOCKER_PLATFORM}" -t openvox-webui-deb-builder -f "$dockerfile" "${BUILD_DIR}"

    # Run build and packaging in Docker container with cached volumes
    log_info "Running DEB build in Docker container (with incremental build cache)..."
    log_info "Cache directory: ${CACHE_DIR}"
    docker run --rm \
        --platform "${DOCKER_PLATFORM}" \
        -v "${SOURCE_DIR}:/source:ro" \
        -v "${BUILD_DIR}:/output" \
        -v "${CARGO_CACHE_DIR}/registry:/root/.cargo/registry" \
        -v "${CARGO_CACHE_DIR}/git:/root/.cargo/git" \
        -v "${CARGO_CACHE_DIR}/target-deb:/build/target" \
        -v "${NPM_CACHE_DIR}:/root/.npm" \
        -e VERSION="${VERSION}" \
        -e RELEASE="${RELEASE}" \
        openvox-webui-deb-builder \
        /bin/bash -c '
            set -e

            # Work in /build which has the cached target directory mounted
            cd /build

            # Copy source excluding target/ and node_modules/ to save space
            # But keep the structure so cargo can use the cached target/
            rm -rf src frontend/src packaging config migrations tests puppet scripts Cargo.toml Cargo.lock debian 2>/dev/null || true
            cp -r /source/src /source/Cargo.toml /source/Cargo.lock /source/packaging /source/config /source/migrations /source/tests /source/puppet /source/scripts .
            cp -r /source/frontend .
            rm -rf frontend/node_modules frontend/dist 2>/dev/null || true

            # Build frontend
            echo "Building frontend..."
            cd frontend
            npm ci --cache /root/.npm --prefer-offline 2>/dev/null || npm ci --cache /root/.npm
            npm run build
            cd ..

            # Build backend (uses cached target/ directory)
            echo "Building backend (incremental)..."
            cargo build --release

            # Now prepare for dpkg-buildpackage in /tmp
            mkdir -p /tmp/openvox-webui-${VERSION}
            cp -r src frontend packaging config migrations puppet Cargo.toml Cargo.lock /tmp/openvox-webui-${VERSION}/
            # Copy documentation files
            cp /source/README.md /source/CHANGELOG.md /source/ROADMAP.md /source/LICENSE /tmp/openvox-webui-${VERSION}/ 2>/dev/null || true
            mkdir -p /tmp/openvox-webui-${VERSION}/target/release
            cp target/release/openvox-webui /tmp/openvox-webui-${VERSION}/target/release/
            cp target/release/run-scheduled-reports /tmp/openvox-webui-${VERSION}/target/release/

            cd /tmp/openvox-webui-${VERSION}

            # Copy debian directory
            cp -r packaging/deb/debian .

            # Update changelog with version
            cat > debian/changelog << CHANGELOG_EOF
openvox-webui (${VERSION}-${RELEASE}) unstable; urgency=medium

  * Release version ${VERSION}

 -- OpenVox Team <team@openvox.io>  $(date -R)
CHANGELOG_EOF

            # Patch debian/rules to skip build and clean phases (code already built in Docker)
            sed -i "s/override_dh_auto_build:/override_dh_auto_build:\n\t# Pre-built in Docker - skipping recompilation\n\ttrue\n\noriginal_dh_auto_build_disabled:/" debian/rules
            # Also prevent cargo clean from removing our pre-built binaries
            sed -i "s/override_dh_auto_clean:/override_dh_auto_clean:\n\t# Skip clean to preserve pre-built binaries\n\ttrue\n\noriginal_dh_auto_clean_disabled:/" debian/rules

            # Build package (dependencies already satisfied since we compiled above)
            echo "Building DEB package..."
            dpkg-buildpackage -us -uc -b -d

            # Copy output
            mkdir -p /output/output
            find /tmp -maxdepth 1 -name "*.deb" -exec cp {} /output/output/ \;
            find /tmp -maxdepth 1 -name "*.changes" -exec cp {} /output/output/ \;

            # Create checksums
            cd /output/output
            for deb in *.deb; do
                [ -f "$deb" ] && sha256sum "$deb" > "${deb}.sha256"
            done

            echo "DEB build complete!"
        '

    log_info "DEB packages built successfully using Docker"
}

sign_packages() {
    log_step "Signing packages..."

    local key_id="${GPG_KEY_ID:-}"

    if [[ -z "$key_id" ]]; then
        log_warn "GPG_KEY_ID not set, skipping package signing"
        return
    fi

    cd "${BUILD_DIR}/output"

    # Sign RPM packages
    for rpm in *.rpm; do
        if [[ -f "$rpm" ]]; then
            log_info "Signing $rpm"
            rpm --addsign --define "_gpg_name ${key_id}" "$rpm"
        fi
    done

    # Sign DEB packages
    for deb in *.deb; do
        if [[ -f "$deb" ]]; then
            log_info "Signing $deb"
            dpkg-sig -k "$key_id" --sign builder "$deb"
        fi
    done

    # Create detached signatures for source tarballs
    for tarball in *.tar.gz; do
        if [[ -f "$tarball" ]]; then
            log_info "Creating signature for $tarball"
            gpg --armor --detach-sign -u "$key_id" "$tarball"
        fi
    done

    cd "${SOURCE_DIR}"
    log_info "Package signing complete"
}

# Parse arguments
BUILD_RPM=false
BUILD_DEB=false
BUILD_SOURCE=false
BUILD_BINARY=false
CLEAN_BUILD=false
CLEAN_CACHE=false
SIGN_PACKAGES=false

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
        -a)
            ARCH="$2"
            shift 2
            ;;
        --docker)
            USE_DOCKER=true
            shift
            ;;
        --no-docker)
            USE_DOCKER=false
            shift
            ;;
        --clean)
            CLEAN_BUILD=true
            shift
            ;;
        --clean-cache)
            CLEAN_CACHE=true
            shift
            ;;
        --sign)
            SIGN_PACKAGES=true
            shift
            ;;
        all)
            BUILD_RPM=true
            BUILD_DEB=true
            BUILD_SOURCE=true
            BUILD_BINARY=true
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
        binary)
            BUILD_BINARY=true
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
if [[ "$BUILD_RPM" == "false" && "$BUILD_DEB" == "false" && \
      "$BUILD_SOURCE" == "false" && "$BUILD_BINARY" == "false" ]]; then
    BUILD_RPM=true
    BUILD_DEB=true
    BUILD_SOURCE=true
    BUILD_BINARY=true
fi

echo ""
echo "========================================"
log_info "OpenVox WebUI Package Builder"
echo "========================================"
log_info "Version:         ${VERSION}-${RELEASE}"
log_info "Architecture:    ${ARCH} (${DEB_ARCH}/${RPM_ARCH})"
log_info "Build directory: ${BUILD_DIR}"
log_info "Use Docker:      ${USE_DOCKER}"
if [[ "$USE_DOCKER" == "true" ]]; then
log_info "Docker platform: ${DOCKER_PLATFORM}"
log_info "Cache directory: ${CACHE_DIR}"
fi
echo "========================================"
echo ""

# Check dependencies
check_dependencies

# Clean cache if requested
if [[ "$CLEAN_CACHE" == "true" ]]; then
    clean_cache_dir
fi

# Clean build directory if requested
if [[ "$CLEAN_BUILD" == "true" ]]; then
    clean_build_dir
fi

# Create build directory
mkdir -p "$BUILD_DIR"

# Build steps
if [[ "$BUILD_SOURCE" == "true" ]]; then
    create_source_tarball
fi

if [[ "$BUILD_BINARY" == "true" ]]; then
    build_binary_tarball
fi

if [[ "$BUILD_RPM" == "true" ]]; then
    if [[ "$USE_DOCKER" == "true" ]]; then
        build_rpm_docker
    else
        build_rpm
    fi
fi

if [[ "$BUILD_DEB" == "true" ]]; then
    if [[ "$USE_DOCKER" == "true" ]]; then
        build_deb_docker
    else
        build_deb
    fi
fi

if [[ "$SIGN_PACKAGES" == "true" ]]; then
    sign_packages
fi

echo ""
echo "========================================"
log_info "Build complete!"
echo "========================================"
echo ""
log_info "Packages available in: ${BUILD_DIR}/output/"
echo ""
if [[ -d "${BUILD_DIR}/output" ]]; then
    ls -lh "${BUILD_DIR}/output/" 2>/dev/null || true
fi
