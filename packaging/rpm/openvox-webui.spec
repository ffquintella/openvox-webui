%define name openvox-webui
%define version 0.1.0
%define release 1%{?dist}
%define _builddir %{_topdir}/BUILD/%{name}-%{version}

Name:           %{name}
Version:        %{version}
Release:        %{release}
Summary:        Web UI for OpenVox infrastructure management

License:        Apache-2.0
URL:            https://github.com/openvoxproject/openvox-webui
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.70
BuildRequires:  cargo
BuildRequires:  nodejs >= 18
BuildRequires:  npm
BuildRequires:  openssl-devel
BuildRequires:  sqlite-devel

Requires:       openssl
Requires:       sqlite

%description
OpenVox WebUI provides a modern web interface for managing and monitoring
OpenVox infrastructure. It supports PuppetDB integration, node classification,
Facter-based management, and role-based access control.

%prep
%setup -q

%build
# Build frontend
cd frontend
npm ci
npm run build
cd ..

# Build backend
cargo build --release

%install
rm -rf %{buildroot}

# Create directories
install -d %{buildroot}%{_bindir}
install -d %{buildroot}%{_sysconfdir}/openvox-webui
install -d %{buildroot}%{_datadir}/openvox-webui/static
install -d %{buildroot}%{_localstatedir}/lib/openvox-webui
install -d %{buildroot}%{_unitdir}

# Install binary
install -m 755 target/release/openvox-webui %{buildroot}%{_bindir}/openvox-webui

# Install frontend assets
cp -r frontend/dist/* %{buildroot}%{_datadir}/openvox-webui/static/

# Install configuration
install -m 644 config/config.example.yaml %{buildroot}%{_sysconfdir}/openvox-webui/config.yaml

# Install systemd unit
install -m 644 packaging/systemd/openvox-webui.service %{buildroot}%{_unitdir}/openvox-webui.service

%pre
# Create openvox-webui user if it doesn't exist
getent group openvox-webui >/dev/null || groupadd -r openvox-webui
getent passwd openvox-webui >/dev/null || \
    useradd -r -g openvox-webui -d %{_localstatedir}/lib/openvox-webui \
    -s /sbin/nologin -c "OpenVox WebUI service account" openvox-webui

%post
%systemd_post openvox-webui.service

%preun
%systemd_preun openvox-webui.service

%postun
%systemd_postun_with_restart openvox-webui.service

%files
%license LICENSE
%doc README.md CHANGELOG.md
%{_bindir}/openvox-webui
%dir %{_sysconfdir}/openvox-webui
%config(noreplace) %{_sysconfdir}/openvox-webui/config.yaml
%{_datadir}/openvox-webui
%attr(750,openvox-webui,openvox-webui) %{_localstatedir}/lib/openvox-webui
%{_unitdir}/openvox-webui.service

%changelog
* Mon Dec 16 2024 OpenVox Team <team@openvox.io> - 0.1.0-1
- Initial release
- Basic backend with Axum framework
- React frontend with TypeScript
- RBAC foundation with system roles
- PuppetDB integration support
- Node classification system
