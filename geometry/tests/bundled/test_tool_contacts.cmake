# SPDX-License-Identifier: MIT
# Contact queries of automatic operations, repeated on synthetic geometry.
target_sources(test_geometry_booleans PRIVATE tests/test_tool_contacts.cpp)
find_package(Threads REQUIRED)
target_link_libraries(test_geometry_booleans PRIVATE mitcad_geometry_io Threads::Threads)
add_test(NAME geometry.tool_contacts COMMAND test_geometry_booleans tool_contacts)
