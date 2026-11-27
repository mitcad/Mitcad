# SPDX-License-Identifier: MIT
# The first use with a local manifest (mitcad#1579): a package and a manifest
# signed by mitcad-release with a test key, checked, verified, unpacked and
# installed through the bridge; a tampered package and another interface
# refused.
if(TARGET mitcad-release)
  target_sources(test_app_services PRIVATE tests/browser_component_flow.cpp webbrowser/BrowserComponent.cpp
    webbrowser/BrowserComponent.hpp webbrowser/BrowserComponentInstall.cpp webbrowser/BrowserComponentInstall.hpp)
  target_link_libraries(test_app_services PRIVATE Qt6::Core Qt6::Gui mitcad_ffi)
  add_dependencies(test_app_services mitcad-release)
  add_test(NAME app.browser_component_flow
    COMMAND test_app_services browser_component_flow "$<TARGET_FILE:mitcad-release>")
  set_tests_properties(app.browser_component_flow PROPERTIES TIMEOUT 120
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()
