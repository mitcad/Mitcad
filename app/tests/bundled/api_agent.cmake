# SPDX-License-Identifier: MIT
# An API agent's key across a change in Preferences while a message runs
# (mitcad#1309): ApiAgent against a local service and a stand-in of the
# live listener, keys for the run only.
target_sources(test_app_services PRIVATE tests/api_agent.cpp agent/ApiAgent.cpp agent/ApiAgent.hpp
  agent/ChatAgent.hpp agent/ChatSettings.cpp agent/ChatSettings.hpp mcp/LocalEndpoint.cpp mcp/LocalEndpoint.hpp)
if(WIN32)
  target_sources(test_app_services PRIVATE mcp/WindowsEndpointSecurity.cpp mcp/WindowsEndpointSecurity.hpp)
  target_link_libraries(test_app_services PRIVATE advapi32)
endif()
target_link_libraries(test_app_services PRIVATE mitcad_ffi Qt6::Core Qt6::Network qt6keychain)
add_test(NAME app.api_agent COMMAND test_app_services api_agent)
set_tests_properties(app.api_agent PROPERTIES TIMEOUT 120
  ENVIRONMENT "MITCAD_TEST_KEYCHAIN=off"
  ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
