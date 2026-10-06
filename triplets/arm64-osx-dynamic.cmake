# SPDX-License-Identifier: MIT
# vcpkg's community triplet of the same name, plus the deployment target:
# the community one leaves it to the SDK's default, so OCCT's dylibs could
# require a newer macOS than the app. CMakePresets.json sets the same 14.4,
# the minimum of the Qt 6.12 binaries.
# Dynamic libraries because OCCT is LGPL (docs/development.md, licence policy).
set(VCPKG_TARGET_ARCHITECTURE arm64)
set(VCPKG_CRT_LINKAGE dynamic)
set(VCPKG_LIBRARY_LINKAGE dynamic)
set(VCPKG_CMAKE_SYSTEM_NAME Darwin)
set(VCPKG_OSX_ARCHITECTURES arm64)
set(VCPKG_OSX_DEPLOYMENT_TARGET 14.4)
