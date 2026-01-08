#!/bin/bash
set -e

VERSION=${1:-"dev"}
DIST_DIR="darkfi-win64-${VERSION}"
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

# Copy binary
cp target/x86_64-pc-windows-gnu/release/darkfi-app.exe "${DIST_DIR}/"

# Copy assets
cp -r assets "${DIST_DIR}/"

# Create README
cat > "${DIST_DIR}/README.txt" <<EOF
DarkFi App v${VERSION}

To run: double-click darkfi-app.exe
EOF

# Create ZIP
zip -r "${DIST_DIR}.zip" "${DIST_DIR}"
echo "Created: ${DIST_DIR}.zip"

# Create NSIS installer
makensis -DVERSION="${VERSION}" -DDIST_DIR="${DIST_DIR}" \
    -DOUTPUT_FILE="darkfi-win64-${VERSION}-installer.exe" \
    release/win/installer.nsi

echo "Created: darkfi-win64-${VERSION}-installer.exe"
rm -rf "${DIST_DIR}"
