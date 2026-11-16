# SPDX-License-Identifier: MIT
# glTF export and import (mitcad#1826): the shared steps of
# tools/ui-gltf-test.sh in the Qt frontend, File > Export to .glb and .gltf
# read back by MitCad's own reader, File > Import and Open of both.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-gltf
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-gltf-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-gltf")
  set_tests_properties(app.ui-macos-gltf PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
