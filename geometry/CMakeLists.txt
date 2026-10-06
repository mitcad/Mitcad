# SPDX-License-Identifier: MIT
# One source per operation family (see include/mitcad/geometry/geometry.hpp),
# so parallel work on different families touches different files.
add_library(mitcad_geometry STATIC
  src/boolean.cpp
  src/brep_import.cpp
  src/dressup.cpp
  src/extrude.cpp
  src/history.cpp
  src/history.hpp
  src/import.cpp
  src/naming.cpp
  src/profile.cpp
  src/query.cpp
  src/shape.cpp
  src/util.cpp
  src/util.hpp
  src/datum.cpp
  include/mitcad/geometry/boolean.hpp
  include/mitcad/geometry/brep_import.hpp
  include/mitcad/geometry/dressup.hpp
  include/mitcad/geometry/extrude.hpp
  include/mitcad/geometry/geometry.hpp
  include/mitcad/geometry/import.hpp
  include/mitcad/geometry/naming.hpp
  include/mitcad/geometry/profile.hpp
  include/mitcad/geometry/query.hpp
  include/mitcad/geometry/shape.hpp
  include/mitcad/geometry/datum.hpp
  # Face operations: shell, draft, offset, delete, replace, split (F2).
  src/face_select.cpp
  src/face_select.hpp
  src/faceops.cpp
  src/replace.cpp
  src/replace.hpp
  src/split.cpp
  src/tool.cpp
  include/mitcad/geometry/faceops.hpp
  include/mitcad/geometry/split.hpp
  include/mitcad/geometry/tool.hpp)
# Transforms, patterns, mirrors, combine and primitives (F4).
target_sources(mitcad_geometry PRIVATE
  src/pattern.cpp
  src/primitive.cpp
  src/transform.cpp
  include/mitcad/geometry/pattern.hpp
  include/mitcad/geometry/primitive.hpp
  include/mitcad/geometry/transform.hpp)
# Profile features (F1): extrude options, revolve, hole, thread.
target_sources(mitcad_geometry PRIVATE
  src/hole.cpp
  src/reference.cpp
  src/revolve.cpp
  src/sweep.cpp
  src/sweep.hpp
  src/thread.cpp
  include/mitcad/geometry/hole.hpp
  include/mitcad/geometry/reference.hpp
  include/mitcad/geometry/revolve.hpp
  include/mitcad/geometry/target.hpp
  include/mitcad/geometry/thread.hpp)
# Sketch planes and projected geometry (S1).
target_sources(mitcad_geometry PRIVATE
  src/sketch_ref.cpp
  include/mitcad/geometry/sketch_ref.hpp)
# Crashes inside OCCT as errors (.f3d import, T1).
target_sources(mitcad_geometry PRIVATE
  src/guard.cpp
  include/mitcad/geometry/guard.hpp)
# Sweeps, lofts, pipes, coils, ribs and webs (F3).
target_sources(mitcad_geometry PRIVATE
  src/loft.cpp
  src/path_sweep.cpp
  src/rib.cpp
  src/spine.cpp
  src/spine.hpp
  include/mitcad/geometry/loft.hpp
  include/mitcad/geometry/path_sweep.hpp
  include/mitcad/geometry/rib.hpp)
# Lofts with end conditions and rails (P2), and the curve and surface
# fitting they share with other features.
target_sources(mitcad_geometry PRIVATE
  src/loft_skin.cpp
  src/loft_skin.hpp
  src/skin.cpp
  src/skin.hpp)
# Fillet and chamfer options (P6): asymmetric and G2 fillets from a chamfer
# whose bevels become blend surfaces; setback and blend corner patches.
target_sources(mitcad_geometry PRIVATE
  src/blend.cpp
  src/blend.hpp
  src/corner.cpp
  src/corner.hpp)
# Sketch text (P3): glyph outlines through OCCT's fonts, with the bundled
# font (third_party/fonts/droid-sans) embedded in the library.
find_package(Freetype REQUIRED)
set(mitcad_font_dir "${PROJECT_SOURCE_DIR}/third_party/fonts/droid-sans")
add_custom_command(
  OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/embedded_fonts.cpp"
  COMMAND "${CMAKE_COMMAND}" "-DOUTPUT=${CMAKE_CURRENT_BINARY_DIR}/embedded_fonts.cpp"
          "-DNAMESPACE=mitcad::geometry::fonts" "-DNAMES=droid_sans,droid_sans_bold"
          "-DPATHS=${mitcad_font_dir}/DroidSans.ttf,${mitcad_font_dir}/DroidSans-Bold.ttf"
          -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/embed.cmake"
  DEPENDS cmake/embed.cmake "${mitcad_font_dir}/DroidSans.ttf" "${mitcad_font_dir}/DroidSans-Bold.ttf"
  COMMENT "Embedding the bundled font"
  VERBATIM)
target_sources(mitcad_geometry PRIVATE
  src/text.cpp
  include/mitcad/geometry/text.hpp
  "${CMAKE_CURRENT_BINARY_DIR}/embedded_fonts.cpp")
target_link_libraries(mitcad_geometry PUBLIC TKService PRIVATE Freetype::Freetype)
# Shapes with their names as bytes, for the store of computed results (P7d).
target_sources(mitcad_geometry PRIVATE
  src/persist.cpp
  include/mitcad/geometry/persist.hpp)
set_property(SOURCE src/persist.cpp APPEND PROPERTY COMPILE_DEFINITIONS "MITCAD_VERSION=\"${PROJECT_VERSION}\"")
# Long operations stopped on request (P7e).
target_sources(mitcad_geometry PRIVATE
  src/cancel.cpp
  include/mitcad/geometry/cancel.hpp)
# Operations leave their inputs as they are: B-rep fingerprints and the
# input check (T0e).
target_sources(mitcad_geometry PRIVATE
  src/input_check.cpp
  include/mitcad/geometry/input_check.hpp)
target_include_directories(mitcad_geometry PUBLIC include)
# Volumes and areas come from the analysis library's integrals, so that
# every reported volume agrees (query.cpp, brep_import.cpp).
target_link_libraries(mitcad_geometry
  PUBLIC TKernel TKMath TKG2d TKG3d TKGeomBase TKBRep TKTopAlgo TKPrim TKBO TKFillet TKGeomAlgo TKShHealing
         TKBool TKOffset TKFeat
  PRIVATE mitcad_geometry_analysis mitcad_warnings)

if(BUILD_TESTING)
  add_executable(test_geometry
    tests/check.hpp
    tests/test_boolean.cpp
    tests/test_dressup.cpp
    tests/test_extrude.cpp
    tests/test_faceops.cpp
    tests/test_split.cpp
    tests/test_main.cpp
    tests/test_naming.cpp
    tests/test_profile.cpp
    tests/test_query.cpp
    tests/test_datum.cpp)
  # Transforms, patterns, mirrors, combine and primitives (F4).
  target_sources(test_geometry PRIVATE tests/test_transform.cpp)
  target_sources(test_geometry PRIVATE
    tests/test_extrude_feature.cpp
    tests/test_hole.cpp
    tests/test_revolve.cpp
    tests/test_thread.cpp)
  # Sweeps, lofts, pipes, coils, ribs and webs (F3).
  target_sources(test_geometry PRIVATE tests/test_sweeps.cpp)
  # Sketch text (P3).
  target_sources(test_geometry PRIVATE tests/test_text.cpp)
  # Lofts with end conditions and rails, curve and surface fitting (P2).
  target_sources(test_geometry PRIVATE tests/test_skin.cpp)
  # Shapes with their names as bytes (P7d).
  target_sources(test_geometry PRIVATE tests/test_persist.cpp)
  # Long operations stopped on request (P7e).
  target_sources(test_geometry PRIVATE tests/test_cancel.cpp)
  # Operations leave their inputs as they are (T0e), the analyses too.
  target_sources(test_geometry PRIVATE tests/test_input_check.cpp)
  target_link_libraries(test_geometry PRIVATE mitcad_geometry_analysis)
  target_link_libraries(test_geometry PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry COMMAND test_geometry)
endif()

# Data exchange and analysis services, separate libraries on top of OCCT.
add_subdirectory(io)
add_subdirectory(analysis)
