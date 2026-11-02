#!/bin/bash
set -euo pipefail
CONTENTS="$(cd "$(dirname "$0")/.." && pwd)"
export LIGHTCRAFT_CONFIG_DIR="$HOME/Library/Application Support/LightCraft EdZ Lab"
export LIGHTCRAFT_LIBRARY="$LIGHTCRAFT_CONFIG_DIR/Library"
export CRAFT_VERSIONS_FILE="$CONTENTS/Resources/versions.json"
mkdir -p "$LIGHTCRAFT_CONFIG_DIR" "$LIGHTCRAFT_LIBRARY"
exec "$CONTENTS/MacOS/LightCraftCore" --library "$LIGHTCRAFT_LIBRARY" "$@"
