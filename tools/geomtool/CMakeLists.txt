# SPDX-License-Identifier: MIT
# Command line tool for data exchange and geometry analysis.
add_executable(mitcad-geomtool main.cpp)
target_link_libraries(mitcad-geomtool PRIVATE mitcad_geometry_io mitcad_geometry_analysis
                                              TKG3d TKTopAlgo TKPrim TKBO TKFillet mitcad_warnings)

if(BUILD_TESTING)
  # Drives the tool on files it generates (tests/geomtool-test.cmake).
  add_test(NAME tools.geomtool
    COMMAND "${CMAKE_COMMAND}" "-DTOOL=$<TARGET_FILE:mitcad-geomtool>"
            "-DDIR=${CMAKE_CURRENT_BINARY_DIR}/test-files"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/tests/geomtool-test.cmake")
endif()
