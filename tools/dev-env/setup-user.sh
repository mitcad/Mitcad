#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Installs the per-user Mitcad toolchain with pinned versions: Rust, vcpkg,
# Qt and cargo-deny, builds the render worker's libraries (build-cycles.sh)
# and writes CMakeUserPresets.json for this checkout.
# Run as the developer, not as root, after install-packages.sh.
#
# Optional: MITCAD_GPU_ADAPTER=NVIDIA selects the GPU Mesa uses under WSLg;
# MITCAD_RENDER_CUDA=0 builds the renderer without its CUDA device.
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

# The Rust compiler cache, installed but off on Linux: in a new worktree it
# hits only the dependencies (the project's own crates have another path in
# each worktree), and it turns incremental compilation off, which slows the
# rebuilds of one worktree (measured on a Linux workstation, 27.10.2026).
# To use it: -DMITCAD_RUST_COMPILER_CACHE=ON -DMITCAD_SCCACHE=$HOME/.local/bin/sccache
# and MITCAD_SCCACHE_DIR on a large build disk rather than a small home.
echo "== sccache $SCCACHE_LINUX_VERSION"
if [ "$("$HOME/.local/bin/sccache" --version 2> /dev/null || true)" != "sccache $SCCACHE_LINUX_VERSION" ]; then
  name=sccache-v$SCCACHE_LINUX_VERSION-x86_64-unknown-linux-musl
  work=$(mktemp -d)
  curl -sSfL -o "$work/$name.tar.gz" \
    "https://github.com/mozilla/sccache/releases/download/v$SCCACHE_LINUX_VERSION/$name.tar.gz"
  echo "$SCCACHE_LINUX_X86_64_SHA256  $work/$name.tar.gz" | sha256sum -c --quiet
  tar -xzf "$work/$name.tar.gz" -C "$work"
  mkdir -p "$HOME/.local/bin"
  install -m 755 "$work/$name/sccache" "$HOME/.local/bin/sccache"
  rm -rf -- "${work:?}"
fi
"$HOME/.local/bin/sccache" --version

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
# The interface languages (mitcad#1460) need Qt's Linguist tools (lrelease,
# lupdate) and Qt's translations (qtbase_<code>.qm), which aqt's base
# install has (qttools, qttranslations).
for file in bin/lrelease bin/lupdate translations/qtbase_fi.qm; do
  [ -e "$HOME/Qt/$QT_VERSION/gcc_64/$file" ] || { echo "Qt $QT_VERSION has no $file" >&2; exit 1; }
done
# The Chromium Embedded Framework of the web browser component (Insert >
# Component from Web, mitcad#1579): the pinned distribution, named in the
# login profile as MITCAD_CEF_ROOT, which the build reads; its system
# libraries come from install-packages.sh.
echo "== CEF $CEF_VERSION"
CEF_ROOT=$("$SCRIPT_DIR/fetch-cef.sh")
touch "$HOME/.profile"
sed -i '/^export MITCAD_CEF_ROOT=/d' "$HOME/.profile"
echo "export MITCAD_CEF_ROOT=\"$CEF_ROOT\"" >> "$HOME/.profile"

# Every build has the render worker (docs/rendering.md "Building"): Cycles
# and Open Image Denoise, built once outside the build folders; re-running
# skips what is done. With Cycles' CUDA device and its kernels (mitcad#202),
# which the releases need; MITCAD_RENDER_CUDA=0 leaves them out on a
# development machine that does not want the kernels' build (about 10
# minutes).
CYCLES_ARGS=()
[ "${MITCAD_RENDER_CUDA:-1}" = 0 ] || CYCLES_ARGS+=(--cuda)
echo "== The render worker's libraries (build-cycles.sh ${CYCLES_ARGS[*]})"
"$SCRIPT_DIR/build-cycles.sh" "${CYCLES_ARGS[@]}"

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
