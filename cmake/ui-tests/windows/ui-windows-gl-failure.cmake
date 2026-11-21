# SPDX-License-Identifier: MIT
# The 3D view's attach to the graphics card that fails at the first open,
# after a move to a new window, by an exception, or after a screen change
# with Component from Web's window open never ends MitCad: the message
# once, the view drawn again once the test switch is cleared, the design
# saved, a clean exit without a crash report (mitcad#2125); the Linux
# test's shared input scripts. The screen change runs with the browser
# window where the build made the web browser component
# (app/webbrowser/CMakeLists.txt), without it elsewhere.
get_property(gl_failure_web_browser GLOBAL PROPERTY MITCAD_BROWSER_COMPONENT_DIR)
set(gl_failure_web_arguments "")
if(gl_failure_web_browser)
  set(gl_failure_web_arguments -WebBrowser "${gl_failure_web_browser}")
endif()
add_test(NAME app.ui-windows-gl-failure
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-gl-failure-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-gl-failure"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>" ${gl_failure_web_arguments})
set_tests_properties(app.ui-windows-gl-failure PROPERTIES TIMEOUT 900)
