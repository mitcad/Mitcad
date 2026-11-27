# SPDX-License-Identifier: MIT
# Unions of many bodies group by group, and a union that lost material
# tried with a fuzzy value, on the reported design of GitHub #133 read
# from MITCAD_MITCAD_REPORTED (its B-rep data zlib-compressed, mitcad#1273).
find_package(ZLIB REQUIRED)
target_sources(test_geometry_booleans PRIVATE tests/test_join_many_bodies.cpp)
target_link_libraries(test_geometry_booleans PRIVATE ZLIB::ZLIB)
add_test(NAME geometry.join_many_bodies COMMAND test_geometry_booleans join_many_bodies)
set_tests_properties(geometry.join_many_bodies PROPERTIES TIMEOUT 600)
