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
    x86_64)  DEB_ARCH="amd64" ;;
    aarch64) DEB_ARCH="arm64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# --- stop existing service if running ---
if systemctl is-active m-webshell &>/dev/null; then
    echo "  -> stopping m-webshell service..."
    sudo systemctl stop m-webshell
fi

# --- install ttyd if not present ---
if ! command -v ttyd &>/dev/null; then
    echo "  -> ttyd not found, installing..."
    TTYD_ARCH="$ARCH"
    case "$ARCH" in
        x86_64)  TTYD_ARCH="x86_64" ;;
        aarch64) TTYD_ARCH="aarch64" ;;
    esac
    TTYD_URL="https://github.com/tsl0922/ttyd/releases/latest/download/ttyd.${TTYD_ARCH}"
    echo "  -> downloading ttyd..."
    sudo curl -fsSL -o /usr/local/bin/ttyd "$TTYD_URL"
    sudo chmod 755 /usr/local/bin/ttyd
    echo "  -> ttyd installed."
else
    echo "  -> ttyd already installed."
fi

# --- find latest release .deb URL ---
echo "  -> fetching latest release..."
DOWNLOAD_URL=$(curl -fsSL "$GITHUB_API" | grep -o '"browser_download_url": *"[^"]*'"$DEB_ARCH"'.deb"' | sed 's/.*"browser_download_url": *"\([^"]*\)".*/\1/' | head -1)

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
sudo dpkg -i "$TMPFILE" 2>/dev/null || sudo apt-get install -f -y

# --- create runtime directory ---
sudo mkdir -p /run/m-webshell
sudo chmod 0750 /run/m-webshell

# --- generate TOTP secret if none exists ---
if [ ! -f /etc/m-webshell/m-webshell.totp ]; then
    echo "  -> generating TOTP secret..."
    /usr/local/bin/m-webshell generate-secret
fi

# --- start or restart service ---
if [ -f /etc/systemd/system/m-webshell.service ]; then
    sudo systemctl daemon-reload
    sudo systemctl start m-webshell
    echo ""
    echo "  m-webshell upgraded and started."
else
    echo ""
    echo "  m-webshell installed."
    echo ""
    echo "  Next steps:"
    echo "    1. Edit the UID in the service file:"
    echo "       sudo nano /etc/systemd/system/m-webshell.service"
    echo "    2. Enable and start the service:"
    echo "       sudo systemctl daemon-reload"
    echo "       sudo systemctl enable --now m-webshell"
    echo "    3. Scan the QR code:"
    echo "       m-webshell show-secret"
fi

echo ""
echo "  Navigate to: https://your-domain/system/manage-<TOTP>/"
echo ""