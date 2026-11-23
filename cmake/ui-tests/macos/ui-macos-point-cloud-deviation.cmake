# SPDX-License-Identifier: MIT
# Point clouds and Point Cloud Deviation (mitcad#2240): the shared steps of
# tools/ui-point-cloud-deviation-test.sh in the Qt frontend, a scan inserted
# beside a block, the deviation's statistics and coloured points, kept and
# saved.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-point-cloud-deviation
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-point-cloud-deviation-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-point-cloud-deviation")
  set_tests_properties(app.ui-macos-point-cloud-deviation PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
