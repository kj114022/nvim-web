Name:           nvim-web
Version:        0.9.9
Release:        1%{?dist}
Summary:        Neovim in the Browser

License:        MIT
URL:            https://github.com/kj114022/nvim-web
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  openssl-devel

Requires:       neovim >= 0.9.0
Requires:       ca-certificates
Recommends:     ripgrep
Recommends:     git

%description
nvim-web runs the actual Neovim binary on a host machine and renders
its output in a WebAssembly-based UI over WebSocket/WebTransport.

Features:
- Full Neovim compatibility (your config, plugins, LSP work)
- Real-time collaborative editing (CRDTs)
- WebTransport/QUIC support for lower latency
- OIDC/BeyondCorp authentication
- Kubernetes pod-per-session scaling

%prep
%autosetup

%build
cargo build --release -p nvim-web-host

%install
rm -rf $RPM_BUILD_ROOT
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_sysconfdir}/nvim-web
mkdir -p %{buildroot}%{_docdir}/%{name}

install -m 755 target/release/nvim-web-host %{buildroot}%{_bindir}/nvim-web
install -m 644 config.example.toml %{buildroot}%{_sysconfdir}/nvim-web/
install -m 644 README.md %{buildroot}%{_docdir}/%{name}/
install -m 644 LICENSE %{buildroot}%{_docdir}/%{name}/

%files
%license LICENSE
%doc README.md
%{_bindir}/nvim-web
%config(noreplace) %{_sysconfdir}/nvim-web/config.example.toml

%changelog
* Tue Jan 14 2026 nvim-web contributors <nvim-web@example.com> - 0.9.9-1
- Initial package
- WebSocket and WebTransport support
- CRDT-based collaborative editing
- OIDC authentication
- Kubernetes scaling
