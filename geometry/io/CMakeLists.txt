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
# Shaded meshes of faces (USDZ export, mitcad#123).
target_sources(mitcad_geometry_io PRIVATE
  src/face_mesh.cpp
  include/mitcad/io/face_mesh.hpp)
# Two display meshings compared face by face (mitcad-cli mesh-compare, mitcad#644).
target_sources(mitcad_geometry_io PRIVATE
  src/mesh_compare.cpp
  include/mitcad/io/mesh_compare.hpp)
# Display triangulations kept in the result store (mitcad#858).
target_sources(mitcad_geometry_io PRIVATE
  src/mesh_store.cpp
  include/mitcad/io/mesh_store.hpp)
# STEP names in raw bytes of another code page than UTF-8 (mitcad#1002).
target_sources(mitcad_geometry_io PRIVATE
  src/step_names.cpp
  src/step_names.hpp)
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
