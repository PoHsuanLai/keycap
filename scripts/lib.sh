# Shared by install.sh and uninstall.sh. Sourced, never run.
#
# Every change goes through `step`: it prints the command, and runs it unless --dry-run. Root
# commands go through the first escalation helper found ($SUDO, sudo, doas, pkexec); with none,
# they are printed for you to run as root. Under DESTDIR (a staging root for packages and tests)
# the files land under that directory as you and nothing talks to systemd or udev.

DRY_RUN=no
DESTDIR="${DESTDIR:-}"
PREFIX=/usr/local
# yes once a root step was printed instead of run (no escalation helper).
MANUAL=no

say() { printf '%s\n' "$*"; }

# step DESCRIPTION CMD...: print, then run unless this is a dry run.
step() {
  local what="$1"; shift
  say "  $what"
  say "      \$ $*"
  [[ "$DRY_RUN" == yes ]] || "$@"
}

# ESCALATE: the words that put a command in front of root, empty when none is available.
# $SUDO may hold arguments ("sudo -n"); the first word must be a program on PATH.
ESCALATE=()
pick_escalator() {
  local candidate words
  for candidate in "${SUDO:-}" sudo doas pkexec; do
    [[ -n "$candidate" ]] || continue
    read -ra words <<<"$candidate"
    if command -v "${words[0]}" >/dev/null 2>&1; then
      ESCALATE=("${words[@]}")
      return 0
    fi
  done
  return 0
}
pick_escalator

# set_layout: where each kind of file goes for $PREFIX. The default prefix keeps the machine-local
# places in /etc (an admin's files outrank the vendor ones); any other prefix is a package or a
# bundle, so the vendor directories under it. The machine config is /etc/keycap either way.
set_layout() {
  BINDIR="$PREFIX/bin"
  if [[ "$PREFIX" == /usr/local ]]; then
    UNITDIR=/etc/systemd/system
    UDEVDIR=/etc/udev/rules.d
    POLKITDIR=/etc/polkit-1/rules.d
    MODULESDIR=/etc/modules-load.d
  else
    UNITDIR="$PREFIX/lib/systemd/system"
    UDEVDIR="$PREFIX/lib/udev/rules.d"
    POLKITDIR="$PREFIX/share/polkit-1/rules.d"
    MODULESDIR="$PREFIX/lib/modules-load.d"
  fi
}

# absolute_arg FLAG VALUE: VALUE with trailing slashes dropped, or exit 2 when it is not absolute.
absolute_arg() {
  case "$2" in
    /*) local v="${2%/}"; printf '%s' "${v:-/}" ;;
    *) say "$0: $1 needs an absolute path"; exit 2 ;;
  esac
}

# dest PATH: where an absolute system path really goes.
dest() { printf '%s%s' "$DESTDIR" "$1"; }

# root_step DESCRIPTION CMD...: a step that needs root.
root_step() {
  local what="$1"; shift
  if [[ -n "$DESTDIR" || "$(id -u)" == 0 ]]; then
    step "$what" "$@"
  elif (( ${#ESCALATE[@]} > 0 )); then
    step "$what" "${ESCALATE[@]}" "$@"
  else
    MANUAL=yes
    say "  $what"
    say "      (no sudo, doas or pkexec here: run this as root)"
    say "      \$ $*"
  fi
}

# manual_note: the closing line when root steps were printed instead of run.
manual_note() {
  [[ "$MANUAL" == yes ]] || return 0
  say "Some steps need root and no sudo, doas or pkexec was found. Run the commands marked above as root,"
  say "or set SUDO to your own helper and run this again."
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
