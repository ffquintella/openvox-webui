%define name openvox-webui
%define version 0.9.0
%define release 1%{?dist}
%define _builddir %{_topdir}/BUILD/%{name}-%{version}

Name:           %{name}
Version:        %{version}
Release:        %{release}
Summary:        Web UI for OpenVox infrastructure management

License:        Apache-2.0
URL:            https://github.com/ffquintella/openvox-webui
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  nodejs >= 20
BuildRequires:  npm
BuildRequires:  openssl-devel
BuildRequires:  sqlite-devel
BuildRequires:  gcc
BuildRequires:  make

Requires:       openssl
Requires:       sqlite
Requires(pre):  shadow-utils
Requires(post): systemd
Requires(preun): systemd
Requires(postun): systemd

%description
OpenVox WebUI provides a modern web interface for managing and monitoring
OpenVox infrastructure. Features include:

 * PuppetDB integration with graphical dashboards and real-time monitoring
 * Puppet CA management for certificate operations
 * Node classification using dynamic rule-based groups
 * Facter template management for external fact generation
 * Role-based access control (RBAC) with fine-grained permissions
 * Multi-tenancy support with organization isolation
 * API key management for automation
 * Comprehensive audit logging
 * Light and dark theme support

%prep
%setup -q

%build
# Build frontend
cd frontend
npm ci --prefer-offline
npm run build
cd ..

# Build backend
cargo build --release

%install
rm -rf %{buildroot}

# Create directories
install -d %{buildroot}%{_bindir}
install -d %{buildroot}%{_sysconfdir}/openvox-webui
install -d %{buildroot}%{_sysconfdir}/openvox-webui/ssl
install -d %{buildroot}%{_datadir}/openvox-webui/static
install -d %{buildroot}%{_localstatedir}/lib/openvox-webui
install -d %{buildroot}%{_localstatedir}/log/openvox/webui
install -d %{buildroot}%{_unitdir}

# Install binary
install -m 755 target/release/openvox-webui %{buildroot}%{_bindir}/openvox-webui

# Install scheduled report runner binary if it exists
if [ -f target/release/run-scheduled-reports ]; then
    install -m 755 target/release/run-scheduled-reports %{buildroot}%{_bindir}/openvox-webui-scheduled-reports
fi

# Install frontend assets
cp -r frontend/dist/* %{buildroot}%{_datadir}/openvox-webui/static/

# Install configuration
install -m 640 config/config.example.yaml %{buildroot}%{_sysconfdir}/openvox-webui/config.yaml

# Install systemd units
install -m 644 packaging/systemd/openvox-webui.service %{buildroot}%{_unitdir}/openvox-webui.service

%pre
# Create openvox-webui group if it doesn't exist
getent group openvox-webui >/dev/null || groupadd -r openvox-webui

# Create openvox-webui user if it doesn't exist
getent passwd openvox-webui >/dev/null || \
    useradd -r -g openvox-webui -d %{_localstatedir}/lib/openvox-webui \
    -s /sbin/nologin -c "OpenVox WebUI service account" openvox-webui

%post
%systemd_post openvox-webui.service

# Set proper ownership on first install
chown -R openvox-webui:openvox-webui %{_localstatedir}/lib/openvox-webui
chown -R openvox-webui:openvox-webui %{_localstatedir}/log/openvox/webui
chown root:openvox-webui %{_sysconfdir}/openvox-webui/config.yaml

echo ""
echo "OpenVox WebUI has been installed!"
echo ""
echo "Next steps:"
echo "  1. Edit /etc/openvox-webui/config.yaml"
echo "  2. Generate or install TLS certificates in /etc/openvox-webui/ssl/"
echo "  3. Start the service: systemctl start openvox-webui"
echo "  4. Enable on boot: systemctl enable openvox-webui"
echo ""
echo "Default admin credentials (change immediately!):"
echo "  Username: admin"
echo "  Password: admin"
echo ""

%preun
%systemd_preun openvox-webui.service

%postun
%systemd_postun_with_restart openvox-webui.service

# On complete removal (not upgrade)
if [ $1 -eq 0 ]; then
    # Remove log files but preserve data directory
    rm -rf %{_localstatedir}/log/openvox/webui
fi

%files
%license LICENSE
%doc README.md CHANGELOG.md ROADMAP.md
%{_bindir}/openvox-webui
%dir %{_sysconfdir}/openvox-webui
%dir %attr(750,root,openvox-webui) %{_sysconfdir}/openvox-webui/ssl
%config(noreplace) %attr(640,root,openvox-webui) %{_sysconfdir}/openvox-webui/config.yaml
%{_datadir}/openvox-webui
%attr(750,openvox-webui,openvox-webui) %{_localstatedir}/lib/openvox-webui
%attr(750,openvox-webui,openvox-webui) %{_localstatedir}/log/openvox/webui
%{_unitdir}/openvox-webui.service

%changelog
* Wed Dec 18 2024 OpenVox Team <team@openvox.io> - 0.9.0-1
- Security hardening: rate limiting, security headers, TLS 1.3 default
- Multi-tenancy with organization isolation
- API key management for automation
- Comprehensive audit logging
- Light/dark theme support
- Performance optimizations (N+1 query fixes, lazy loading)

* Mon Dec 16 2024 OpenVox Team <team@openvox.io> - 0.8.0-1
- Puppet CA management (sign, reject, revoke, renew)
- RBAC improvements with database-backed roles
- Node classification engine
- Dashboard visualizations

* Mon Dec 02 2024 OpenVox Team <team@openvox.io> - 0.1.0-1
- Initial release
- Basic backend with Axum framework
- React frontend with TypeScript
- RBAC foundation with system roles
- PuppetDB integration support
