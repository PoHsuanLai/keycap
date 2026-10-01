#!/usr/bin/env bash
# Build keycap and print the commands that install it.
#
# Never run by the build or the tests: keycap grabs the keyboards, so installing it changes who
# owns your keys. Every step needs root, so this script builds as you and prints the sudo
# commands instead of running them.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

(cd "$HERE" && cargo build --release)

cat <<STEPS
keycap is built. Install it with:
  sudo install -Dm755 "$HERE/target/release/keycap" /usr/local/bin/keycap
  sudo install -Dm644 "$HERE/dist/keycap.service" /etc/systemd/system/keycap.service
  sudo install -Dm644 "$HERE/dist/70-keycap.rules" /etc/udev/rules.d/70-keycap.rules
  sudo install -Dm644 "$HERE/dist/keycap.polkit.rules" /etc/polkit-1/rules.d/keycap.polkit.rules
  sudo udevadm control --reload-rules
  sudo udevadm trigger --subsystem-match=misc
  sudo systemctl daemon-reload && sudo systemctl enable --now keycap

If the keyboard misbehaves: hold both Shift keys and press Esc to release the grab, or run
  sudo systemctl stop keycap
STEPS
