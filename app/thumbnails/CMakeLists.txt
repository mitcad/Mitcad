# SPDX-License-Identifier: MIT
# Windows Explorer's thumbnails of MitCad designs (mitcad#1695): the
# thumbnail handler mitcad-thumbnails.dll, an in-process COM server
# (ThumbnailHandler.cpp) that reads a .mitcad file's preview through the
# thumbnailer's C ABI (the static library mitcad_thumbnailer_capi,
# core/thumbnailer-capi). The installer puts it in bin and registers it
# for .mitcad (cmake/Packaging.cmake, packaging/windows/filetype.nsh).
# Windows only: Linux has mitcad-thumbnailer's freedesktop entry
# (mitcad#1539), macOS nothing yet.
if(NOT WIN32)
  return()
endif()

add_library(mitcad-thumbnails MODULE
  ThumbnailHandler.cpp
  ThumbnailHandler.hpp
  mitcad-thumbnails.def)
target_link_libraries(mitcad-thumbnails PRIVATE mitcad_thumbnailer_capi mitcad_warnings ole32 gdi32)
target_include_directories(mitcad-thumbnails PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
set_target_properties(mitcad-thumbnails PROPERTIES PREFIX "")
# Built with the application, beside it in the build folder.
add_dependencies(mitcad mitcad-thumbnails)

# The handler as Explorer calls it (tests/thumbnail_handler_windows.cpp):
# loaded from the build folder, on a design saved with a preview, one
# without (as before MitCad kept them) and a file that is no design. The
# installer test runs the same program with --registered against the
# installed handler (tools/installer-windows-test.ps1).
if(BUILD_TESTING)
  add_executable(test_thumbnail_handler "${PROJECT_SOURCE_DIR}/app/tests/thumbnail_handler_windows.cpp")
  target_link_libraries(test_thumbnail_handler PRIVATE mitcad_warnings ole32 shlwapi gdi32 advapi32)
  target_include_directories(test_thumbnail_handler PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  # The C++ runtime linked in, so that the installer test runs it with a
  # PATH of the system's only.
  set_target_properties(test_thumbnail_handler PROPERTIES MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
  add_dependencies(test_thumbnail_handler mitcad-thumbnails)
  add_test(NAME app.thumbnails-windows
    COMMAND test_thumbnail_handler --dll "$<TARGET_FILE:mitcad-thumbnails>"
            --work "${CMAKE_CURRENT_BINARY_DIR}/thumbnail-handler-test")
  set_tests_properties(app.thumbnails-windows PROPERTIES TIMEOUT 120)
endif()
