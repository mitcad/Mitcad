#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Installs the per-user Mitcad toolchain with pinned versions on macOS:
# CMake, Ninja, Rust, cargo-deny, vcpkg and Qt, and writes
# CMakeUserPresets.json for this checkout. Meant for the isolated macOS
# arm64 build VM (UTM or lume on Apple's Virtualization.framework); never run
# it on the host. Run as the developer, not as root, after
# tools/dev-env/sync-to-mac.sh has copied the working tree.
#
# CMake and Ninja come from the upstream release archives into ~/.local, not
# from Homebrew. The archives are checked against the SHA-256 pins in
# versions.sh; while a pin is empty the script stops, unless MITCAD_SKIP_HASH=1
# accepts the unverified download.
#
# Needs the Xcode Command Line Tools (xcode-select --install); no full Xcode.
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=tools/dev-env/versions.sh
. "$SCRIPT_DIR/versions.sh"

REPO=$(cd "$SCRIPT_DIR/../.." && pwd)

if [ "$(id -u)" = 0 ]; then
  echo "Run as the developer user, not root." >&2
  exit 1
fi
if [ "$(uname -s)" != Darwin ]; then
  echo "This script is for macOS; use setup-user.sh on Linux." >&2
  exit 1
fi
if [ "$(uname -m)" != arm64 ]; then
  echo "This script is for Apple Silicon (arm64) guests; found $(uname -m)." >&2
  exit 1
fi

echo "== Xcode Command Line Tools"
if ! xcode-select -p > /dev/null 2>&1 || ! xcrun --find clang > /dev/null 2>&1; then
  echo "The Xcode Command Line Tools are missing. Install them in the guest with" >&2
  echo "  xcode-select --install" >&2
  echo "and run this script again." >&2
  exit 1
fi
xcode-select -p
clang --version | head -n 1

# Downloads and intermediates stay under $HOME (not /tmp).
WORK="$HOME/.cache/mitcad-setup"
mkdir -p "$WORK" "$HOME/.local/bin"
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

# Fetches a URL into $WORK and checks it against a SHA-256 pin. An empty pin
# is an error unless MITCAD_SKIP_HASH=1.
download() {
  local url=$1 expected=$2 file="$WORK/$(basename "$1")" actual
  curl --proto '=https' --tlsv1.2 -fsSL -o "$file" "$url"
  if [ -n "$expected" ]; then
    actual=$(shasum -a 256 "$file" | cut -d ' ' -f 1)
    if [ "$actual" != "$expected" ]; then
      echo "SHA-256 mismatch for $url" >&2
      echo "  expected $expected" >&2
      echo "  got      $actual" >&2
      rm -f "$file"
      exit 1
    fi
  elif [ "${MITCAD_SKIP_HASH:-}" = 1 ]; then
    echo "WARNING: $(basename "$file") is not verified (MITCAD_SKIP_HASH=1); sha256 $(shasum -a 256 "$file" | cut -d ' ' -f 1)" >&2
  else
    echo "No SHA-256 pin for $url in tools/dev-env/versions.sh." >&2
    echo "Fill it in from upstream, or set MITCAD_SKIP_HASH=1 to accept the download unverified." >&2
    rm -f "$file"
    exit 1
  fi
  printf '%s\n' "$file"
}

echo "== CMake $CMAKE_VERSION"
if ! cmake --version 2> /dev/null | head -n 1 | grep -q "$CMAKE_VERSION"; then
  archive=$(download \
    "https://github.com/Kitware/CMake/releases/download/v$CMAKE_VERSION/cmake-$CMAKE_VERSION-macos-universal.tar.gz" \
    "$CMAKE_MACOS_SHA256")
  rm -rf "$HOME/.local/cmake-$CMAKE_VERSION" "$WORK/cmake-extract"
  mkdir -p "$WORK/cmake-extract"
  tar -xzf "$archive" -C "$WORK/cmake-extract"
  mv "$WORK/cmake-extract/cmake-$CMAKE_VERSION-macos-universal" "$HOME/.local/cmake-$CMAKE_VERSION"
  for tool in cmake ctest cpack; do
    ln -sf "$HOME/.local/cmake-$CMAKE_VERSION/CMake.app/Contents/bin/$tool" "$HOME/.local/bin/$tool"
  done
  rm -rf "$WORK/cmake-extract" "$archive"
fi
cmake --version | head -n 1

echo "== Ninja $NINJA_VERSION"
if [ "$(ninja --version 2> /dev/null || true)" != "$NINJA_VERSION" ]; then
  archive=$(download \
    "https://github.com/ninja-build/ninja/releases/download/v$NINJA_VERSION/ninja-mac.zip" \
    "$NINJA_MAC_SHA256")
  rm -rf "$WORK/ninja-extract"
  mkdir -p "$WORK/ninja-extract"
  unzip -q -o "$archive" -d "$WORK/ninja-extract"
  install -m 755 "$WORK/ninja-extract/ninja" "$HOME/.local/bin/ninja"
  rm -rf "$WORK/ninja-extract" "$archive"
fi
echo "ninja $(ninja --version)"

# vcpkg's ports fix up their .pc files with a pkg-config from the system,
# which macOS lacks: pkgconf, built from its release archive.
echo "== pkgconf $PKGCONF_VERSION"
if [ "$(pkg-config --version 2> /dev/null || true)" != "$PKGCONF_VERSION" ]; then
  archive=$(download \
    "https://distfiles.ariadne.space/pkgconf/pkgconf-$PKGCONF_VERSION.tar.xz" \
    "$PKGCONF_SHA256")
  rm -rf "$WORK/pkgconf-extract"
  mkdir -p "$WORK/pkgconf-extract"
  tar -xJf "$archive" -C "$WORK/pkgconf-extract"
  (cd "$WORK/pkgconf-extract/pkgconf-$PKGCONF_VERSION" &&
    ./configure --quiet --prefix="$HOME/.local/pkgconf-$PKGCONF_VERSION" &&
    make --quiet -j "$(sysctl -n hw.ncpu)" && make --quiet install) > /dev/null
  ln -sf "$HOME/.local/pkgconf-$PKGCONF_VERSION/bin/pkgconf" "$HOME/.local/bin/pkg-config"
  rm -rf "$WORK/pkgconf-extract" "$archive"
fi
echo "pkg-config $(pkg-config --version)"

# The Command Line Tools' python3 is too old for aqtinstall's dependencies,
# so aqtinstall runs on a standalone CPython build kept apart in ~/.local.
PYTHON_HOME="$HOME/.local/python-$PYTHON_VERSION"
echo "== Python $PYTHON_VERSION"
if [ ! -x "$PYTHON_HOME/bin/python3" ]; then
  archive=$(download \
    "https://github.com/astral-sh/python-build-standalone/releases/download/$PYTHON_BUILD/cpython-$PYTHON_VERSION%2B$PYTHON_BUILD-aarch64-apple-darwin-install_only.tar.gz" \
    "$PYTHON_MAC_SHA256")
  rm -rf "$PYTHON_HOME" "$WORK/python-extract"
  mkdir -p "$WORK/python-extract"
  tar -xzf "$archive" -C "$WORK/python-extract"
  mv "$WORK/python-extract/python" "$PYTHON_HOME"
  rm -rf "$WORK/python-extract" "$archive"
fi
"$PYTHON_HOME/bin/python3" --version

# Non-interactive ssh commands (the sync and build steps) run zsh without a
# login shell, which reads only ~/.zshenv.
echo "== PATH in ~/.zshenv"
if ! grep -q '# mitcad toolchain' "$HOME/.zshenv" 2> /dev/null; then
  {
    echo ""
    echo "# mitcad toolchain"
    echo 'export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"'
  } >> "$HOME/.zshenv"
fi

# The toolchain pinned in rust-toolchain.toml, with its components, so that
# rustup has nothing to download during builds and tests.
RUST_TOOLCHAIN=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$REPO/rust-toolchain.toml")
RUST_COMPONENTS=$(sed -n 's/^components *= *\[\(.*\)\]/\1/p' "$REPO/rust-toolchain.toml" | tr -d '" ')

echo "== Rust $RUST_TOOLCHAIN ($RUST_COMPONENTS)"
if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --no-modify-path --profile minimal --default-toolchain "$RUST_TOOLCHAIN"
fi
# shellcheck source=/dev/null
. "$HOME/.cargo/env"
rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal --component "$RUST_COMPONENTS" \
  --no-self-update
rustc "+$RUST_TOOLCHAIN" --version

echo "== cargo-deny $CARGO_DENY_VERSION"
if ! cargo deny --version 2> /dev/null | grep -q "$CARGO_DENY_VERSION"; then
  cargo "+$RUST_TOOLCHAIN" install cargo-deny --version "$CARGO_DENY_VERSION" --locked
fi

echo "== vcpkg at $VCPKG_COMMIT"
if [ ! -d "$HOME/vcpkg/.git" ]; then
  git clone --quiet https://github.com/microsoft/vcpkg "$HOME/vcpkg"
fi
git -C "$HOME/vcpkg" fetch --quiet origin
git -C "$HOME/vcpkg" checkout --quiet "$VCPKG_COMMIT"
"$HOME/vcpkg/bootstrap-vcpkg.sh" -disableMetrics > /dev/null

# aqtinstall's Qt for macOS (clang_64) lands in ~/Qt/<version>/macos.
echo "== Qt $QT_VERSION"
if [ ! -d "$HOME/Qt/$QT_VERSION/macos" ]; then
  rm -rf "$HOME/tools/aqt"
  "$PYTHON_HOME/bin/python3" -m venv "$HOME/tools/aqt"
  "$HOME/tools/aqt/bin/pip" install --quiet "$AQT_SPEC"
  (cd "$WORK" && "$HOME/tools/aqt/bin/aqt" install-qt mac desktop "$QT_VERSION" clang_64 \
    -O "$HOME/Qt" > /dev/null)
fi

echo "== mosquitto (a test tool)"
# The MQTT broker of the live updates' tests (mitcad#89); they start their
# own and skip without it. From Homebrew when it is there, not as a service.
if command -v mosquitto > /dev/null 2>&1 || [ -x /opt/homebrew/sbin/mosquitto ]; then
  echo "mosquitto is installed"
elif command -v brew > /dev/null 2>&1; then
  brew install mosquitto
else
  echo "Homebrew is missing: the tests against mosquitto are skipped (brew install mosquitto)"
fi

echo "== CMakeUserPresets.json"
if [ ! -f "$REPO/CMakeUserPresets.json" ]; then
  cat > "$REPO/CMakeUserPresets.json" << EOF
{
  "version": 6,
  "configurePresets": [
    {
      "name": "dev",
      "displayName": "macOS Debug (local toolchain)",
      "inherits": "macos-debug",
      "environment": {
        "VCPKG_ROOT": "\$penv{HOME}/vcpkg",
        "QT_ROOT_DIR": "\$penv{HOME}/Qt/$QT_VERSION/macos"
      }
    }
  ],
  "buildPresets": [{ "name": "dev", "configurePreset": "dev" }],
  "testPresets": [
    { "name": "dev", "configurePreset": "dev", "output": { "outputOnFailure": true } }
  ]
}
EOF
fi

echo "Done. Build with: cmake --preset dev && cmake --build --preset dev && ctest --preset dev"
