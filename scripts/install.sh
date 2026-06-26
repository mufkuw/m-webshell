#!/bin/bash
#
# m-webshell install script
# https://github.com/mufkuw/m-webshell
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/mufkuw/m-webshell/main/scripts/install.sh | bash
#
set -e

REPO="mufkuw/m-webshell"
GITHUB_API="https://api.github.com/repos/${REPO}/releases/latest"

echo ""
echo "  m-webshell — the keep for your web terminal"
echo ""

# --- detect arch ---
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  DEB_ARCH="amd64";  TARGET="x86_64-unknown-linux-gnu" ;;
    aarch64) DEB_ARCH="arm64";   TARGET="aarch64-unknown-linux-gnu" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# --- find latest release .deb URL ---
echo "  -> fetching latest release..."
DOWNLOAD_URL=$(curl -fsSL "$GITHUB_API" | grep -o "browser_download_url.*${DEB_ARCH}.deb" | head -1 | cut -d'"' -f2)

if [ -z "$DOWNLOAD_URL" ]; then
    echo "  No .deb package found for ${DEB_ARCH}."
    echo "  Build from source instead:"
    echo "    cargo install --git https://github.com/${REPO} --locked"
    exit 1
fi

# --- download and install ---
TMPFILE=$(mktemp /tmp/m-webshell.XXXXXX.deb)
trap 'rm -f "$TMPFILE"' EXIT

echo "  -> downloading ${DEB_ARCH} package..."
curl -fsSL -o "$TMPFILE" "$DOWNLOAD_URL"

echo "  -> installing..."
sudo dpkg -i "$TMPFILE" || sudo apt-get install -f -y

echo ""
echo "  m-webshell installed."
echo ""
echo "  Next steps:"
echo "    1. Edit the UID in the service file:"
echo "       sudo nano /etc/systemd/system/m-webshell.service"
echo "    2. Start the service:"
echo "       sudo systemctl daemon-reload"
echo "       sudo systemctl start m-webshell"
echo "    3. Scan the QR code:"
echo "       m-webshell show-secret"
echo ""
echo "  Then navigate to: https://your-domain/system/manage-<TOTP>/"
echo ""