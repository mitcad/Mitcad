# SPDX-License-Identifier: MIT
# A member placed with Timber is made of its library's wood (mitcad#1817):
# the shared steps of tools/ui-timber-material-test.sh in the Qt frontend,
# Physical Properties showing Spruce and the mass its density gives.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-timber-material
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-timber-material-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-timber-material")
  set_tests_properties(app.ui-macos-timber-material PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
