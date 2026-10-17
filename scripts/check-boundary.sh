#!/usr/bin/env bash
# Crate boundaries, mechanically enforced.
#
# `cargo tree -i <dep>` exits 101 when the dependency is absent, which is precisely the state
# we want. Checking the exit status would therefore fail whenever the boundary holds, so we
# check for OUTPUT instead: any line naming the dependency is a leak.
set -uo pipefail
cd "$(dirname "$0")/.."

# RULES: what a crate reaches through ANY path (transitive, default features). keycap-core is
# the pure remap core a compositor links: no device, socket, runtime, bus or file-watching
# crate, so no evdev, uinput, tokio, zbus, notify or rustix.
EFFECTS="evdev input-linux uinput tokio zbus zvariant notify rustix libc nix mio async-std smol"
RULES=(
  "keycap-core: $EFFECTS"
  "keycap-protocol: $EFFECTS toml"
)
fail=0

for rule in "${RULES[@]}"; do
  crate="${rule%%:*}"
  read -r -a forbidden <<<"${rule#*:}"
  # A crate that cargo cannot find would make every check below pass vacuously.
  if ! cargo tree -p "$crate" --depth 0 >/dev/null 2>&1; then
    echo "ERROR: cargo tree cannot resolve $crate; the boundary was not checked"
    fail=1
    continue
  fi
  leaked=0
  for dep in "${forbidden[@]}"; do
    if cargo tree -p "$crate" -i "$dep" -e normal,build 2>/dev/null | grep -q .; then
      echo "LEAK: $crate depends on $dep"
      cargo tree -p "$crate" -i "$dep" -e normal,build 2>/dev/null | head -20
      leaked=1
      fail=1
    fi
  done
  if [ "$leaked" -eq 0 ]; then
    echo "boundary holds: $crate reaches none of ${forbidden[*]}"
  fi
done

# The allowed edges: each crate's DIRECT normal and build dependencies (all features), ours and
# third-party, and nothing else, so the table stays exact. keycap-core reaches serde, toml and
# thiserror, plus keycap-protocol for the two vocabulary types the config speaks (`ProfileName`,
# `Warning`), which brings serde_json with it. Dev dependencies are outside the table.
EDGES=(
  "keycap-core: keycap-protocol thiserror toml"
  "keycap-protocol: serde serde_json thiserror"
  # keycap also links peephole (who the socket's peer is) and turnstile-client (the root service
  # that makes its virtual devices), both by tag; neither brings a bus or an async runtime.
  "keycap: chordkit input-linux keycap-client keycap-core keycap-protocol notify peephole peephole-core rustix serde serde_json thiserror turnstile-client"
  # keycap-client: what an app links to read the desktop keymap. The protocol, chordkit's pure core
  # and rustix for the SOCK_SEQPACKET socket std lacks; no runtime, no file watcher.
  "keycap-client: chordkit keycap-protocol rustix"
)
for edge in "${EDGES[@]}"; do
  crate="${edge%%:*}"
  read -r -a allowed <<<"${edge#*:}"
  found=$(cargo tree -p "$crate" --depth 1 -e normal,build --prefix none --all-features 2>/dev/null \
    | sed 1d | awk '{print $1}' | sort -u | tr '\n' ' ')
  want=$(printf '%s\n' "${allowed[@]}" | grep . | sort -u | tr '\n' ' ')
  if [ "$found" != "$want" ]; then
    echo "EDGE: $crate depends on [${found% }], the table allows [${want% }]"
    fail=1
  else
    echo "edges hold: $crate depends on [${found% }]"
  fi
done

# Every workspace member has a row above, so a new crate cannot slip in unchecked. keycap-devtest
# is a test-only harness and outside the table.
for member in $(sed -n 's#^members = \[\(.*\)\]#\1#p' Cargo.toml | tr -d '", ' | sed 's#crates/#\n#g' | grep .); do
  [ "$member" = keycap-devtest ] && continue
  printf '%s\n' "${EDGES[@]}" | grep -q "^$member:" || { echo "ERROR: $member has no row in EDGES"; fail=1; }
done

exit "$fail"
