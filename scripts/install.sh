#!/bin/sh
# Installs myna-layout on Debian-based systems (Pop!_OS, Ubuntu).
#
#   ./scripts/install.sh            download the latest release .deb and install it
#   ./scripts/install.sh --source   build the .deb from this checkout and install it
set -eu

REPO="Joopbarbosa/myna-layout"

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "error: '$1' is required but was not found" >&2
        exit 1
    }
}

need sudo
need apt

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if [ "${1:-}" = "--source" ]; then
    need cargo
    cargo install cargo-deb --locked
    cargo deb --output "$work/myna-layout.deb"
else
    need curl
    url=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep -o '"browser_download_url": *"[^"]*\.deb"' \
        | head -n 1 | cut -d '"' -f 4)
    if [ -z "$url" ]; then
        echo "error: no release package found; try './scripts/install.sh --source'" >&2
        exit 1
    fi
    echo "Downloading $url"
    curl -fsSL "$url" -o "$work/myna-layout.deb"
fi

sudo apt install -y "$work/myna-layout.deb"
echo "Done. The indicator starts with your graphical session; log out and in, or run:"
echo "  systemctl --user start myna-layout.service"
