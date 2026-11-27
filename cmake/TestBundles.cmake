# SPDX-License-Identifier: MIT
# C++ test bundles (mitcad#2497): the C++ tests that were programs of their
# own, linked a few tens at a time into one program per subject, as
# test_geometry and test_port_patches are. Each test file registers its
# entry point by name (tools/test-bundle/bundled_test.hpp) and each ctest
# runs `<bundle> <name> [arguments]`: one test in a process of its own, so
# tests share no state (the kernel's globals, the environment, the working
# directory) and a crash ends only its own test. A bundle links the same
# static libraries as the application.
#
# A test of a bundle is registered in a file of its own,
# <folder>/<test file's stem>.cmake (geometry/tests/bundled, app/tests/bundled,
# core/tests/bundled), which adds the source to the bundle and its ctests:
#
#   # What it checks (mitcad#NNNN).
#   target_sources(test_geometry_sheet PRIVATE tests/test_sheet_hem.cpp)
#   add_test(NAME geometry.sheet_hem COMMAND test_geometry_sheet sheet_hem)
#
# so that a new test edits no shared list.

set(MITCAD_TEST_BUNDLE_DIR "${PROJECT_SOURCE_DIR}/tools/test-bundle")

# mitcad_add_test_bundle(<target>): a bundle's program, its tests added by
# the files mitcad_include_bundled_tests includes.
function(mitcad_add_test_bundle target)
  add_executable(${target} "${MITCAD_TEST_BUNDLE_DIR}/bundle_main.cpp" "${MITCAD_TEST_BUNDLE_DIR}/bundled_test.hpp")
  target_include_directories(${target} PRIVATE "${MITCAD_TEST_BUNDLE_DIR}")
  target_link_libraries(${target} PRIVATE mitcad_warnings)
endfunction()

# mitcad_include_bundled_tests(<folder>): includes every <folder>/*.cmake in
# sorted order, in the calling directory's scope (a macro, so that the files
# see and set its variables as the lists they replace did).
macro(mitcad_include_bundled_tests folder)
  file(GLOB mitcad_bundled_test_files CONFIGURE_DEPENDS "${folder}/*.cmake")
  list(SORT mitcad_bundled_test_files)
  foreach(mitcad_bundled_test_file IN LISTS mitcad_bundled_test_files)
    include("${mitcad_bundled_test_file}")
  endforeach()
endmacro()
