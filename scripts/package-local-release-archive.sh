#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
release_dir="${TUNNELMUX_RELEASE_DIR:-$repo_root/target/release}"
dist_dir="${1:-${TUNNELMUX_DIST_DIR:-/tmp/tunnelmux-local-release}}"
version="${TUNNELMUX_VERSION:-$(python3 - <<'PY'
import json
from pathlib import Path
config = json.loads(Path('crates/tunnelmux-gui/tauri.conf.json').read_text())
print(config['version'])
PY
)}"
target="${TUNNELMUX_TARGET:-$(rustc -vV | awk '/host:/ {print $2}')}"
package_name="tunnelmux-${version}-${target}"
package_dir="$dist_dir/$package_name"
archive_path="$dist_dir/$package_name.tar.gz"
checksum_path="$dist_dir/SHA256SUMS"

required_binaries=(tunnelmuxd tunnelmux-cli tunnelmux-gui)
required_docs=(README.md README.zh-CN.md LICENSE CHANGELOG.md)

mkdir -p "$dist_dir"
rm -rf "$package_dir" "$archive_path" "$checksum_path"
mkdir -p "$package_dir"

for binary in "${required_binaries[@]}"; do
  source_path="$release_dir/$binary"
  if [[ ! -f "$source_path" ]]; then
    echo "missing release binary: $source_path" >&2
    exit 1
  fi
  cp "$source_path" "$package_dir/"
done

for doc in "${required_docs[@]}"; do
  source_path="$repo_root/$doc"
  if [[ ! -f "$source_path" ]]; then
    echo "missing release document: $source_path" >&2
    exit 1
  fi
  cp "$source_path" "$package_dir/"
done

tar -C "$dist_dir" -czf "$archive_path" "$package_name"

if command -v sha256sum >/dev/null 2>&1; then
  (
    cd "$dist_dir"
    sha256sum "$package_name.tar.gz" > "$(basename "$checksum_path")"
  )
else
  (
    cd "$dist_dir"
    shasum -a 256 "$package_name.tar.gz" > "$(basename "$checksum_path")"
  )
fi

archive_listing="$(tar -tzf "$archive_path")"
for binary in "${required_binaries[@]}"; do
  if ! grep -q "^${package_name}/${binary}$" <<<"$archive_listing"; then
    echo "archive missing binary: ${binary}" >&2
    exit 1
  fi
done

for doc in "${required_docs[@]}"; do
  if ! grep -q "^${package_name}/${doc}$" <<<"$archive_listing"; then
    echo "archive missing document: ${doc}" >&2
    exit 1
  fi
done

printf 'created %s\n' "$archive_path"
printf 'created %s\n' "$checksum_path"
