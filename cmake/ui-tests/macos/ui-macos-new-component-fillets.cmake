# SPDX-License-Identifier: MIT
# An extrude edited from Join to New Component takes its fillet along
# (mitcad#2185): the shared steps of tools/ui-new-component-fillets-test.sh
# in the Qt frontend, no feature failing and the timeline marking the
# extrude and the fillet in the new component's colour.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-new-component-fillets
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-new-component-fillets-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-new-component-fillets")
  set_tests_properties(app.ui-macos-new-component-fillets PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
