#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# An Ubuntu 24.04 (noble) build root on a Linux host with a newer system:
# a debootstrapped noble file system that builds and tests run in through
# bubblewrap, as the host's own user, with a home folder of its own. The
# Linux release is built there (docs/development.md#linux-on-a-newer-host):
# the AppImage takes glibc and libstdc++ from the user's system, so it
# needs the oldest supported system's (Ubuntu 24.04's), not the host's
# (mitcad#789).
#
#   build-root.sh create [--plain] <root>
#     As root, once (again to update the packages): debootstrap noble into
#     <root> and install the build packages (install-packages.sh and the
#     tools the tests use). --plain: only what an Ubuntu 24.04 desktop has
#     for running Mitcad, plus Xvfb and xdotool, for a start check of an
#     AppImage on a clean 24.04.
#   build-root.sh run [--bind <dir>]... <root> <home> [<command>...]
#     As the user: <command> (default: bash -l) in <root>, with <home> as
#     HOME. The root's /usr, /etc and /var are read-only; <home>, every
#     --bind folder (at the same path), /tmp, the user's runtime folder
#     (systemd-run --user, which check-all's memory scope uses) and the
#     network are the host's, and so is the test runner's history
#     (<home>/.mitcad is the user's own ~/.mitcad). The environment is
#     cleared but for the MITCAD_* variables and the build parallelism
#     (CMAKE_BUILD_PARALLEL_LEVEL, CARGO_BUILD_JOBS, VCPKG_MAX_CONCURRENCY),
#     which pass in; a MITCAD_* path outside the bound folders is reported,
#     as the root would not see it.
#   build-root.sh setup [--bind <dir>]... <root> <home> <checkout>
#     As the user, once: copies <checkout> to <home>/src/mitcad and runs
#     setup-user.sh (Rust, vcpkg, Qt, the render worker's libraries with
#     their CUDA device; writes CMakeUserPresets.json there) and
#     install-appimage-tools.sh in the root. Everything lands in <home>,
#     built against the root's glibc.
#
# Needs debootstrap (create installs it on Ubuntu or Debian) and bwrap
# with unprivileged user namespaces.
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
SUITE=noble
MIRROR=${MITCAD_ROOT_MIRROR:-http://archive.ubuntu.com/ubuntu}
COMPONENTS=main,universe,multiverse

usage() {
  sed -n '3,/^set -euo/{/^#/s/^# \{0,1\}//p}' "$0" >&2
  exit 2
}

# The tools the build, check-all and the tests use beyond
# install-packages.sh (an Ubuntu desktop has them); the core fonts for the
# sketch texts of imported designs.
BUILD_EXTRAS=(
  ca-certificates locales systemd dbus jq file openssl xz-utils zstd bc lsof patch
  x11-xkb-utils xauth fonts-dejavu-core fonts-liberation fonts-noto-core
  ttf-mscorefonts-installer
)
# What an Ubuntu 24.04 desktop has that Mitcad's AppImage uses (glibc,
# libstdc++, X11/xcb, OpenGL with GLVND's libOpenGL, EGL, Wayland's client
# libraries (Qt's Wayland platform, mitcad#2031), fontconfig, FreeType, D-Bus,
# GLib, fonts; each package is on the 24.04 desktop image's manifest),
# and Xvfb, xdotool and xwininfo to start it on a hidden display.
PLAIN_PACKAGES=(
  ca-certificates locales xvfb xauth xdotool x11-utils fonts-dejavu-core
  libgl1 libopengl0 libegl1 libglx-mesa0 libgl1-mesa-dri libfontconfig1 libfreetype6 libdbus-1-3
  libglib2.0-0t64 libx11-6 libx11-xcb1 libxext6 libxrender1 libxi6 libsm6 libice6
  libxkbcommon0 libxkbcommon-x11-0 libxcb-cursor0 libxcb-icccm4 libxcb-image0
  libxcb-keysyms1 libxcb-render-util0 libxcb-xinerama0 libxcb-xkb1 libxcb-shape0
  libxcb-randr0 libwayland-client0 libwayland-cursor0 libwayland-egl1
)

cmd_create() {
  local plain=0 root
  if [ "${1:-}" = --plain ]; then
    plain=1
    shift
  fi
  root=${1:?usage: build-root.sh create [--plain] <root>}
  [ "$(id -u)" = 0 ] || { echo "create runs as root" >&2; exit 1; }
  if ! command -v debootstrap > /dev/null; then
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq debootstrap
  fi
  if [ ! -x "$root/usr/bin/apt-get" ]; then
    mkdir -p "$root"
    debootstrap --variant=minbase --components="$COMPONENTS" "$SUITE" "$root" "$MIRROR"
  fi
  printf 'deb %s %s %s\n' \
    "$MIRROR" "$SUITE" "${COMPONENTS//,/ }" \
    "$MIRROR" "$SUITE-updates" "${COMPONENTS//,/ }" \
    "$MIRROR" "$SUITE-security" "${COMPONENTS//,/ }" > "$root/etc/apt/sources.list"
  # A plain file, which run replaces with the host's by a bind mount.
  cp --remove-destination /etc/resolv.conf "$root/etc/resolv.conf"
  # Mount points of run's binds.
  [ -e "$root/etc/machine-id" ] || : > "$root/etc/machine-id"
  # No services start in the root.
  printf '#!/bin/sh\nexit 101\n' > "$root/usr/sbin/policy-rc.d"
  chmod 755 "$root/usr/sbin/policy-rc.d"

  # /proc and /dev for the packages' scripts while apt runs, unmounted at
  # the end (not recursive: nothing of the host's below them).
  mount -t proc proc "$root/proc"
  mount --bind /dev "$root/dev"
  # (Expanded now: the function's locals are gone when the trap runs.)
  trap "umount '$root/dev' '$root/proc'" EXIT
  local apt=(chroot "$root" env DEBIAN_FRONTEND=noninteractive LC_ALL=C apt-get -y -qq)
  "${apt[@]}" update
  "${apt[@]}" full-upgrade
  if [ "$plain" = 1 ]; then
    "${apt[@]}" install --no-install-recommends "${PLAIN_PACKAGES[@]}"
  else
    cp "$SCRIPT_DIR/install-packages.sh" "$root/root/install-packages.sh"
    chroot "$root" env LC_ALL=C bash /root/install-packages.sh
    echo "ttf-mscorefonts-installer msttcorefonts/accepted-mscorefonts-eula select true" |
      chroot "$root" debconf-set-selections
    "${apt[@]}" install "${BUILD_EXTRAS[@]}"
  fi
  sed -i 's/^# *\(en_US.UTF-8 UTF-8\)/\1/' "$root/etc/locale.gen"
  chroot "$root" locale-gen > /dev/null
  "${apt[@]}" clean
  echo "Ready: $root ($(chroot "$root" ldd --version | head -1))"
}

# bwrap_args <root> <home> <bind>...: ARGS, bwrap's arguments
bwrap_args() {
  local root=$1 home=$2 uid dir
  shift 2
  uid=$(id -u)
  ARGS=(--die-with-parent --clearenv --tmpfs /
    --ro-bind "$root/usr" /usr --ro-bind "$root/etc" /etc --ro-bind "$root/var" /var
    --symlink usr/bin /bin --symlink usr/sbin /sbin --symlink usr/lib /lib --symlink usr/lib64 /lib64
    --ro-bind-data 9 /etc/passwd --ro-bind /etc/group /etc/group
    --tmpfs /var/tmp --dev /dev --bind /proc /proc --ro-bind /sys /sys --bind /tmp /tmp
    --tmpfs /run --bind "$home" "$home")
  # The host's name servers, where the root's /etc/resolv.conf points (a
  # link into /run, as systemd's package makes it, or a plain file).
  local resolv=/etc/resolv.conf
  if [ -L "$root/etc/resolv.conf" ]; then
    resolv=$(readlink "$root/etc/resolv.conf")
    case $resolv in /*) ;; *) resolv=$(realpath -m "/etc/$resolv") ;; esac
  fi
  ARGS+=(--ro-bind "$(readlink -f /etc/resolv.conf)" "$resolv")
  [ ! -e /etc/machine-id ] || ARGS+=(--ro-bind /etc/machine-id /etc/machine-id)
  if [ -d "/run/user/$uid" ]; then
    ARGS+=(--bind "/run/user/$uid" "/run/user/$uid" --setenv XDG_RUNTIME_DIR "/run/user/$uid")
    [ ! -S "/run/user/$uid/bus" ] || ARGS+=(--setenv DBUS_SESSION_BUS_ADDRESS "unix:path=/run/user/$uid/bus")
  fi
  for dir in "$@"; do
    ARGS+=(--bind "$dir" "$dir")
  done
  # The test runner's history (~/.mitcad: each test's time and memory, the
  # failure scores, the UI tests' cap; tools/testrun/README.md) is the
  # host user's own folder, shared both ways: a new root starts from what
  # the host's runs learnt, its runs add to it, and neither side goes back
  # to an older copy. The runner's lock file beside the history is shared
  # with it, so runs inside and outside the root add one at a time.
  if [ -d "$HOME/.mitcad" ] && [ "$(readlink -f "$HOME")" != "$(readlink -f "$home")" ]; then
    mkdir -p "$home/.mitcad"
    ARGS+=(--bind "$HOME/.mitcad" "$home/.mitcad")
  fi
  ARGS+=(--setenv HOME "$home" --setenv USER "$(id -un)" --setenv LOGNAME "$(id -un)"
    --setenv PATH /usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
    --setenv LANG C.UTF-8 --setenv TERM "${TERM:-dumb}")
  # The host's time zone (the root's is UTC), for the logs' clock times.
  local tz=${TZ:-$(readlink -f /etc/localtime | sed -n 's|^/usr/share/zoneinfo/||p')}
  [ -z "$tz" ] || [ ! -e "$root/usr/share/zoneinfo/$tz" ] || ARGS+=(--setenv TZ "$tz")
  # The MITCAD_* variables (the corpora, test slots, ...) and the build
  # parallelism (CMAKE_BUILD_PARALLEL_LEVEL, CARGO_BUILD_JOBS,
  # VCPKG_MAX_CONCURRENCY) pass in.
  local name value bound
  while IFS='=' read -r name value; do
    case $name in MITCAD_* | CMAKE_BUILD_PARALLEL_LEVEL | CARGO_BUILD_JOBS | VCPKG_MAX_CONCURRENCY) ;; *) continue ;; esac
    ARGS+=(--setenv "$name" "${!name}")
    case $value in /*) ;; *) continue ;; esac
    bound=0
    for dir in "$home" "$@" /tmp; do
      case ${value%%:*}/ in "${dir%/}"/*) bound=1 ;; esac
    done
    [ "$bound" = 1 ] || echo "build-root: $name=$value is outside the bound folders" >&2
  done < <(env)
}

# parse_binds "$@": BINDS from leading --bind <dir> options, REST the rest
parse_binds() {
  BINDS=()
  while [ "${1:-}" = --bind ]; do
    [ -d "${2:-}" ] || { echo "build-root: no folder ${2:-}" >&2; exit 1; }
    BINDS+=("$(cd "$2" && pwd -P)")
    shift 2
  done
  REST=("$@")
}

cmd_run() {
  local root home
  parse_binds "$@"
  set -- "${REST[@]+"${REST[@]}"}"
  root=${1:?usage: build-root.sh run [--bind <dir>]... <root> <home> [<command>...]}
  home=${2:?usage: build-root.sh run [--bind <dir>]... <root> <home> [<command>...]}
  shift 2
  [ -x "$root/usr/bin/apt-get" ] || { echo "build-root: no root in $root (build-root.sh create)" >&2; exit 1; }
  [ "$(id -u)" != 0 ] || { echo "build-root: run as the user, not root" >&2; exit 1; }
  mkdir -p "$home"
  home=$(cd "$home" && pwd -P)
  bwrap_args "$root" "$home" "${BINDS[@]+"${BINDS[@]}"}"
  # Outside the bound folders the root has no such folder: start in the home.
  local inside=0 dir
  for dir in "$home" "${BINDS[@]+"${BINDS[@]}"}" /tmp; do
    case $PWD/ in "$dir"/*) inside=1 ;; esac
  done
  if [ "$inside" = 1 ]; then ARGS+=(--chdir "$PWD"); else ARGS+=(--chdir "$home"); fi
  [ $# -gt 0 ] || set -- bash -l
  # The host's users, the user's home being <home> (rustup and others
  # compare HOME with it).
  exec 9< <(awk -F: -v OFS=: -v uid="$(id -u)" -v home="$home" '$3 == uid { $6 = home } 1' /etc/passwd)
  exec bwrap "${ARGS[@]}" -- "$@"
}

cmd_setup() {
  local root home checkout
  parse_binds "$@"
  set -- "${REST[@]+"${REST[@]}"}"
  root=${1:?usage: build-root.sh setup [--bind <dir>]... <root> <home> <checkout>}
  home=${2:?usage: build-root.sh setup [--bind <dir>]... <root> <home> <checkout>}
  checkout=${3:?usage: build-root.sh setup [--bind <dir>]... <root> <home> <checkout>}
  mkdir -p "$home/src/mitcad"
  rsync -a --exclude=.git --exclude=/build --exclude=/CMakeUserPresets.json \
    "$checkout/" "$home/src/mitcad/"
  # A login shell's profile: ~/.local/bin and cargo (which rustup adds).
  [ -f "$home/.profile" ] || printf '%s\n' \
    '[ -d "$HOME/.local/bin" ] && PATH="$HOME/.local/bin:$PATH"' \
    '[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"' > "$home/.profile"
  local binds=() dir
  for dir in "${BINDS[@]+"${BINDS[@]}"}"; do binds+=(--bind "$dir"); done
  bash "$SCRIPT_DIR/build-root.sh" run "${binds[@]+"${binds[@]}"}" "$root" "$home" bash -lc \
    'cd "$HOME/src/mitcad" && tools/dev-env/setup-user.sh && tools/dev-env/install-appimage-tools.sh'
}

case ${1:-} in
  create) shift; cmd_create "$@" ;;
  run) shift; cmd_run "$@" ;;
  setup) shift; cmd_setup "$@" ;;
  *) usage ;;
esac
