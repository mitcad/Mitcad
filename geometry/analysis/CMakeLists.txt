# SPDX-License-Identifier: MIT
# Geometry analysis: physical properties, measurement, interference,
# sections and shape comparison with OCCT (no Qt).
add_library(mitcad_geometry_analysis STATIC
  src/common.cpp
  src/compare.cpp
  src/convert.hpp
  src/face_integral.cpp
  src/face_integral.hpp
  src/interference.cpp
  src/measure.cpp
  src/mesh_section.cpp
  src/mesh_section.hpp
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
  # A test of the geometry's base bundle (geometry/CMakeLists.txt, mitcad#2497).
  target_sources(test_geometry_base PRIVATE tests/test_analysis.cpp)
  target_link_libraries(test_geometry_base PRIVATE mitcad_geometry_analysis TKG3d TKGeomBase TKGeomAlgo TKTopAlgo
                                                   TKPrim TKBO TKFillet)
  add_test(NAME geometry.analysis COMMAND test_geometry_base geometry_analysis)
endif()
