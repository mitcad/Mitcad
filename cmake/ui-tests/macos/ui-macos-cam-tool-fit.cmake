# SPDX-License-Identifier: MIT
# The milling panels' Tool (mitcad#2590): the shared steps of
# tools/ui-cam-tool-fit-test.sh in the Qt frontend, a saved pocket's ball
# end mill flagged and replaced, the flat end mill by default, the
# engraving cutter on a contour's line, a setup without an end mill
# refused at Tool.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-cam-tool-fit
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cam-tool-fit-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cam-tool-fit")
  set_tests_properties(app.ui-macos-cam-tool-fit PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
