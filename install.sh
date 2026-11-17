#!/bin/sh
# Installs clnr from the latest release into ~/.local/bin.
#
#   curl -fsSL https://raw.githubusercontent.com/nikhil-pandey/clnr/main/install.sh | sh
set -eu

case "$(uname -s)" in
Darwin) os=apple-darwin ;;
Linux) os=unknown-linux-musl ;;
*) echo "clnr: unsupported OS $(uname -s)" >&2 && exit 1 ;;
esac

case "$(uname -m)" in
x86_64 | amd64) arch=x86_64 ;;
arm64 | aarch64) arch=aarch64 ;;
*) echo "clnr: unsupported architecture $(uname -m)" >&2 && exit 1 ;;
esac

target="$arch-$os"
bin_dir="$HOME/.local/bin"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fsSL "https://github.com/nikhil-pandey/clnr/releases/latest/download/clnr-$target.tar.gz" | tar -xz -C "$tmp"
mkdir -p "$bin_dir"
install -m 755 "$tmp/clnr-$target/clnr" "$bin_dir/"
echo "installed clnr to $bin_dir"
case ":$PATH:" in
*":$bin_dir:"*) ;;
*) echo "add $bin_dir to your PATH" ;;
esac
