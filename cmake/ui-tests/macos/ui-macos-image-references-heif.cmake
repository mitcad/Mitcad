# SPDX-License-Identifier: MIT
# HEIF image references through macOS' ImageIO (mitcad#1304): a HEIF file
# inserted upright and embedded as a PNG file, drawn again with no HEIF
# decoder after the file is gone.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND BASH_EXECUTABLE AND TARGET mitcad AND TARGET mitcad-cli)
  add_test(NAME app.ui-macos-image-references-heif
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-image-references-heif-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-image-references-heif")
  set_tests_properties(app.ui-macos-image-references-heif PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
