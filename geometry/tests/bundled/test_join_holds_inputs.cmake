# SPDX-License-Identifier: MIT
# A union holds every shape it joins (mitcad#2053): the check of a point
# inside each input on candidates that miss inputs and on unions that
# hold them all, and the reported Join of a hub with 30 bodies (GitHub
# mitcad/Mitcad#463, `github-463/` under MITCAD_MITCAD_REPORTED, its STEP
# file unpacked into the build folder; skipped without it).
find_package(ZLIB REQUIRED)
target_sources(test_geometry_booleans PRIVATE tests/test_join_holds_inputs.cpp)
target_link_libraries(test_geometry_booleans PRIVATE mitcad_geometry_io ZLIB::ZLIB)
add_test(NAME geometry.join_holds_inputs
  COMMAND test_geometry_booleans join_holds_inputs "${CMAKE_CURRENT_BINARY_DIR}/join-holds-inputs")
set_tests_properties(geometry.join_holds_inputs PROPERTIES TIMEOUT 600)
