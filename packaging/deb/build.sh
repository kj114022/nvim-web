#!/bin/bash
# Build Debian package for nvim-web
set -e

VERSION="0.1.0"
PACKAGE_NAME="nvim-web"
ARCH="amd64"
BUILD_DIR="build"
PACKAGE_DIR="${BUILD_DIR}/${PACKAGE_NAME}_${VERSION}_${ARCH}"

# Clean previous builds
rm -rf "$BUILD_DIR"
mkdir -p "${PACKAGE_DIR}/DEBIAN"
mkdir -p "${PACKAGE_DIR}/usr/bin"
mkdir -p "${PACKAGE_DIR}/usr/share/doc/${PACKAGE_NAME}"
mkdir -p "${PACKAGE_DIR}/usr/share/man/man1"
mkdir -p "${PACKAGE_DIR}/etc/${PACKAGE_NAME}"

# Build the binary
echo "Building nvim-web..."
cd ../..
cargo build --release -p nvim-web-host
cd packaging/deb

# Copy files
cp ../../target/release/nvim-web-host "${PACKAGE_DIR}/usr/bin/nvim-web"
cp ../../config.example.toml "${PACKAGE_DIR}/etc/${PACKAGE_NAME}/"
cp ../../README.md "${PACKAGE_DIR}/usr/share/doc/${PACKAGE_NAME}/"
cp ../../LICENSE "${PACKAGE_DIR}/usr/share/doc/${PACKAGE_NAME}/copyright"

# Create control file
cat > "${PACKAGE_DIR}/DEBIAN/control" << EOF
Package: ${PACKAGE_NAME}
Version: ${VERSION}
Section: editors
Priority: optional
Architecture: ${ARCH}
Depends: neovim (>= 0.9.0), ca-certificates
Recommends: ripgrep, git
Maintainer: nvim-web contributors <nvim-web@example.com>
Homepage: https://github.com/kj114022/nvim-web
Description: Neovim in the Browser
 nvim-web runs the actual Neovim binary on a host machine and renders
 its output in a WebAssembly-based UI over WebSocket/WebTransport.
 .
 Features:
  - Full Neovim compatibility (your config, plugins, LSP work)
  - Real-time collaborative editing (CRDTs)
  - WebTransport/QUIC support for lower latency
  - OIDC/BeyondCorp authentication
  - Kubernetes pod-per-session scaling
EOF

# Create conffiles list
echo "/etc/${PACKAGE_NAME}/config.example.toml" > "${PACKAGE_DIR}/DEBIAN/conffiles"

# Create postinst script
cat > "${PACKAGE_DIR}/DEBIAN/postinst" << 'EOF'
#!/bin/bash
set -e
echo "nvim-web installed successfully!"
echo "Run 'nvim-web' to start the server."
echo "Configuration: /etc/nvim-web/config.example.toml"
EOF
chmod 755 "${PACKAGE_DIR}/DEBIAN/postinst"

# Set permissions
chmod 755 "${PACKAGE_DIR}/usr/bin/nvim-web"
chmod 644 "${PACKAGE_DIR}/etc/${PACKAGE_NAME}/config.example.toml"

# Build package
dpkg-deb --build "${PACKAGE_DIR}"

echo ""
echo "Package built: ${PACKAGE_DIR}.deb"
