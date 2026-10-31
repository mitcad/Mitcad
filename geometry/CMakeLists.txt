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
  src/local_boolean.cpp
  src/local_boolean.hpp
  src/thread_options.hpp
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
# Booleans on perforated bodies: holes away from the tool sit out (mitcad#72).
target_sources(mitcad_geometry PRIVATE
  src/far_features.cpp
  src/far_features.hpp)
# The material a near copy of a body lacks, cut only where the two differ
# (the import's history-based guesses, mitcad#85).
target_sources(mitcad_geometry PRIVATE
  src/removed.cpp
  include/mitcad/geometry/removed.hpp)
# Roundings that run over cut-outs of their faces: the cut-outs filled and
# cut out again (mitcad#311).
target_sources(mitcad_geometry PRIVATE
  src/cutouts.cpp
  src/cutouts.hpp)
# Fillets and chamfers of whole circles that run over a neighbouring face,
# built as rings (mitcad#60).
target_sources(mitcad_geometry PRIVATE
  src/ring_dressup.cpp
  src/ring_dressup.hpp)
# Sheet material construction (mitcad#206).
target_sources(mitcad_geometry PRIVATE
  src/sheet.cpp
  src/sheet_chart.cpp
  src/sheet_curve.cpp
  src/sheet_curve.hpp
  src/sheet_trim.cpp
  include/mitcad/geometry/sheet_domain.hpp
  include/mitcad/geometry/sheet_trim.hpp
  include/mitcad/geometry/sheet.hpp)
# The faces a run of features made, as solids a pattern copies (its
# optimized compute option).
target_sources(mitcad_geometry PRIVATE
  src/face_copy.cpp
  include/mitcad/geometry/face_copy.hpp)
# The tangent chains of a body's edges and faces, for the hover (mitcad#290).
target_sources(mitcad_geometry PRIVATE
  src/tangent_chain.cpp
  include/mitcad/geometry/tangent_chain.hpp)
# How a new feature's tool lies against bodies: Extrude's automatic
# operation (mitcad#289).
target_sources(mitcad_geometry PRIVATE
  src/contact.cpp
  include/mitcad/geometry/contact.hpp)
# The analytic surfaces and curves of a dressup's walked B-spline faces
# (mitcad#328).
target_sources(mitcad_geometry PRIVATE
  src/canonical.cpp
  src/canonical.hpp)
# BUILD: the face names of derived parts (mitcad#375).
target_sources(mitcad_geometry PRIVATE
  src/build.cpp
  include/mitcad/geometry/build.hpp)
# Helices built as B-reps whose faces run all the turns, and threads whose
# turns meet in one edge (mitcad#235).
target_sources(mitcad_geometry PRIVATE
  src/helix_brep.cpp
  src/helix_brep.hpp)
# Contact sets: bodies that stop each other while joints move (mitcad#454).
target_sources(mitcad_geometry PRIVATE
  src/motion_contact.cpp
  include/mitcad/geometry/motion_contact.hpp)
# Emboss: profiles wrapped onto cylinders (mitcad#458).
target_sources(mitcad_geometry PRIVATE
  src/emboss.cpp
  include/mitcad/geometry/emboss.hpp)
# Boundary Fill: the cells tool bodies enclose (mitcad#457).
target_sources(mitcad_geometry PRIVATE
  src/boundary_fill.cpp
  include/mitcad/geometry/boundary_fill.hpp)
# Patch, Stitch and Thicken: surface bodies filled, sewn and made solid (mitcad#456).
target_sources(mitcad_geometry PRIVATE
  src/surfaces.cpp
  include/mitcad/geometry/surfaces.hpp)
# Surface modelling: surface bodies and their features (mitcad#455).
target_sources(mitcad_geometry PRIVATE
  src/surface_sweeps.cpp
  include/mitcad/geometry/surface_sweeps.hpp)
# Surface modelling: Offset Surface, Ruled Surface and Extend (mitcad#455).
target_sources(mitcad_geometry PRIVATE
  src/free_loops.hpp
  src/surface_offsets.cpp
  include/mitcad/geometry/surface_offsets.hpp)
# Surface modelling: Trim, Untrim, Unstitch and Reverse Normal (mitcad#455).
target_sources(mitcad_geometry PRIVATE
  src/surface_edits.cpp
  include/mitcad/geometry/surface_edits.hpp)
# Plastic part features: the bands of lips and grooves (mitcad#459).
target_sources(mitcad_geometry PRIVATE
  src/plastic.cpp
  include/mitcad/geometry/plastic.hpp)
# Surface modelling: Blend, Boundary, Match and Fair Surface (mitcad#455).
target_sources(mitcad_geometry PRIVATE
  src/sheet_edges.hpp
  src/surface_fills.cpp
  include/mitcad/geometry/surface_fills.hpp)
# Drawings: views with hidden lines removed and round holes (mitcad#471).
target_sources(mitcad_geometry PRIVATE
  src/drawing.cpp
  include/mitcad/geometry/drawing.hpp)
target_link_libraries(mitcad_geometry PRIVATE TKHLR TKMesh)

# Gears: toothed profiles swept along their axis or ruled to an apex (mitcad#472).
target_sources(mitcad_geometry PRIVATE
  src/gear.cpp
  include/mitcad/geometry/gear.hpp)
# Faces that are pieces of one surface joined into one face, their edges
# along one curve into one edge (a thread's faces over all its turns,
# mitcad#343).
target_sources(mitcad_geometry PRIVATE
  src/face_join.cpp
  src/face_join.hpp)
# Faces of a body moved by a rigid motion, their neighbours following
# (mitcad#343).
target_sources(mitcad_geometry PRIVATE
  src/move_faces.cpp
  include/mitcad/geometry/move_faces.hpp)
# A planar face of a body extruded as a profile (mitcad#535).
target_sources(mitcad_geometry PRIVATE
  src/face_extrude.cpp
  include/mitcad/geometry/face_extrude.hpp)
# A planar face of a body revolved as a profile (mitcad#539).
target_sources(mitcad_geometry PRIVATE
  src/face_revolve.cpp
  include/mitcad/geometry/face_revolve.hpp)
# Blend surfaces of imported bodies stored without their approximation,
# made with OCCT's blend walking (mitcad#621).
target_sources(mitcad_geometry PRIVATE
  src/blend_surfaces.cpp
  src/blend_surfaces.hpp)
# Feature inputs drawn where they were: a Fillet's or a Chamfer's picked
# edges as polylines (mitcad#957).
target_sources(mitcad_geometry PRIVATE
  src/input_edges.cpp
  include/mitcad/geometry/input_edges.hpp)
# Dogbones: relief cylinders at a body's inside corners (mitcad#1011).
target_sources(mitcad_geometry PRIVATE
  src/dogbone.cpp
  include/mitcad/geometry/dogbone.hpp)
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
  # Booleans on perforated bodies (mitcad#72).
  target_sources(test_geometry PRIVATE tests/test_far_features.cpp)
  # The material a near copy of a body lacks (mitcad#85).
  target_sources(test_geometry PRIVATE tests/test_removed.cpp)
  # A mirror image joined only where it differs from the body (mitcad#88).
  target_sources(test_geometry PRIVATE tests/test_mirror_join.cpp)
  # Roundings wider than a neighbouring face, OCCT's edge overflow (mitcad#121).
  target_sources(test_geometry PRIVATE tests/test_fillet_overflow.cpp)
  # Roundings OCCT's fillet failed on, with the port's patches (mitcad#122).
  target_sources(test_geometry PRIVATE tests/test_fillet_kernel.cpp)
  # Ends, seams and corner caps of OCCT's roundings (mitcad#133).
  target_sources(test_geometry PRIVATE tests/test_fillet_ends.cpp)
  # Allocations that fail inside OCCT, with the port's patch (mitcad#132).
  target_sources(test_geometry PRIVATE tests/test_allocation.cpp)
  # ShapeFix on faces with many holes, with the port's patch 0060 (mitcad#60).
  target_sources(test_geometry PRIVATE tests/test_shapefix.cpp)
  # A chamfer whose pieces meet at a vertex of the edge they reach, with the
  # port's patches 0090 and 0091 (mitcad#160).
  target_sources(test_geometry PRIVATE tests/test_chamfer_kernel.cpp)
  # Shells whose wall makes a narrow face vanish, with the port's patch 0100
  # (mitcad#172).
  target_sources(test_geometry PRIVATE tests/test_shell_vanishing.cpp)
  # Walked roundings that are true rolling balls, with the port's patches
  # 0075-0077 (mitcad#121).
  target_sources(test_geometry PRIVATE tests/test_fillet_rolling_ball.cpp)
  # The faces a run of features made, copied by a pattern's optimized
  # compute option.
  target_sources(test_geometry PRIVATE tests/test_face_copy.cpp)
  # Operations on bodies of thousands of faces, with the port's patch 0110
  # (mitcad#121).
  target_sources(test_geometry PRIVATE tests/test_large_bodies.cpp)
  # Corners where a rounding ends, with the port's patches 0130 and later
  # (mitcad#236).
  target_sources(test_geometry PRIVATE tests/test_fillet_corners.cpp)
  # Chamfers wider than a face beside them, with the port's patches 0140 and
  # later (mitcad#240).
  target_sources(test_geometry PRIVATE tests/test_chamfer_overflow.cpp)
  # Roundings whose results were invalid or whose computation failed, with
  # the port's patches 0150 and later (mitcad#239).
  target_sources(test_geometry PRIVATE tests/test_fillet_results.cpp)
  # Fillet contours that stop where their edges turn, with the port's patch
  # 0180 (mitcad#241).
  target_sources(test_geometry PRIVATE tests/test_fillet_chains.cpp)
  # The tangent chains of a body's edges and faces (mitcad#290).
  target_sources(test_geometry PRIVATE tests/test_tangent_chain.cpp)
  # Roundings that run over cut-outs of their faces (mitcad#311).
  target_sources(test_geometry PRIVATE tests/test_fillet_cutouts.cpp)
  # Section curves of sketches (mitcad#389).
  target_sources(test_geometry PRIVATE tests/test_section_curves.cpp)
  # Roundings cut by a face of the solid they pass through, with the port's
  # patch 0700 (mitcad#343).
  target_sources(test_geometry PRIVATE tests/test_fillet_pierced.cpp)
  # Faces moved by a rigid motion, their neighbours following (mitcad#343).
  target_sources(test_geometry PRIVATE tests/test_move_faces.cpp)
  # A planar face of a body extruded as a profile (mitcad#535).
  target_sources(test_geometry PRIVATE tests/test_face_extrude.cpp)
  # A planar face of a body revolved as a profile (mitcad#539).
  target_sources(test_geometry PRIVATE tests/test_face_revolve.cpp)
  target_link_libraries(test_geometry PRIVATE mitcad_geometry_analysis)
  target_link_libraries(test_geometry PRIVATE mitcad_geometry mitcad_warnings)
  # The suite in parts, a ctest each (geometry.suite.<part>, label
  # geometry_suite): the parts run beside each other and the test runner
  # profiles each, where the whole suite was one 250 s test that every
  # check waited for (mitcad#832). `test_geometry` without arguments still
  # runs every part in one process. The parts are the table in
  # tests/test_main.cpp; geometry.suite_parts fails when this list and that
  # table differ, so that no part is left without its ctest. A new part:
  # its name at the end of this list. A part long enough to hold up a
  # check runs as numbered parts of about equal time (thread.1-3,
  # sweeps.1-5, mitcad#870).
  set(geometry_suite_parts
    naming
    profile
    extrude
    boolean
    dressup
    query
    transform
    faceops
    split
    datum
    extrude_feature
    revolve
    hole
    thread.1
    thread.2
    thread.3
    sweeps.1
    sweeps.2
    sweeps.3
    sweeps.4
    sweeps.5
    text
    skin
    persist
    cancel
    input_check
    far_features
    removed
    mirror_join
    fillet_overflow
    fillet_kernel
    fillet_ends
    allocation
    shapefix
    chamfer_kernel
    shell_vanishing
    fillet_rolling_ball
    face_copy
    large_bodies
    fillet_corners
    chamfer_overflow
    fillet_results
    fillet_chains
    tangent_chain
    fillet_cutouts
    section_curves
    fillet_pierced
    move_faces
    face_extrude
    face_revolve)
  foreach(part IN LISTS geometry_suite_parts)
    add_test(NAME geometry.suite.${part} COMMAND test_geometry --part ${part})
    set_tests_properties(geometry.suite.${part} PROPERTIES LABELS geometry_suite)
  endforeach()
  add_test(NAME geometry.suite_parts COMMAND test_geometry --check-parts ${geometry_suite_parts})
  # Planar material domains and sheet thickness/reference sides (mitcad#206).
  add_executable(test_sheet tests/test_sheet.cpp)
  target_link_libraries(test_sheet PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.sheet COMMAND test_sheet)
  add_executable(test_sheet_trim tests/test_sheet_trim.cpp)
  target_link_libraries(test_sheet_trim PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.sheet_trim COMMAND test_sheet_trim)
  add_executable(test_sheet_bend tests/test_sheet_bend.cpp)
  target_link_libraries(test_sheet_bend PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.sheet_bend COMMAND test_sheet_bend)
  add_executable(test_sheet_bend_graph tests/test_sheet_bend_graph.cpp)
  target_link_libraries(test_sheet_bend_graph PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.sheet_bend_graph COMMAND test_sheet_bend_graph)
  # Booleans on stored faces that run many turns (threaded holes, mitcad#349).
  add_executable(test_multi_turn_faces tests/test_multi_turn_faces.cpp)
  target_link_libraries(test_multi_turn_faces PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.multi_turn_faces COMMAND test_multi_turn_faces)
  # Joins of treads and helical stringers on shared cylinders (mitcad#400).
  add_executable(test_helical_joins tests/test_helical_joins.cpp)
  target_link_libraries(test_helical_joins PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.helical_joins COMMAND test_helical_joins)
  # Corners and ends of roundings, with the port's patches 0720 and later
  # (mitcad#343).
  add_executable(test_fillet_corner_cases tests/test_fillet_corner_cases.cpp)
  target_link_libraries(test_fillet_corner_cases PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_corner_cases COMMAND test_fillet_corner_cases)
  # A thread's crest narrower than the roundings of both its edges, with the
  # port's patch 0750 (mitcad#343).
  add_executable(test_fillet_thread_crests tests/test_fillet_thread_crests.cpp)
  target_link_libraries(test_fillet_thread_crests PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_thread_crests COMMAND test_fillet_thread_crests)
  # Roundings vanishing inside their chains, rolling over narrow faces and
  # lenses, with the port's patches 0770 and later (mitcad#343).
  add_executable(test_fillet_lens_overflow tests/test_fillet_lens_overflow.cpp)
  target_link_libraries(test_fillet_lens_overflow PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_lens_overflow COMMAND test_fillet_lens_overflow)
  # Roundings round a thread's chamfer and up to a groove's floor on bodies
  # with the traits of stored ones, with the port's patches 0800 and later
  # (mitcad#343).
  add_executable(test_fillet_crest_corners tests/test_fillet_crest_corners.cpp)
  target_link_libraries(test_fillet_crest_corners PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_crest_corners COMMAND test_fillet_crest_corners)
  # A fillet contour round a closed chain that stops at a turn short of its
  # start ends, with the port's patch 0840 (mitcad#434).
  add_executable(test_fillet_chain_loops tests/test_fillet_chain_loops.cpp)
  target_link_libraries(test_fillet_chain_loops PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_chain_loops COMMAND test_fillet_chain_loops)
  # A fillet contour that runs on into an edge whose faces are nearly
  # tangent, with the port's patches 0860 and 0861 (mitcad#441).
  add_executable(test_fillet_near_tangent_contour tests/test_fillet_near_tangent_contour.cpp)
  target_link_libraries(test_fillet_near_tangent_contour PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_near_tangent_contour COMMAND test_fillet_near_tangent_contour)
  # A rounding along an edge of a strip the ball cannot touch, with the
  # port's patch 0880 (mitcad#343).
  add_executable(test_fillet_strip_start tests/test_fillet_strip_start.cpp)
  target_link_libraries(test_fillet_strip_start PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_strip_start COMMAND test_fillet_strip_start)
  # Roundings whose contacts run off a narrow face: up to a sliver at the
  # edge's end, and over a hole's rounded mouth, with the port's patches 0890
  # and 0891 (mitcad#343).
  add_executable(test_fillet_end_overflow tests/test_fillet_end_overflow.cpp)
  target_link_libraries(test_fillet_end_overflow PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_end_overflow COMMAND test_fillet_end_overflow)
  # A rounding whose contact runs off a narrow face across its tangent edge
  # with a rounded rim keeps to that face's surface, cut by the rim, with
  # the port's patch 0870 (mitcad#343).
  add_executable(test_fillet_tangent_overflow tests/test_fillet_tangent_overflow.cpp)
  target_link_libraries(test_fillet_tangent_overflow PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_tangent_overflow COMMAND test_fillet_tangent_overflow)
  # A rounding ending where the face at the end goes on tangentially from
  # one of its faces, cut by that face, with the port's patch 0911
  # (mitcad#343).
  add_executable(test_fillet_tangent_end_face tests/test_fillet_tangent_end_face.cpp)
  target_link_libraries(test_fillet_tangent_end_face PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_tangent_end_face COMMAND test_fillet_tangent_end_face)
  # Both rims of a disc thinner than the two roundings, the rims' circles
  # turning opposite ways, with the port's patch 0930 (mitcad#343).
  add_executable(test_fillet_ride_against tests/test_fillet_ride_against.cpp)
  target_link_libraries(test_fillet_ride_against PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_ride_against COMMAND test_fillet_ride_against)
  # A rounding whose cut on the face at the end crosses that face's seam
  # past the seam's end, with the port's patch 0931 (mitcad#343).
  add_executable(test_fillet_seam_run_on tests/test_fillet_seam_run_on.cpp)
  target_link_libraries(test_fillet_seam_run_on PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_seam_run_on COMMAND test_fillet_seam_run_on)
  # A rounding over a narrow face whose far edge is a concave crease, the
  # face beyond cutting the blend's side and growing up to it, with the
  # port's patch 0940 (mitcad#343).
  add_executable(test_fillet_narrow_cut tests/test_fillet_narrow_cut.cpp)
  target_link_libraries(test_fillet_narrow_cut PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_narrow_cut COMMAND test_fillet_narrow_cut)
  # Boundary Fill: the cells tool bodies enclose (mitcad#457).
  add_executable(test_boundary_fill tests/test_boundary_fill.cpp)
  target_link_libraries(test_boundary_fill PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.boundary_fill COMMAND test_boundary_fill)
  # Patch, Stitch and Thicken: surface bodies filled, sewn and made solid (mitcad#456).
  add_executable(test_surfaces tests/test_surfaces.cpp)
  target_link_libraries(test_surfaces PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.surfaces COMMAND test_surfaces)
  # Surface modelling: surface bodies and their features (mitcad#455).
  add_executable(test_surface_modelling tests/test_surface_modelling.cpp)
  target_link_libraries(test_surface_modelling PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.surface_modelling COMMAND test_surface_modelling)
  # OCCT's crash handlers next to the application's crash report (mitcad#62).
  if(NOT WIN32)
    add_executable(test_guard tests/test_guard.cpp)
    target_link_libraries(test_guard PRIVATE mitcad_geometry mitcad_warnings)
    add_test(NAME geometry.guard_inside COMMAND test_guard inside)
    set_tests_properties(geometry.guard_inside PROPERTIES PASS_REGULAR_EXPRESSION "converted: SIGSEGV")
    add_test(NAME geometry.guard_outside COMMAND test_guard outside)
    set_tests_properties(geometry.guard_outside PROPERTIES PASS_REGULAR_EXPRESSION "earlier handler: signal 11")
    # The wrapper of the analysis bridge's operations (mitcad#392).
    add_test(NAME geometry.guard_operation COMMAND test_guard operation)
    set_tests_properties(geometry.guard_operation PROPERTIES ENVIRONMENT "MITCAD_TEST_OCCT_CRASH=clip"
      PASS_REGULAR_EXPRESSION "error: clip: SIGSEGV")
  endif()
  # Emboss: profiles wrapped onto cylinders (mitcad#458).
  add_executable(test_emboss tests/test_emboss.cpp)
  target_link_libraries(test_emboss PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.emboss COMMAND test_emboss)
  # Drawings: views with hidden lines removed and round holes (mitcad#471).
  add_executable(test_drawing tests/test_drawing.cpp)
  target_link_libraries(test_drawing PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.drawing COMMAND test_drawing)
  # A face taken away by the roundings of both its edges, their contacts
  # along each other, with the port's patch 0950 (mitcad#343).
  add_executable(test_fillet_consumed_strip tests/test_fillet_consumed_strip.cpp)
  target_link_libraries(test_fillet_consumed_strip PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_consumed_strip COMMAND test_fillet_consumed_strip)
  # A rounding ending where its wall steps back by much less than the
  # radius, with the port's patch 0951 (mitcad#343).
  add_executable(test_fillet_step_end tests/test_fillet_step_end.cpp)
  target_link_libraries(test_fillet_step_end PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_step_end COMMAND test_fillet_step_end)
  # A rounding vanishing where its faces come tangent, its walk stopped on
  # its own edge short of the vertex, with the port's patch 0961
  # (mitcad#343).
  add_executable(test_fillet_vanishing_own_edge tests/test_fillet_vanishing_own_edge.cpp)
  target_link_libraries(test_fillet_vanishing_own_edge PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_vanishing_own_edge COMMAND test_fillet_vanishing_own_edge)
  # Projection bounds whose parameter tolerance is below floating-point
  # spacing, with the port's patch 0960 (mitcad#504).
  add_executable(test_projection_boundary tests/test_projection_boundary.cpp)
  target_link_libraries(test_projection_boundary PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.projection_boundary COMMAND test_projection_boundary)
  set_tests_properties(geometry.projection_boundary PROPERTIES TIMEOUT 30)
  # A bottom outline rounded past wall corners rounded with less than its
  # radius, with the port's patch 0970 (mitcad#343).
  add_executable(test_fillet_tight_corner tests/test_fillet_tight_corner.cpp)
  target_link_libraries(test_fillet_tight_corner PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_tight_corner COMMAND test_fillet_tight_corner)
  # A rounding whose contact crosses a concave crease of a few degrees on
  # its side, with the port's patch 0980 (mitcad#343).
  add_executable(test_fillet_concave_crease tests/test_fillet_concave_crease.cpp)
  target_link_libraries(test_fillet_concave_crease PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_concave_crease COMMAND test_fillet_concave_crease)
  # A rounding rolling on an edge, its ends cut by the faces at the ends past
  # where its ball comes tangent to that edge's face, with the port's patch
  # 0990 (mitcad#343).
  add_executable(test_fillet_rolled_end_cut tests/test_fillet_rolled_end_cut.cpp)
  target_link_libraries(test_fillet_rolled_end_cut PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_rolled_end_cut COMMAND test_fillet_rolled_end_cut)
  # A chamfer across a slight concave crease of the tip's faces, over a side
  # strip narrower than the chamfer whose foot face is a B-spline, with the
  # port's patch 1000 (mitcad#343).
  add_executable(test_chamfer_crease_strip tests/test_chamfer_crease_strip.cpp)
  target_link_libraries(test_chamfer_crease_strip PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.chamfer_crease_strip COMMAND test_chamfer_crease_strip)
  # A closed rim chamfered past a pocket under it, the chamfer cut lengthwise
  # by the pocket's ceiling, with the port's patch 1010 (mitcad#343).
  add_executable(test_chamfer_rim_cutout tests/test_chamfer_rim_cutout.cpp)
  target_link_libraries(test_chamfer_rim_cutout PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.chamfer_rim_cutout COMMAND test_chamfer_rim_cutout)
  # A drafted peg's corners rounded down to a block whose walls go on from
  # the peg's sides at slight concave creases and whose corners are rounded
  # with less, the walls and those corners cutting the rounding, with the
  # port's patch 1020 (mitcad#343).
  add_executable(test_fillet_crease_into_end_walls tests/test_fillet_crease_into_end_walls.cpp)
  target_link_libraries(test_fillet_crease_into_end_walls PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_crease_into_end_walls COMMAND test_fillet_crease_into_end_walls)
  # Rolling-ball blends of imported bodies stored without their
  # approximation, walked by OCCT (mitcad#621).
  add_executable(test_blend_surfaces tests/test_blend_surfaces.cpp)
  target_link_libraries(test_blend_surfaces PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.blend_surfaces COMMAND test_blend_surfaces)
  # The face merge after booleans on faces of one cylinder whose axes point
  # the other way, with the port's patch 1040 (mitcad#642).
  add_executable(test_merge_reversed_axis tests/test_merge_reversed_axis.cpp)
  target_link_libraries(test_merge_reversed_axis PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.merge_reversed_axis COMMAND test_merge_reversed_axis)
endif()

if(BUILD_TESTING)
  add_executable(test_tool_contacts tests/test_tool_contacts.cpp)
  find_package(Threads REQUIRED)
  target_link_libraries(test_tool_contacts PRIVATE mitcad_geometry mitcad_geometry_io mitcad_warnings Threads::Threads)
  add_test(NAME geometry.tool_contacts COMMAND test_tool_contacts)
endif()

if(BUILD_TESTING)
  # Two concave edges rounded at a convex corner left sharp, the roundings
  # running past it into each other, with the port's patch 1060 (mitcad#840).
  add_executable(test_fillet_corner_past_wall tests/test_fillet_corner_past_wall.cpp)
  target_link_libraries(test_fillet_corner_past_wall PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_corner_past_wall COMMAND test_fillet_corner_past_wall)
  set_tests_properties(geometry.fillet_corner_past_wall PROPERTIES TIMEOUT 60)
  # A convex and a concave edge chamfered at a vertex where a sharp edge goes
  # on, the corner the same at both ends of the bracket, with the port's
  # patch 1090 (mitcad#1005).
  add_executable(test_chamfer_sharp_corner tests/test_chamfer_sharp_corner.cpp)
  target_link_libraries(test_chamfer_sharp_corner PRIVATE mitcad_geometry mitcad_geometry_io mitcad_warnings)
  add_test(NAME geometry.chamfer_sharp_corner COMMAND test_chamfer_sharp_corner)
  set_tests_properties(geometry.chamfer_sharp_corner PROPERTIES TIMEOUT 60)
endif()

# Data exchange and analysis services, separate libraries on top of OCCT.
add_subdirectory(io)
add_subdirectory(analysis)
