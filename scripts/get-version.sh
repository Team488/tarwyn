# MIT License https://github.com/rojo-rbx/rokit/blob/main/LICENSE.txt
#!/usr/bin/env bash

set -euo pipefail

CLI_MANIFEST=$(cargo read-manifest --manifest-path core/Cargo.toml)
CLI_VERSION=$(echo $CLI_MANIFEST | jq -r .version)

echo $CLI_VERSION
