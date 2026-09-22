#!/usr/bin/env zsh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKSPACE="$(cd "$ROOT/.." && pwd)"
cd "$ROOT"

# Workspace target only. Ignore injected CARGO_TARGET_DIR (agent sandbox caches).
export CARGO_TARGET_DIR="$WORKSPACE/target"

echo "Building release VST3 and CLAP bundles..."
cargo xtask bundle flattery --release

BUNDLED_DIR="$CARGO_TARGET_DIR/bundled"

VST3_DIR="$HOME/Library/Audio/Plug-Ins/VST3"
CLAP_DIR="$HOME/Library/Audio/Plug-Ins/CLAP"

mkdir -p "$VST3_DIR" "$CLAP_DIR"

echo "Installing from $BUNDLED_DIR to $VST3_DIR and $CLAP_DIR..."
rm -rf "$VST3_DIR/Flattery.vst3" "$CLAP_DIR/Flattery.clap"
cp -R "$BUNDLED_DIR/Flattery.vst3" "$VST3_DIR/"
cp -R "$BUNDLED_DIR/Flattery.clap" "$CLAP_DIR/"

codesign --force --deep -s - "$VST3_DIR/Flattery.vst3"
codesign --force --deep -s - "$CLAP_DIR/Flattery.clap"

echo "Installed successfully:"
echo "  - $VST3_DIR/Flattery.vst3"
echo "  - $CLAP_DIR/Flattery.clap"
