# SPDX-License-Identifier: MIT
# The overlay layer's fallback (mitcad#2406): the steps
# tools/ui-scripts/paint-layer-fallback.mitcad-ui shares with the Linux test
# and app.ui-windows-paint-layer-fallback, in the Qt frontend: with the
# layer's failure forced, sketch mode's overlay drawn as a widget over the
# view, the log saying why.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli
   AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-paint-layer-fallback
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-paint-layer-fallback-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>"
            "${CMAKE_BINARY_DIR}/ui-macos-paint-layer-fallback")
  set_tests_properties(app.ui-macos-paint-layer-fallback PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
