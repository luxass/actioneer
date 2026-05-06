#!/bin/sh

set -eu

repo="luxass/actioneer"
install_dir="${ACTIONEER_INSTALL_DIR:-${HOME}/.local/bin}"
version="${ACTIONEER_VERSION:-latest}"

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "error: missing required command: $1" >&2
        exit 1
    fi
}

detect_os() {
    case "$(uname -s)" in
        Darwin) echo "macos" ;;
        Linux) echo "linux-musl" ;;
        *)
            echo "error: unsupported operating system: $(uname -s)" >&2
            exit 1
            ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        arm64|aarch64) echo "aarch64" ;;
        x86_64|amd64) echo "x86_64" ;;
        *)
            echo "error: unsupported architecture: $(uname -m)" >&2
            exit 1
            ;;
    esac
}

download() {
    url="$1"
    output="$2"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$output"
        return
    fi

    if command -v wget >/dev/null 2>&1; then
        wget -qO "$output" "$url"
        return
    fi

    echo "error: either curl or wget is required" >&2
    exit 1
}

download_to_stdout() {
    url="$1"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url"
        return
    fi

    if command -v wget >/dev/null 2>&1; then
        wget -qO- "$url"
        return
    fi

    echo "error: either curl or wget is required" >&2
    exit 1
}

install_binary() {
    src="$1"
    dst="$2"

    mkdir -p "$dst"

    if command -v install >/dev/null 2>&1; then
        install "$src" "$dst/actioneer"
    else
        cp "$src" "$dst/actioneer"
        chmod +x "$dst/actioneer"
    fi
}

resolve_latest_version() {
    api_url="https://api.github.com/repos/${repo}/releases/latest"
    download_to_stdout "$api_url" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\(v[^"]*\)".*/\1/p' | head -n 1
}

need_cmd uname
need_cmd tar
need_cmd mktemp

os="$(detect_os)"
arch="$(detect_arch)"
target="${arch}-${os}"

if [ "$version" = "latest" ]; then
    resolved_version="$(resolve_latest_version)"

    if [ -z "$resolved_version" ]; then
        echo "error: failed to resolve the latest release version" >&2
        exit 1
    fi

    version="${resolved_version#v}"
fi

archive_url="https://github.com/${repo}/releases/download/v${version}/actioneer-${version}-${target}.tar.gz"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

archive_path="${tmp_dir}/actioneer.tar.gz"
extract_dir="${tmp_dir}/extract"
mkdir -p "$extract_dir"

download "$archive_url" "$archive_path"

tar -xzf "$archive_path" -C "$extract_dir"

binary_path="$(find "$extract_dir" -type f -name actioneer | head -n 1)"

if [ -z "$binary_path" ]; then
    echo "error: actioneer binary not found in release archive" >&2
    exit 1
fi

install_binary "$binary_path" "$install_dir"

echo "installed actioneer to ${install_dir}/actioneer"
