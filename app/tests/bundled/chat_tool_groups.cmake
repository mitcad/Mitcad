# SPDX-License-Identifier: MIT
# The tool calls' group rows (mitcad#983) as the panel asks for them, the
# chat's Rust logic through the bridge.
target_sources(test_app_services PRIVATE tests/chat_tool_groups.cpp agent/ChatLogic.hpp)
target_link_libraries(test_app_services PRIVATE mitcad_ffi Qt6::Core)
add_test(NAME app.chat_tool_groups COMMAND test_app_services chat_tool_groups)
set_tests_properties(app.chat_tool_groups PROPERTIES
  ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
