# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND Python3_Interpreter_FOUND
   AND TARGET mitcad-cli AND TARGET mitcad-render)
  add_test(NAME app.render_metal_scenes
    COMMAND "${Python3_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/render-metal-acceptance.py"
            "$<TARGET_FILE:mitcad-cli>" "$<TARGET_FILE:mitcad-render>"
            "${CMAKE_BINARY_DIR}/render-metal-scenes")
  set_tests_properties(app.render_metal_scenes PROPERTIES
    TIMEOUT 1200 SKIP_RETURN_CODE 77 RUN_SERIAL TRUE ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
endif()
