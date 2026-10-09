#!/bin/sh
# Install ctx (Linux / macOS).
#   curl -fsSL https://raw.githubusercontent.com/Idrisanu/ctx/main/install.sh | sh
set -eu

REPO="Idrisanu/ctx"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"

uname_s="$(uname -s)"
uname_m="$(uname -m)"
case "$uname_s" in
  Linux) os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  *) echo "unsupported OS: $uname_s" >&2; exit 1 ;;
esac
case "$uname_m" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64)
    if [ "$os" = "apple-darwin" ]; then arch="aarch64"; else arch="x86_64"; fi
    ;;
  *) echo "unsupported arch: $uname_m" >&2; exit 1 ;;
esac
target="${arch}-${os}"

version="${CTX_VERSION:-latest}"
if [ "$version" = "latest" ]; then
  url="https://github.com/$REPO/releases/latest/download/ctx-$target.tar.gz"
else
  url="https://github.com/$REPO/releases/download/$version/ctx-$target.tar.gz"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "downloading $url"
curl -fsSL "$url" -o "$tmp/ctx.tar.gz"
echo "verifying checksum"
sums_url="${url%/*}/SHA256SUMS"
curl -fsSL "$sums_url" -o "$tmp/SHA256SUMS"
archive="ctx-$target.tar.gz"
want="$(grep " $archive\$" "$tmp/SHA256SUMS" | awk '{print $1}')"
if [ -z "$want" ]; then
  echo "error: $archive not listed in SHA256SUMS" >&2; exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  echo "$want  $tmp/ctx.tar.gz" | sha256sum -c - >/dev/null
elif command -v shasum >/dev/null 2>&1; then
  echo "$want  $tmp/ctx.tar.gz" | shasum -a 256 -c - >/dev/null
else
  echo "error: need sha256sum or shasum to verify the download" >&2; exit 1
fi
echo "checksum ok"
tar xzf "$tmp/ctx.tar.gz" -C "$tmp"
mkdir -p "$BIN_DIR"
mv "$tmp/ctx" "$BIN_DIR/ctx"
chmod +x "$BIN_DIR/ctx"
echo "installed ctx to $BIN_DIR/ctx"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "NOTE: $BIN_DIR is not on your PATH. Add: export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac
"$BIN_DIR/ctx" --version
