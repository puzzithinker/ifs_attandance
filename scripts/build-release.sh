#!/usr/bin/env bash
# Build the single shipping binary (SQLite statically linked).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> cargo test --workspace"
cargo test --workspace

echo "==> cargo build --release -p ifs-app"
cargo build --release -p ifs-app

OUT_DIR="$ROOT/dist"
mkdir -p "$OUT_DIR"

if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "$(uname -s)" == CYGWIN* || -f "$ROOT/target/release/ifs_attendance.exe" ]]; then
  SRC="$ROOT/target/release/ifs_attendance.exe"
  DEST="$OUT_DIR/ifs_attendance.exe"
else
  SRC="$ROOT/target/release/ifs_attendance"
  DEST="$OUT_DIR/ifs_attendance"
fi

cp -f "$SRC" "$DEST"
# Optional friendlier Windows name next to ship folder
if [[ -f "$ROOT/target/release/ifs_attendance.exe" ]]; then
  cp -f "$ROOT/target/release/ifs_attendance.exe" "$OUT_DIR/IFS_AML_Attendance.exe"
fi

ls -lh "$DEST"
file "$DEST" || true
echo ""
echo "Ship only this file to seminar laptops."
echo "On first run it creates agent.db + station.toml next to the exe."
echo "Done: $DEST"
