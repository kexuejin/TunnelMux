#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <tauri-project-dir> <target-triple>" >&2
  exit 64
fi

project_dir=$1
target_triple=$2

case "$target_triple" in
  *windows*)
    extension=".exe"
    ;;
  *)
    extension=""
    ;;
esac

source_path="target/${target_triple}/release/tunnelmuxd${extension}"
destination_dir="${project_dir}/bin"
destination_path="${destination_dir}/tunnelmuxd-${target_triple}${extension}"

if [[ ! -f "$source_path" ]]; then
  echo "daemon binary not found: $source_path" >&2
  exit 66
fi

mkdir -p "$destination_dir"
cp "$source_path" "$destination_path"
chmod +x "$destination_path" 2>/dev/null || true
printf 'staged GUI daemon binary %s\n' "$destination_path"
