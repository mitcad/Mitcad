# SPDX-License-Identifier: MIT
# Point pairs on bodies of a placed component (mitcad#2566): the shared
# steps of tools/ui-point-cloud-pairs-placed-test.sh in the Qt frontend, a
# scan and a mesh aligned onto a block of a component placed turned and
# moved by three point pairs clicked in the view and the best fit.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-point-cloud-pairs-placed
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-point-cloud-pairs-placed-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-point-cloud-pairs-placed")
  set_tests_properties(app.ui-macos-point-cloud-pairs-placed PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
