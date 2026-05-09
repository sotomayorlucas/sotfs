#!/usr/bin/env bash
# examples/persistent_mount.sh — end-to-end demo of persistent sotFS.
#
# Creates a redb-backed volume, mounts it, writes a directory tree,
# unmounts, re-mounts, verifies persistence, then exports to Graph
# Hunter format. Idempotent: safe to re-run.

set -euo pipefail

BIN_DIR="${BIN_DIR:-./target/release}"
DB="${DB:-/tmp/sotfs-demo.redb}"
MNT="${MNT:-/tmp/sotfs-demo-mnt}"

step() { printf "\n\033[1;34m== %s ==\033[0m\n" "$*"; }

cleanup() {
  if mountpoint -q "$MNT" 2>/dev/null; then
    fusermount3 -u "$MNT" 2>/dev/null || fusermount -u "$MNT" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# ─── 0. Build (if needed) ────────────────────────────────────────────
if [ ! -x "$BIN_DIR/sotfs-fuse" ]; then
  step "Building release binaries"
  cargo build --release --bin sotfs-fuse --bin sotfsctl --bin sotfs-export-hunter
fi

# ─── 1. Reset state ──────────────────────────────────────────────────
step "Reset $DB and $MNT"
cleanup
rm -f "$DB"
mkdir -p "$MNT"

# ─── 2. mkfs ─────────────────────────────────────────────────────────
step "sotfsctl mkfs $DB"
"$BIN_DIR/sotfsctl" mkfs "$DB"

# ─── 3. First mount + writes ─────────────────────────────────────────
step "Mount + populate"
"$BIN_DIR/sotfs-fuse" "$MNT" --db "$DB" &
sleep 1
mkdir -p "$MNT/projects/alpha" "$MNT/projects/beta"
echo "alpha source" > "$MNT/projects/alpha/main.rs"
echo "beta config"  > "$MNT/projects/beta/cfg.toml"
ln -s ../alpha/main.rs "$MNT/projects/beta/link-to-alpha"
setfattr -n user.author -v "lucas" "$MNT/projects/alpha/main.rs"

ls -laR "$MNT/projects/" | head -20
echo "  fs stats: $(df --output=source,size,used "$MNT" | tail -n1)"

step "Unmount"
fusermount3 -u "$MNT"
sleep 0.3

# ─── 4. Offline check ────────────────────────────────────────────────
step "sotfsctl check $DB"
"$BIN_DIR/sotfsctl" check "$DB"

# ─── 5. Re-mount + verify persistence ────────────────────────────────
step "Re-mount + verify"
"$BIN_DIR/sotfs-fuse" "$MNT" --db "$DB" &
sleep 1
test "$(cat "$MNT/projects/alpha/main.rs")" = "alpha source"
test "$(readlink "$MNT/projects/beta/link-to-alpha")" = "../alpha/main.rs"
test "$(getfattr --only-values -n user.author "$MNT/projects/alpha/main.rs" 2>/dev/null)" = "lucas"
echo "  persistence verified ✓"

step "Unmount"
fusermount3 -u "$MNT"
sleep 0.3

# ─── 6. Export to Graph Hunter ───────────────────────────────────────
HUNTER_OUT="${HUNTER_OUT:-/tmp/sotfs-demo-hunter.json}"
step "Export to Graph Hunter ($HUNTER_OUT)"
"$BIN_DIR/sotfs-export-hunter" "$DB" -o "$HUNTER_OUT"
echo "  first 3 events:"
jq '.events[0:3]' "$HUNTER_OUT"

step "Done"
echo "Volume:        $DB"
echo "Hunter export: $HUNTER_OUT"
echo "Mountpoint:    $MNT (clean)"
