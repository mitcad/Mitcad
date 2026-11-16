# SPDX-License-Identifier: MIT
# A fillet's short tangent chain is a remark, not a warning (mitcad#1914):
# the shared steps of tools/ui-fillet-chain-remark-test.sh in the Qt
# frontend, on the reported design of the private test files
# (MITCAD_MITCAD_REPORTED); skipped without it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-fillet-chain-remark
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-fillet-chain-remark-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-fillet-chain-remark")
  set_tests_properties(app.ui-macos-fillet-chain-remark PROPERTIES
    TIMEOUT 1260 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
