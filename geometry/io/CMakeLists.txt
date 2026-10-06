# SPDX-License-Identifier: MIT
# Data exchange: STEP, IGES, STL and OBJ with OCCT (no Qt).
add_library(mitcad_geometry_io STATIC
  src/exchange.cpp
  src/iges.cpp
  src/mesh.cpp
  src/obj.cpp
  src/step.cpp
  src/stl.cpp
  src/xcaf.cpp
  src/xcaf.hpp
  include/mitcad/io/body.hpp
  include/mitcad/io/exchange.hpp
  include/mitcad/io/iges.hpp
  include/mitcad/io/mesh.hpp
  include/mitcad/io/obj.hpp
  include/mitcad/io/step.hpp
  include/mitcad/io/stl.hpp)
target_include_directories(mitcad_geometry_io PUBLIC include)
target_link_libraries(mitcad_geometry_io
  PUBLIC TKernel TKMath TKG3d TKBRep TKMesh
  PRIVATE TKTopAlgo TKShHealing TKXSBase TKDE TKDESTEP TKDEIGES TKDESTL TKDEOBJ TKRWMesh
          TKLCAF TKCAF TKXCAF mitcad_warnings)

if(BUILD_TESTING)
  add_executable(test_geometry_io tests/test_io.cpp)
  target_link_libraries(test_geometry_io PRIVATE mitcad_geometry_io TKTopAlgo TKPrim TKBO TKFillet
                                                 mitcad_warnings)
  add_test(NAME geometry.io COMMAND test_geometry_io "${CMAKE_CURRENT_BINARY_DIR}/test-files")
endif()
