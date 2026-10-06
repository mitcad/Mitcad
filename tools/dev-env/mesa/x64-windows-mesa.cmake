# SPDX-License-Identifier: MIT
# vcpkg triplet for tools/dev-env/setup-mesa-windows.ps1.
#
# Release only (an LLVM debug build would need tens of gigabytes). Static
# libraries with the dynamic CRT, so Mesa's opengl32.dll and
# libgallium_wgl.dll carry LLVM, zlib and zstd inside and need no other DLLs
# next to mitcad.exe.
set(VCPKG_TARGET_ARCHITECTURE x64)
set(VCPKG_CRT_LINKAGE dynamic)
set(VCPKG_LIBRARY_LINKAGE static)
set(VCPKG_BUILD_TYPE release)

if(PORT STREQUAL "mesa")
  # Only the software rasterizers. Without a GPU the D3D12 driver would run
  # on WARP, and Zink and the Vulkan drivers are not needed.
  set(VCPKG_MESON_CONFIGURE_OPTIONS "-Dgallium-drivers=llvmpipe,softpipe" "-Dvulkan-drivers=")
endif()
