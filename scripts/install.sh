#!/usr/bin/env zsh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec "$ROOT/../scripts/install-plugin.sh" "$ROOT/.." flattery flattery-xtask Flattery Flattery Flattery vst3,clap
