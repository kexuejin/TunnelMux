#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "usage: $0 <source-dir> <destination-dir> <extension> [extension ...]" >&2
  exit 64
fi

source_dir=$1
shift
destination_dir=$1
shift
extensions=("$@")

if [[ ! -d "$source_dir" ]]; then
  echo "source directory not found: $source_dir" >&2
  exit 66
fi

mkdir -p "$destination_dir"
found=0

for extension in "${extensions[@]}"; do
  while IFS= read -r -d '' asset; do
    found=1
    destination_path="$destination_dir/$(basename "$asset")"
    cp "$asset" "$destination_path"
    printf 'collected %s\n' "$destination_path"
  done < <(find "$source_dir" -type f -name "*.${extension}" -print0 | sort -z)
done

if [[ $found -eq 0 ]]; then
  echo "no GUI bundle artifacts found in $source_dir for extensions: ${extensions[*]}" >&2
  exit 1
fi
