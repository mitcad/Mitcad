#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Ubuntu 24.04 system packages for building and testing Mitcad. Run as root.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq \
  build-essential cmake ninja-build pkg-config git curl zip unzip tar rsync ccache \
  autoconf autoconf-archive automake libtool python3-venv \
  libgl1-mesa-dev libegl1-mesa-dev libx11-dev libxext-dev libxi-dev libxrandr-dev \
  libxcursor-dev libxinerama-dev libxkbcommon-dev libxkbcommon-x11-0 libxcb-cursor0 \
  libxcb-icccm4 libxcb-image0 libxcb-keysyms1 libxcb-render-util0 libxcb-xinerama0 \
  libxcb-xkb1 libxcb-shape0 libxcb-randr0 libfontconfig1-dev libfreetype-dev libdbus-1-3 \
  mesa-utils gdb xdotool xvfb x11-apps librsvg2-bin imagemagick
echo "System packages installed."
