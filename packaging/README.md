# Packaging Scripts for nvim-web

Package manager files for all major platforms.

## Supported Platforms

| Platform | Manager | Directory | Install Command |
|----------|---------|-----------|-----------------|
| macOS | Homebrew | `nvim-web.rb` | `brew install --build-from-source nvim-web.rb` |
| Ubuntu/Debian | apt/dpkg | `deb/` | `sudo dpkg -i nvim-web_*.deb` |
| Ubuntu | Snap | `snap/` | `sudo snap install nvim-web` |
| Fedora/RHEL | dnf/rpm | `rpm/` | `sudo dnf install nvim-web-*.rpm` |
| Arch Linux | pacman/AUR | `arch/` | `makepkg -si` |
| NixOS | nix | `nix/` | `nix build .#nvim-web` |
| Windows | Scoop | `windows/nvim-web.json` | `scoop install nvim-web` |
| Windows | WinGet | `windows/winget.yaml` | `winget install nvim-web` |
| Linux | Flatpak | `flatpak/` | `flatpak install com.github.kj114022.nvim-web` |

## Building Packages

### Ubuntu/Debian (.deb)

```bash
cd deb
./build.sh
# Output: build/nvim-web_0.1.0_amd64.deb
sudo dpkg -i build/nvim-web_0.1.0_amd64.deb
```

### Ubuntu (Snap)

```bash
cd snap
snapcraft
# Output: nvim-web_0.1.0_amd64.snap
sudo snap install nvim-web_0.1.0_amd64.snap --dangerous
```

### Fedora/RHEL (.rpm)

```bash
rpmbuild -ba rpm/nvim-web.spec
# Output: ~/rpmbuild/RPMS/x86_64/nvim-web-0.1.0-1.x86_64.rpm
```

### Arch Linux (PKGBUILD)

```bash
cd arch
makepkg -si
```

### NixOS

```bash
cd nix
nix build .#nvim-web
# Or add to your system flake
```

### Flatpak

```bash
cd flatpak
flatpak-builder --user --install build com.github.kj114022.nvim-web.yaml
```

## Prerequisites

| Format | Build Requirements |
|--------|-------------------|
| deb | `dpkg-dev`, `fakeroot` |
| rpm | `rpmbuild`, `rpmdevtools` |
| arch | `base-devel` |
| nix | Nix package manager |
| snap | `snapcraft` |
| flatpak | `flatpak-builder` |

## CI/CD Integration

The GitHub Actions workflow builds all packages on release:

```yaml
# .github/workflows/release.yml
- name: Build packages
  run: |
    cd packaging/deb && ./build.sh
    cd ../snap && snapcraft
    # ...
```
