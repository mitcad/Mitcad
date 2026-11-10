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
# The MQTT broker of the live updates' tests (mitcad#89), a test tool: each
# test starts its own on free ports, so the system service is not needed.
apt-get install -y -qq mosquitto mosquitto-clients
systemctl disable --now mosquitto 2> /dev/null || true
# The live updates' keychain (mitcad#89, cmake/Keychain.cmake): libsecret's
# headers for building QtKeychain (the library itself is loaded at run time,
# when the system has it); GNOME's keyring as a Secret Service for
# tools/ui-live-test.sh, a test tool started by the test in a D-Bus session
# of its own.
apt-get install -y -qq --no-install-recommends libsecret-1-dev gnome-keyring
# The Chromium Embedded Framework (Insert > Component from Web's web browser
# component, mitcad#1579): the system libraries CEF's libcef loads, for the
# build's tests (every desktop with a browser has them).
apt-get install -y -qq --no-install-recommends \
  libnss3 libnspr4 libatk1.0-0t64 libatk-bridge2.0-0t64 libcups2t64 libxcomposite1 libxdamage1 \
  libxfixes3 libxrandr2 libxtst6 libxshmfence1 libgbm1 libdrm2 libpango-1.0-0 libcairo2 libasound2t64
echo "System packages installed."
