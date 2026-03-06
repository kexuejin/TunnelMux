#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <output-path>" >&2
  exit 64
fi

output_path=$1
required_vars=(
  WINDOWS_TRUSTED_SIGNING_ENDPOINT
  WINDOWS_TRUSTED_SIGNING_ACCOUNT
  WINDOWS_TRUSTED_SIGNING_PROFILE
)
missing=()

for var_name in "${required_vars[@]}"; do
  if [[ -z "${!var_name:-}" ]]; then
    missing+=("$var_name")
  fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
  printf 'missing required environment variables: %s\n' "${missing[*]}" >&2
  exit 65
fi

mkdir -p "$(dirname "$output_path")"

OUTPUT_PATH="$output_path" python - <<'PY'
import json
import os
from pathlib import Path

output_path = Path(os.environ["OUTPUT_PATH"])
endpoint = os.environ["WINDOWS_TRUSTED_SIGNING_ENDPOINT"]
account = os.environ["WINDOWS_TRUSTED_SIGNING_ACCOUNT"]
profile = os.environ["WINDOWS_TRUSTED_SIGNING_PROFILE"]

config = {
    "bundle": {
        "windows": {
            "signCommand": (
                f"trusted-signing-cli -e {endpoint} -a {account} -c {profile} "
                "-d TunnelMux %1"
            )
        }
    }
}
output_path.write_text(json.dumps(config, indent=2) + "\n")
PY
