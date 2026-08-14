#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Type-check all TypeScript files and report results.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "▶ Running TypeScript type check..."
npx tsc --noEmit --pretty

echo ""
echo "✅ Type check passed"
