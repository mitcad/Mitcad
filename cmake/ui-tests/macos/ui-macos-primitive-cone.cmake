# SPDX-License-Identifier: MIT
# Cone (mitcad#2239): a cone placed on XY and made a frustum, its volumes
# and faces checked; the shared steps of tools/ui-primitive-cone-test.sh in
# the Qt frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-primitive-cone
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-primitive-cone-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-primitive-cone")
  set_tests_properties(app.ui-macos-primitive-cone PROPERTIES
    TIMEOUT 180 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
