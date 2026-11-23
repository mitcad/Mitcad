# SPDX-License-Identifier: MIT
# A lathe made in Machines and Tools… (mitcad#2230): the shared steps of
# tools/ui-cam-lathe-machine-test.sh in the Qt frontend, New Machine… >
# Lathe, its Edit Machine…, a turning tool and a parting tool, a lathe
# setup, Turn Facing and Generate.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-cam-lathe-machine
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cam-lathe-machine-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cam-lathe-machine")
  set_tests_properties(app.ui-macos-cam-lathe-machine PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
