#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Installs the per-user Mitcad toolchain with pinned versions: Rust, vcpkg,
# Qt and cargo-deny, and writes CMakeUserPresets.json for this checkout.
# Run as the developer, not as root, after install-packages.sh.
#
# Optional: MITCAD_GPU_ADAPTER=NVIDIA selects the GPU Mesa uses under WSLg.
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
# The pinned versions (Qt, aqtinstall, cargo-deny, vcpkg) shared with the
# other setup scripts.
# shellcheck source=tools/dev-env/versions.sh
. "$SCRIPT_DIR/versions.sh"

REPO=$(cd "$(dirname "$0")/../.." && pwd)

if [ "$(id -u)" = 0 ]; then
  echo "Run as the developer user, not root." >&2
  exit 1
fi

# The toolchain pinned in rust-toolchain.toml, with its components, so that
# rustup has nothing to download during builds and tests.
RUST_TOOLCHAIN=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$REPO/rust-toolchain.toml")
RUST_COMPONENTS=$(sed -n 's/^components *= *\[\(.*\)\]/\1/p' "$REPO/rust-toolchain.toml" | tr -d '" ')

echo "== Rust $RUST_TOOLCHAIN ($RUST_COMPONENTS)"
if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --profile minimal --default-toolchain "$RUST_TOOLCHAIN"
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

echo "== Qt $QT_VERSION"
if [ ! -d "$HOME/Qt/$QT_VERSION/gcc_64" ]; then
  python3 -m venv "$HOME/tools/aqt"
  "$HOME/tools/aqt/bin/pip" install --quiet "$AQT_SPEC"
  (cd /tmp && "$HOME/tools/aqt/bin/aqt" install-qt linux desktop "$QT_VERSION" linux_gcc_64 \
    -O "$HOME/Qt" > /dev/null)
fi

echo "== WSLg GPU settings"
if [ -e /dev/dxg ] && ! grep -q GALLIUM_DRIVER "$HOME/.profile"; then
  {
    echo ""
    echo "# WSLg: hardware-accelerated OpenGL through Mesa's d3d12 driver"
    echo "export GALLIUM_DRIVER=d3d12"
    [ -n "${MITCAD_GPU_ADAPTER:-}" ] &&
      echo "export MESA_D3D12_DEFAULT_ADAPTER_NAME=$MITCAD_GPU_ADAPTER"
  } >> "$HOME/.profile"
fi

echo "== CMakeUserPresets.json"
if [ ! -f "$REPO/CMakeUserPresets.json" ]; then
  cat > "$REPO/CMakeUserPresets.json" << EOF
{
  "version": 6,
  "configurePresets": [
    {
      "name": "dev",
      "displayName": "Linux Debug (local toolchain)",
      "inherits": "linux-debug",
      "environment": {
        "VCPKG_ROOT": "\$penv{HOME}/vcpkg",
        "QT_ROOT_DIR": "\$penv{HOME}/Qt/$QT_VERSION/gcc_64"
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
