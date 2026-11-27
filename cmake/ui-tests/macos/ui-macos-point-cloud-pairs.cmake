# SPDX-License-Identifier: MIT
# Point pairs and the crop box of the point cloud tools (mitcad#2565): the
# shared steps of tools/ui-point-cloud-pairs-test.sh in the Qt frontend, a
# scan aligned onto a block by three point pairs clicked in the view and
# the best fit, then cropped by a box.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-point-cloud-pairs
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-point-cloud-pairs-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-point-cloud-pairs")
  set_tests_properties(app.ui-macos-point-cloud-pairs PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
