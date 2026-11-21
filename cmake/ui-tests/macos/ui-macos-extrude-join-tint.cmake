# SPDX-License-Identifier: MIT
# An Extrude Join's target body in the command's look (mitcad#2069): the
# shared steps of tools/ui-extrude-join-tint-test.sh in the Qt frontend,
# the target tinted faintly and the added material blue.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-extrude-join-tint
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-extrude-join-tint-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-extrude-join-tint")
  set_tests_properties(app.ui-macos-extrude-join-tint PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
