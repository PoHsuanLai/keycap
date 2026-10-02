# Shared by install.sh and uninstall.sh. Sourced, never run.
#
# Every change goes through `step`: it prints the command, and runs it unless --dry-run. Root
# commands go through sudo, except under DESTDIR (a staging root for tests), where the files land
# under that directory as you and nothing talks to systemd or udev.

DRY_RUN=no
DESTDIR="${DESTDIR:-}"

say() { printf '%s\n' "$*"; }

# step DESCRIPTION CMD...: print, then run unless this is a dry run.
step() {
  local what="$1"; shift
  say "  $what"
  say "      \$ $*"
  [[ "$DRY_RUN" == yes ]] || "$@"
}

# as_root CMD...: the command with sudo in front, unless we already are root or stage in DESTDIR.
as_root() {
  if [[ -n "$DESTDIR" || "$(id -u)" == 0 ]]; then "$@"; else sudo "$@"; fi
}

# dest PATH: where an absolute system path really goes.
dest() { printf '%s%s' "$DESTDIR" "$1"; }

# root_step DESCRIPTION CMD...: a step that needs root.
root_step() {
  local what="$1"; shift
  if [[ -n "$DESTDIR" || "$(id -u)" == 0 ]]; then step "$what" "$@"; else step "$what" sudo "$@"; fi
}

# Only real systems have a systemd, udev and SELinux to tell; a DESTDIR staging run skips them.
system_step() {
  if [[ -n "$DESTDIR" ]]; then
    say "  $1"
    say "      (skipped: staging into $DESTDIR)"
  else
    root_step "$@"
  fi
}

# install_file MODE SRC DST: copy when the content differs or the mode differs; say "unchanged"
# otherwise. Sets CHANGED=yes when it copied (or would, in a dry run).
CHANGED=no
install_file() {
  local mode="$1" src="$2" dst="$3" real
  real="$(dest "$dst")"
  if [[ -f "$real" ]] && cmp -s "$src" "$real" && [[ "$(stat -c %a "$real")" == "${mode#0}" || "$(stat -c %a "$real")" == "$mode" ]]; then
    say "  unchanged: $dst"
    return 0
  fi
  CHANGED=yes
  root_step "install $dst" install -Dm"$mode" "$src" "$real"
}

# remove_file DST: remove when present.
remove_file() {
  local real
  real="$(dest "$1")"
  if [[ -e "$real" ]]; then
    CHANGED=yes
    root_step "remove $1" rm -f "$real"
  else
    say "  absent: $1"
  fi
}

# unit_loaded NAME: whether systemd knows the unit (always false when staging).
unit_known() { [[ -z "$DESTDIR" ]] && systemctl cat "$1" >/dev/null 2>&1; }
