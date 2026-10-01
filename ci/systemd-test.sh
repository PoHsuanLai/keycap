#!/usr/bin/env bash
# The systemd tier: the installed unit against a fake keyboard, on a machine whose PID 1 is
# systemd (a GitHub Actions ubuntu runner). It checks what no other tier can: that the unit
# starts under its own hardening (DynamicUser, the input group, DeviceAllow), that the watchdog
# kills a hung daemon, that a crash loop ends in `failed`, and that `reset-failed` recovers it.
#
# It installs files under /usr/local and /etc and creates an input device, so it refuses to run
# anywhere but CI unless KEYCAP_CI_ALLOW_INSTALL=1 is set. Build first:
#   cargo build --release -p keycap --bins --examples
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${KEYCAP_TARGET_DIR:-$HERE/target/release}"
UNIT=keycap
FAKE_NAME=keycap-ci-keyboard
SOCKET=/run/keycap/keycap.sock
DROPIN_DIR=/etc/systemd/system/keycap.service.d
FAKE_LOG="$(mktemp)"

step() { printf '\n==== %s ====\n' "$*"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
prop() { systemctl show -p "$1" --value "$UNIT"; }

# wait_for DESCRIPTION SECONDS COMMAND...: polls COMMAND every 100 ms.
wait_for() {
  local what=$1 limit=$2 i
  shift 2
  for ((i = 0; i < limit * 10; i++)); do
    if "$@"; then
      echo "ok: $what (after $((i / 10)).$((i % 10)) s)"
      return 0
    fi
    sleep 0.1
  done
  echo "timed out after ${limit} s waiting for: $what" >&2
  return 1
}

output_present() { grep -qxF 'keycap keyboard' /sys/class/input/input*/name 2>/dev/null; }
output_absent() { ! output_present; }
unit_active() { [ "$(prop ActiveState)" = active ]; }
unit_failed() { [ "$(prop ActiveState)" = failed ]; }
grab_active() {
  "$TARGET/examples/socket_client" "$SOCKET" status 2>/dev/null | grep -qF "$FAKE_NAME Active"
}
fake_ready() { grep -q '^ready ' "$FAKE_LOG"; }

dump() {
  echo; echo "==== diagnostics ===="
  systemctl status "$UNIT" --no-pager -l || true
  sudo journalctl -u "$UNIT" --no-pager -n 200 || true
  echo "--- fake keyboard log"; cat "$FAKE_LOG" || true
  echo "--- /dev/uinput and /dev/input"; ls -l /dev/uinput /dev/input || true
  echo "--- input device names"; grep -r . /sys/class/input/input*/name || true
  echo "--- socket status"; "$TARGET/examples/socket_client" "$SOCKET" status || true
}

cleanup() {
  sudo systemctl stop "$UNIT" 2>/dev/null || true
  sudo systemctl reset-failed "$UNIT" 2>/dev/null || true
  sudo pkill -f '[e]xamples/fake_keyboard' 2>/dev/null || true
}

on_exit() {
  local rc=$?
  if [ "$rc" -ne 0 ]; then dump; fi
  cleanup
  exit "$rc"
}
trap on_exit EXIT

step "preconditions"
if [ "${CI:-}" != true ] && [ "${KEYCAP_CI_ALLOW_INSTALL:-}" != 1 ]; then
  fail "this installs a unit and creates an input device; run it in CI or set KEYCAP_CI_ALLOW_INSTALL=1"
fi
[ "$(ps -p 1 -o comm=)" = systemd ] || fail "PID 1 is not systemd"
systemctl --version | head -n 1
for bin in "$TARGET/keycap" "$TARGET/examples/fake_keyboard" "$TARGET/examples/socket_client"; do
  [ -x "$bin" ] || fail "$bin is missing; build with: cargo build --release -p keycap --bins --examples"
done

step "load uinput"
sudo modprobe uinput
ls -l /dev/uinput

step "install the binary, the unit, the udev rule and the polkit rule, as install.sh prints"
sudo install -Dm755 "$TARGET/keycap" /usr/local/bin/keycap
sudo install -Dm644 "$HERE/dist/keycap.service" /etc/systemd/system/keycap.service
sudo install -Dm644 "$HERE/dist/70-keycap.rules" /etc/udev/rules.d/70-keycap.rules
sudo install -Dm644 "$HERE/dist/keycap.polkit.rules" /etc/polkit-1/rules.d/keycap.polkit.rules
sudo install -d "$DROPIN_DIR"
printf '[Service]\nEnvironment=KEYCAP_GRAB_VIRTUAL=%s\n' "$FAKE_NAME" |
  sudo tee "$DROPIN_DIR/ci.conf" >/dev/null
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=misc
sudo udevadm settle
sudo systemctl daemon-reload
echo "/dev/uinput is $(stat -c '%U:%G %a' /dev/uinput); the rule asks for root:input 660"

step "plug in the fake keyboard (as root, like a device a person plugs in)"
sudo env KEYCAP_FAKE_NAME="$FAKE_NAME" nohup "$TARGET/examples/fake_keyboard" hold 900 \
  >"$FAKE_LOG" 2>&1 </dev/null &
wait_for "the fake keyboard to be created" 15 fake_ready
cat "$FAKE_LOG"

step "start the unit"
sudo systemctl start "$UNIT"
systemctl is-active --quiet "$UNIT" || fail "the unit is not active after start"
echo "main pid $(prop MainPID), restarts so far $(prop NRestarts)"
wait_for "keycap keyboard to appear" 10 output_present
wait_for "the daemon to hold the fake keyboard" 15 grab_active

step "watchdog: kill -STOP the daemon; systemd must restart it"
pid="$(prop MainPID)"
restarts_before="$(prop NRestarts)"
restarted() { [ "$(prop NRestarts)" -gt "$restarts_before" ]; }
sudo kill -STOP "$pid"
started=$SECONDS
wait_for "NRestarts to rise above $restarts_before" 8 restarted
elapsed=$((SECONDS - started))
[ "$elapsed" -le 6 ] || fail "the watchdog took ${elapsed} s to restart a stopped daemon (WatchdogSec=2)"
wait_for "the unit to be active again" 10 unit_active
new_pid="$(prop MainPID)"
[ "$new_pid" != "$pid" ] || fail "main pid is still $pid after the watchdog restart"
wait_for "keycap keyboard to reappear" 10 output_present
wait_for "the new daemon to hold the fake keyboard" 15 grab_active
sudo journalctl -u "$UNIT" --no-pager | grep -i 'watchdog' || fail "the journal does not mention the watchdog"

step "crash loop: kill -9 until the start limit trips"
last=0
new_main_pid() {
  local p
  p="$(prop MainPID)"
  [ "$p" != 0 ] && [ "$p" != "$last" ]
}
for i in 1 2 3 4 5 6; do
  if unit_failed; then break; fi
  wait_for "a new main pid before kill #$i" 5 new_main_pid || break
  last="$(prop MainPID)"
  echo "kill -9 #$i: pid $last"
  sudo kill -9 "$last" || true
done
wait_for "the unit to fail" 10 unit_failed
result="$(prop Result)"
[ "$result" = start-limit-hit ] || fail "Result is '$result', expected start-limit-hit"
wait_for "keycap keyboard to be gone while the unit is failed" 5 output_absent

step "reset-failed and start: the unit recovers"
sudo systemctl reset-failed "$UNIT"
sudo systemctl start "$UNIT"
systemctl is-active --quiet "$UNIT" || fail "the unit is not active after reset-failed and start"
wait_for "keycap keyboard to appear" 10 output_present
wait_for "the daemon to hold the fake keyboard" 15 grab_active

step "stop: the virtual keyboard goes with the process"
sudo systemctl stop "$UNIT"
wait_for "keycap keyboard to be gone" 5 output_absent

step "systemd tier passed"
