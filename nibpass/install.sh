#!/usr/bin/env bash
set -euo pipefail

repo="${NIBPASS_REPO:-nibtools/nibtools}"
version="${NIBPASS_VERSION:-latest}"
prefix="${NIBPASS_PREFIX:-${HOME}/.local}"
bin_dir="${NIBPASS_BIN_DIR:-${prefix}/bin}"

usage() {
  cat <<'USAGE'
Install NibPass from GitHub Releases.

Usage:
  install.sh [--version nibpass-v0.1.0] [--prefix ~/.local] [--bin-dir ~/.local/bin]

Environment:
  NIBPASS_REPO       GitHub owner/repo, default nibtools/nibtools
  NIBPASS_VERSION    Release tag, default latest
  NIBPASS_PREFIX     Install prefix, default ~/.local
  NIBPASS_BIN_DIR    Binary directory, default $NIBPASS_PREFIX/bin
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      version="${2:?missing value for --version}"
      shift 2
      ;;
    --prefix)
      prefix="${2:?missing value for --prefix}"
      bin_dir="${NIBPASS_BIN_DIR:-${prefix}/bin}"
      shift 2
      ;;
    --bin-dir)
      bin_dir="${2:?missing value for --bin-dir}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$(uname -s)" in
  Darwin)
    platform="macos-universal"
    ;;
  Linux)
    case "$(uname -m)" in
      x86_64|amd64)
        platform="linux-x86_64"
        ;;
      *)
        echo "unsupported Linux architecture: $(uname -m)" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "unsupported OS: $(uname -s)" >&2
    exit 1
    ;;
esac

archive="nibpass-${platform}.tar.gz"
if [[ "$version" == "latest" ]]; then
  version="$(
    curl -fsSL "https://api.github.com/repos/${repo}/releases?per_page=100" |
      sed -n 's/.*"tag_name": "\(nibpass-v[^"]*\)".*/\1/p' |
      head -n 1
  )"
  if [[ -z "$version" ]]; then
    echo "could not find a nibpass-v* release in ${repo}" >&2
    exit 1
  fi
fi
url="https://github.com/${repo}/releases/download/${version}/${archive}"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

echo "Downloading ${url}"
curl -fsSL "$url" -o "${tmp_dir}/${archive}"
tar -xzf "${tmp_dir}/${archive}" -C "$tmp_dir"

mkdir -p "$bin_dir"
install -m 755 "${tmp_dir}/nibpass-${platform}/bin/nibpass" "${bin_dir}/nibpass"

echo "Installed nibpass to ${bin_dir}/nibpass"
echo "Run this once to add shell setup:"
echo "  ${bin_dir}/nibpass install-shell"
