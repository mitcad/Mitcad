# SPDX-License-Identifier: MIT
# Geometry analysis: physical properties, measurement, interference,
# sections and shape comparison with OCCT (no Qt).
add_library(mitcad_geometry_analysis STATIC
  src/common.cpp
  src/compare.cpp
  src/convert.hpp
  src/interference.cpp
  src/measure.cpp
  src/properties.cpp
  src/section.cpp
  include/mitcad/analysis/common.hpp
  include/mitcad/analysis/compare.hpp
  include/mitcad/analysis/interference.hpp
  include/mitcad/analysis/measure.hpp
  include/mitcad/analysis/properties.hpp
  include/mitcad/analysis/section.hpp)
target_include_directories(mitcad_geometry_analysis PUBLIC include)
# TKG3d is public for GProp_GProps (common.hpp's integrals).
target_link_libraries(mitcad_geometry_analysis
  PUBLIC TKernel TKMath TKBRep TKG3d
  PRIVATE TKG2d TKGeomBase TKGeomAlgo TKTopAlgo TKPrim TKBO mitcad_warnings)

if(BUILD_TESTING)
  add_executable(test_geometry_analysis tests/test_analysis.cpp)
  target_link_libraries(test_geometry_analysis PRIVATE mitcad_geometry_analysis TKG3d TKGeomBase
                                                       TKTopAlgo TKPrim TKBO TKFillet mitcad_warnings)
  add_test(NAME geometry.analysis COMMAND test_geometry_analysis)
endif()
