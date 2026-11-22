# SPDX-License-Identifier: MIT
# The build notices a changed overlay port by itself (mitcad#2134).
#
# vcpkg's toolchain installs the manifest in every configure: `vcpkg
# install` hashes every file of an overlay port's folder (portfile, vcpkg.json,
# patches), so a changed OCCT patch gives the port a new ABI hash, and the
# install rebuilds the port and replaces it in `<build>/vcpkg_installed`.
# The new headers and libraries then rebuild and relink what uses them
# (Ninja's depfiles and the imported targets' library files). The toolchain
# makes only `vcpkg.json` and `vcpkg-configuration.json` configure
# dependencies, though, so `cmake --build` alone (no configure first)
# did not run the install after a port change and kept building against
# the OCCT installed before. Adding the overlay ports' files (the folders
# with a `portfile.cmake` under third_party/vcpkg-ports, vcpkg.json's
# "overlay-ports") and the overlay triplets (VCPKG_OVERLAY_TRIPLETS) here
# as configure dependencies makes such a build configure again first, and
# with that run vcpkg's install. A fresh configure (`cmake --fresh`) is not
# needed for a port change. The patches' descriptions under
# third_party/vcpkg-ports/docs are outside the port folders and are not
# dependencies: vcpkg does not read them.
#
# Included from the top-level CMakeLists.txt after project(), where the
# toolchain has set VCPKG_MANIFEST_MODE and VCPKG_MANIFEST_INSTALL.
if(VCPKG_MANIFEST_MODE AND VCPKG_MANIFEST_INSTALL)
  set(mitcad_vcpkg_port_inputs "")
  # The globs are CONFIGURE_DEPENDS too, so that a file added to or removed
  # from a port folder configures again as well.
  file(GLOB mitcad_vcpkg_portfiles CONFIGURE_DEPENDS
    "${PROJECT_SOURCE_DIR}/third_party/vcpkg-ports/*/portfile.cmake")
  foreach(mitcad_portfile IN LISTS mitcad_vcpkg_portfiles)
    get_filename_component(mitcad_port_dir "${mitcad_portfile}" DIRECTORY)
    file(GLOB_RECURSE mitcad_port_files CONFIGURE_DEPENDS LIST_DIRECTORIES false
      "${mitcad_port_dir}/*")
    list(APPEND mitcad_vcpkg_port_inputs ${mitcad_port_files})
  endforeach()
  foreach(mitcad_triplet_dir IN LISTS VCPKG_OVERLAY_TRIPLETS)
    file(GLOB mitcad_triplet_files CONFIGURE_DEPENDS "${mitcad_triplet_dir}/*.cmake")
    list(APPEND mitcad_vcpkg_port_inputs ${mitcad_triplet_files})
  endforeach()
  set_property(DIRECTORY "${PROJECT_SOURCE_DIR}" APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    ${mitcad_vcpkg_port_inputs})
endif()
