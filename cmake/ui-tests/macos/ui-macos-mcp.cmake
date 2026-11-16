# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND Python3_Interpreter_FOUND
   AND TARGET mitcad-cli)
  # The Qt frontend in every build (the release's Qt suite, mitcad#1056),
  # the AppKit frontend too where it is built.
  set(mitcad_mcp_test_uis qt)
  if(MITCAD_MACOS_NATIVE_UI)
    list(APPEND mitcad_mcp_test_uis native)
  endif()
  foreach(mitcad_mcp_test_ui IN LISTS mitcad_mcp_test_uis)
    # The output folder is the test's name without "app.", as every other
    # ui-macos-* folder (mitcad#1151).
    if(mitcad_mcp_test_ui STREQUAL "native")
      set(mitcad_mcp_folder ui-macos-native-mcp)
    else()
      set(mitcad_mcp_folder ui-macos-mcp)
    endif()
    set(mitcad_mcp_test app.${mitcad_mcp_folder})
    add_test(NAME ${mitcad_mcp_test}
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-mcp-test.py"
              "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/${mitcad_mcp_folder}"
              "${mitcad_mcp_test_ui}")
    # With the live debugging flow (mitcad#265, tools/ui-mcp-live.py).
    set_tests_properties(${mitcad_mcp_test} PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
  endforeach()
endif()
