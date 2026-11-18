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
# Sheet footprints of ordinary cuts across bends (mitcad#1628).
target_sources(mitcad_geometry PRIVATE
  src/sheet_footprint.cpp
  include/mitcad/geometry/sheet_footprint.hpp)
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
# Emboss: profiles wrapped onto cylinders (mitcad#458), projected onto any face (mitcad#1091).
target_sources(mitcad_geometry PRIVATE
  src/emboss.cpp
  src/emboss_projection.cpp
  src/emboss_projection.hpp
  src/emboss_taper.cpp
  src/emboss_taper.hpp
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
# Roundings that take a face away, round a loop of edges, mitred where
# they meet (mitcad#1118).
target_sources(mitcad_geometry PRIVATE
  src/mitred_dressup.cpp
  src/mitred_dressup.hpp)
# Roundings of the valleys where tubes meet, run on through the nodes where
# three of them meet a facet narrower than the radius (mitcad#1205).
target_sources(mitcad_geometry PRIVATE
  src/node_dressup.cpp
  src/node_dressup.hpp)
# Shells whose body has parts no thicker than the wall: the body less a
# cavity where the kernel's thick solid fails (mitcad#1233).
target_sources(mitcad_geometry PRIVATE
  src/shell_cavity.cpp
  src/shell_cavity.hpp)
# The warning for a fillet or chamfer whose made faces reach to the wrong
# side of the body, a folded or spiked corner (mitcad#1253).
target_sources(mitcad_geometry PRIVATE
  src/dressup_check.cpp
  src/dressup_check.hpp)
# Outlines of faces seen along a direction: Project of a curved face, its
# silhouettes and its seams only on the outline (mitcad#1265; HLR's outlines
# need TKHLR, linked for the drawings).
target_sources(mitcad_geometry PRIVATE
  src/face_outline.cpp
  include/mitcad/geometry/face_outline.hpp)

# Unions of many shapes group by group of meeting boxes, the groups beside
# each other (mitcad#1273).
target_sources(mitcad_geometry PRIVATE
  src/union_groups.cpp
  src/union_groups.hpp)

# Repeated parts: bodies that are copies of one another moved by a rigid
# motion (mitcad#1277).
target_sources(mitcad_geometry PRIVATE
  src/repeated_parts.cpp
  include/mitcad/geometry/repeated_parts.hpp)

# Intersection curves: two sketches' curves swept along their normals and
# the surfaces crossed (mitcad#1405).
target_sources(mitcad_geometry PRIVATE
  src/intersection_curve.cpp
  include/mitcad/geometry/intersection_curve.hpp)

# The item of the body before a feature that an item of its result (a pick
# on the feature's preview) comes from (mitcad#1458).
target_sources(mitcad_geometry PRIVATE
  src/item_origin.cpp
  include/mitcad/geometry/item_origin.hpp)

# Errors a person reads in plain words, their places and the agents'
# detail on a last line the model splits off (mitcad#1612).
target_sources(mitcad_geometry PRIVATE
  include/mitcad/geometry/user_error.hpp)

# Pipes and sweeps that pass through themselves, built in pieces and fused
# (mitcad#1697).
target_sources(mitcad_geometry PRIVATE
  src/pipe_folds.cpp
  src/pipe_folds.hpp)
# Flex: the route of a flexible item past the bodies and the solid along it
# (mitcad#1713; the bodies' triangles need TKMesh, linked for the drawings).
target_sources(mitcad_geometry PRIVATE
  src/flex_mesh.cpp
  src/flex_mesh.hpp
  src/flex_route.cpp
  include/mitcad/geometry/flex_route.hpp)

# Flex: routes of flexible items, the solids of their sections that are not
# round and the end ports of pipes and Flex items (mitcad#1713).
target_sources(mitcad_geometry PRIVATE
  src/flex.cpp
  include/mitcad/geometry/flex.hpp)

# Flex drape: a Flex item let hang, a chain that falls, lies on and wraps
# round the bodies, and the nodes it is kept by (mitcad#1713).
target_sources(mitcad_geometry PRIVATE
  src/flex_drape.cpp
  include/mitcad/geometry/flex_drape.hpp)

# Edges inside one smooth surface (seams, edges between pieces of one
# face) that the views leave out (mitcad#1773).
target_sources(mitcad_geometry PRIVATE
  src/smooth_seams.cpp
  include/mitcad/geometry/smooth_seams.hpp)

# Form bodies: the patches of a control cage's limit surface made faces and
# sewn into a solid or a sheet (mitcad#1757).
target_sources(mitcad_geometry PRIVATE
  src/form.cpp
  include/mitcad/geometry/form.hpp)
# Nearness of shapes and points on surfaces, told without measuring more
# than the answer needs (mitcad#1785).
target_sources(mitcad_geometry PRIVATE
  src/proximity.cpp
  src/proximity.hpp)
# Fill Cavity: the empty spaces of a body below a cap plane (mitcad#1818).
target_sources(mitcad_geometry PRIVATE
  src/fill_cavity.cpp
  include/mitcad/geometry/fill_cavity.hpp)
# What is computed of a shape again and again, kept by the shape's content
# (mitcad#1801).
target_sources(mitcad_geometry PRIVATE
  src/content_cache.cpp
  src/content_cache.hpp)
# A taper whose walls meet or vanish within the extrusion, its topology
# events resolved (mitcad#1851; sweep.cpp's tapered()).
target_sources(mitcad_geometry PRIVATE
  src/taper_events.cpp
  src/taper_events.hpp)
# A solid carried through a smooth map of space: those tapers bent onto a
# cylinder or a cone, or along a curved path (mitcad#1854).
target_sources(mitcad_geometry PRIVATE
  src/space_map.cpp
  src/space_map.hpp)
# Press Pull on a face continued Straight or Along the walls, tapered by a
# draft or a scale (mitcad#1850).
target_sources(mitcad_geometry PRIVATE
  src/press_pull.cpp
  include/mitcad/geometry/press_pull.hpp)
# Fillets whose rounded corners meet, built as the body's core swept by the
# ball (mitcad#1954).
target_sources(mitcad_geometry PRIVATE
  src/cored_dressup.cpp
  src/cored_dressup.hpp)
# Boxes over the trimmed faces, not their surfaces' parameter rectangles
# (mitcad#2020): bounding_box, the faces' boxes, drawing views' boxes.
target_sources(mitcad_geometry PRIVATE
  src/trimmed_box.cpp
  include/mitcad/geometry/trimmed_box.hpp)
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
  # Fillets and chamfers that take a face away exactly (mitcad#1106).
  target_sources(test_geometry PRIVATE tests/test_dressup_takes_face.cpp)
  # Section Analysis' clip of a threaded revolved body in its profile's plane
  # (mitcad#1230).
  target_sources(test_geometry PRIVATE tests/test_section_clip.cpp)
  # Shells of bodies with parts no thicker than the wall (mitcad#1233).
  target_sources(test_geometry PRIVATE tests/test_shell_thin_parts.cpp)
  # Outlines of faces seen along a direction: Project of a curved face
  # (mitcad#1265).
  target_sources(test_geometry PRIVATE tests/test_face_outline.cpp)
  # Tapers whose walls meet or vanish within the extrusion (mitcad#1851).
  target_sources(test_geometry PRIVATE tests/test_taper_events.cpp)
  # Press Pull on a face continued Straight or Along the walls, tapered
  # (mitcad#1850).
  target_sources(test_geometry PRIVATE tests/test_press_pull.cpp)
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
    face_revolve
    dressup_takes_face
    section_clip
    shell_thin_parts
    face_outline
    taper_events
    press_pull)
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
  # Chamfers wider than a narrow face beside their edge, cut by the face
  # beyond (mitcad#1232).
  add_executable(test_chamfer_narrow_face tests/test_chamfer_narrow_face.cpp)
  target_link_libraries(test_chamfer_narrow_face PRIVATE mitcad_geometry mitcad_geometry_analysis mitcad_warnings)
  add_test(NAME geometry.chamfer_narrow_face COMMAND test_chamfer_narrow_face)
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
  # A crash in code of its own inside an operation, in a function's body or
  # in a destructor: the operation's error, and the process goes on
  # (mitcad#1429).
  add_executable(test_guard_fault tests/test_guard_fault.cpp)
  target_link_libraries(test_guard_fault PRIVATE mitcad_geometry mitcad_warnings)
  foreach(where body destructor destructor-thread)
    add_test(NAME geometry.guard_fault_${where} COMMAND test_guard_fault ${where})
    set_tests_properties(geometry.guard_fault_${where} PROPERTIES
      PASS_REGULAR_EXPRESSION "error: probe: .*next operation: done")
  endforeach()
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
  # A chamfer whose chain runs on past its start into a face beyond and ends
  # against an earlier chamfer, alike at both ends, with the port's patch
  # 1120 (mitcad#1185).
  add_executable(test_chamfer_end_against_chamfer tests/test_chamfer_end_against_chamfer.cpp)
  target_link_libraries(test_chamfer_end_against_chamfer PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.chamfer_end_against_chamfer COMMAND test_chamfer_end_against_chamfer)
  set_tests_properties(geometry.chamfer_end_against_chamfer PROPERTIES TIMEOUT 60)
  # Lofts from a polygon to a circle shelled with both ends open and offset
  # closed, with the port's patches 1130 and 1190 (mitcad#1184, mitcad#1193).
  # One ctest per loft: in one ctest the cases took 158 s of 300 and timed
  # out under a release's load (mitcad#1565).
  add_executable(test_shell_loft_to_circle tests/test_shell_loft_to_circle.cpp)
  target_link_libraries(test_shell_loft_to_circle PRIVATE mitcad_geometry mitcad_warnings)
  set(shell_loft_to_circle_parts
    octagon_24_5_seam_0
    octagon_24_5_seam_22_5
    octagon_21_7_seam_0
    octagon_21_7_seam_22_5
    square_20
    lean_in_8
    lean_in_10
    lean_in_14_1
    facade)
  foreach(part IN LISTS shell_loft_to_circle_parts)
    add_test(NAME geometry.shell_loft_to_circle.${part} COMMAND test_shell_loft_to_circle ${part})
    set_tests_properties(geometry.shell_loft_to_circle.${part} PROPERTIES TIMEOUT 300)
  endforeach()
  add_test(NAME geometry.shell_loft_to_circle_parts
    COMMAND test_shell_loft_to_circle --check-parts ${shell_loft_to_circle_parts})
  # Reported volumes and areas against exact values and OCCT's adaptive
  # Gauss-Kronrod integration, lofts and their shells included (mitcad#1200).
  add_executable(test_accurate_volumes tests/test_accurate_volumes.cpp)
  target_link_libraries(test_accurate_volumes PRIVATE mitcad_geometry TKMesh mitcad_warnings)
  add_test(NAME geometry.accurate_volumes COMMAND test_accurate_volumes)
  set_tests_properties(geometry.accurate_volumes PROPERTIES TIMEOUT 300)
endif()

if(BUILD_TESTING)
  # Closed inward offsets of lofts from a polygon to a circle whose seam
  # splits a side, with the port's patch 1160 (mitcad#1199).
  add_executable(test_offset_loft_circle_seam tests/test_offset_loft_circle_seam.cpp)
  target_link_libraries(test_offset_loft_circle_seam PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.offset_loft_circle_seam COMMAND test_offset_loft_circle_seam)
  set_tests_properties(geometry.offset_loft_circle_seam PROPERTIES TIMEOUT 300)
  # The lean-in ones, the circle inside the polygon (mitcad#1199).
  add_test(NAME geometry.offset_loft_circle_seam_lean_in COMMAND test_offset_loft_circle_seam lean-in)
  set_tests_properties(geometry.offset_loft_circle_seam_lean_in PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # Roundings of the valleys where tubes meet, run on through nodes whose
  # facets are narrower than the radius (mitcad#1205).
  add_executable(test_fillet_tube_node tests/test_fillet_tube_node.cpp)
  target_link_libraries(test_fillet_tube_node PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_tube_node COMMAND test_fillet_tube_node)
  set_tests_properties(geometry.fillet_tube_node PROPERTIES TIMEOUT 300)
  # A rounding over the mouths of slots whose rounded ends touch the top
  # along a crest line, on a case of MITCAD_GEOMETRY_CASES, with the port's
  # patch 1210 (mitcad#1204).
  add_executable(test_fillet_slot_mouths tests/test_fillet_slot_mouths.cpp)
  target_link_libraries(test_fillet_slot_mouths PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_slot_mouths COMMAND test_fillet_slot_mouths)
  set_tests_properties(geometry.fillet_slot_mouths PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A rounding whose contact runs off a narrow face onto an earlier blend of
  # its radius rolls on that face's far edges, on cases of
  # MITCAD_GEOMETRY_CASES, with the port's patch 1230 (mitcad#1215).
  add_executable(test_fillet_roll_over_blend tests/test_fillet_roll_over_blend.cpp)
  target_link_libraries(test_fillet_roll_over_blend PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_roll_over_blend COMMAND test_fillet_roll_over_blend)
  set_tests_properties(geometry.fillet_roll_over_blend PROPERTIES TIMEOUT 600)
endif()

# Data exchange and analysis services, separate libraries on top of OCCT.
add_subdirectory(io)
add_subdirectory(analysis)

if(BUILD_TESTING)
  # Roundings of a short slanted corner edge where a thin wall meets a
  # rounded end wall (mitcad#1253).
  add_executable(test_fillet_step_corner tests/test_fillet_step_corner.cpp)
  target_link_libraries(test_fillet_step_corner PRIVATE mitcad_geometry mitcad_geometry_io mitcad_warnings)
  add_test(NAME geometry.fillet_step_corner COMMAND test_fillet_step_corner)
  set_tests_properties(geometry.fillet_step_corner PROPERTIES TIMEOUT 300)
endif()

if(BUILD_TESTING)
  # A chamfer of a concave edge wider than the narrow face beside it, cut by
  # the face beyond, never hanging out past it (mitcad#1260).
  add_executable(test_chamfer_overflow_concave tests/test_chamfer_overflow_concave.cpp)
  target_link_libraries(test_chamfer_overflow_concave PRIVATE mitcad_geometry mitcad_geometry_io
    mitcad_geometry_analysis mitcad_warnings)
  add_test(NAME geometry.chamfer_overflow_concave COMMAND test_chamfer_overflow_concave)
  set_tests_properties(geometry.chamfer_overflow_concave PROPERTIES TIMEOUT 300)
endif()

if(BUILD_TESTING)
  # Faces on one surface joined after a cut, keeping all their names: a
  # cut's wall from eight arcs of one circle, going on from a rim
  # (mitcad#1261).
  add_executable(test_same_surface_join tests/test_same_surface_join.cpp)
  target_link_libraries(test_same_surface_join PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.same_surface_join COMMAND test_same_surface_join)
endif()

if(BUILD_TESTING)
  # Unions of many bodies group by group, and a union that lost material
  # tried with a fuzzy value, on the reported design of GitHub #133 read
  # from MITCAD_MITCAD_REPORTED (its B-rep data zlib-compressed, mitcad#1273).
  find_package(ZLIB REQUIRED)
  add_executable(test_join_many_bodies tests/test_join_many_bodies.cpp)
  target_link_libraries(test_join_many_bodies PRIVATE mitcad_geometry ZLIB::ZLIB mitcad_warnings)
  add_test(NAME geometry.join_many_bodies COMMAND test_join_many_bodies)
  set_tests_properties(geometry.join_many_bodies PROPERTIES TIMEOUT 600)

  # Repeated parts: copies of a body moved by rigid motions found as one
  # part, its mirror image apart (mitcad#1277).
  add_executable(test_repeated_parts tests/test_repeated_parts.cpp)
  target_link_libraries(test_repeated_parts PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.repeated_parts COMMAND test_repeated_parts)
endif()

if(BUILD_TESTING)
  # OCCT's solid classifier on a point in the plane of a far face of the
  # shell, and the check of a block whose two cavities' bottoms lie in one
  # plane, with the port's patch 1285 (mitcad#1280); a point in the plane of
  # a flat shell and a cut by such a shell (mitcad#1881).
  add_executable(test_classifier_far_plane tests/test_classifier_far_plane.cpp)
  target_link_libraries(test_classifier_far_plane PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.classifier_far_plane COMMAND test_classifier_far_plane)
endif()

if(BUILD_TESTING)
  # A fin's top chamfered and rounded across slight creases of its profile,
  # on a case of MITCAD_GEOMETRY_CASES, with the port's patches 1300 to 1302
  # (mitcad#1341).
  add_executable(test_fillet_fin_top_creases tests/test_fillet_fin_top_creases.cpp)
  target_link_libraries(test_fillet_fin_top_creases PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_fin_top_creases COMMAND test_fillet_fin_top_creases)
  set_tests_properties(geometry.fillet_fin_top_creases PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A fin's top rounded up to its end face, and the fin's end rounded along
  # its sides up to the top, on cases of MITCAD_GEOMETRY_CASES, with the
  # port's patches 1310 and 1311 (mitcad#1345).
  add_executable(test_fillet_fin_end tests/test_fillet_fin_end.cpp)
  target_link_libraries(test_fillet_fin_end PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_fin_end COMMAND test_fillet_fin_end)
  set_tests_properties(geometry.fillet_fin_end PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A plate's sides rounded along an earlier rounding that vanishes into the
  # side, on a case of MITCAD_GEOMETRY_CASES, with the port's patch 1320
  # (mitcad#1346).
  add_executable(test_fillet_vanishing_side tests/test_fillet_vanishing_side.cpp)
  target_link_libraries(test_fillet_vanishing_side PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_vanishing_side COMMAND test_fillet_vanishing_side)
  set_tests_properties(geometry.fillet_vanishing_side PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # Roundings that end where a boss's cylinder meets a plate's flat side
  # tangentially, on synthetic bodies and on a case of MITCAD_GEOMETRY_CASES,
  # with the port's patches 1330 to 1332 (mitcad#1359); the plate's top edge
  # and the boss's foot rounded together, with 1400 to 1402 (mitcad#1374).
  add_executable(test_fillet_boss_tangent tests/test_fillet_boss_tangent.cpp)
  target_link_libraries(test_fillet_boss_tangent PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_boss_tangent COMMAND test_fillet_boss_tangent)
  set_tests_properties(geometry.fillet_boss_tangent PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A pocket's floor rounded with twice the radius of its concave corner
  # arcs, on a synthetic body and on a case of MITCAD_GEOMETRY_CASES, with
  # the port's patch 1390 (mitcad#1368).
  add_executable(test_fillet_ball_twice_arc tests/test_fillet_ball_twice_arc.cpp)
  target_link_libraries(test_fillet_ball_twice_arc PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_ball_twice_arc COMMAND test_fillet_ball_twice_arc)
  set_tests_properties(geometry.fillet_ball_twice_arc PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A boss's foot and rim rounded together with half the boss's height, the
  # foot's contact along part of the rim's, on synthetic bodies, with the
  # port's patch 1340 (mitcad#1392).
  add_executable(test_fillet_boss_foot_rim tests/test_fillet_boss_foot_rim.cpp)
  target_link_libraries(test_fillet_boss_foot_rim PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_boss_foot_rim COMMAND test_fillet_boss_foot_rim)
  set_tests_properties(geometry.fillet_boss_foot_rim PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A concave rounding along a wall's foot on a rod past concave arcs of the
  # wall tighter than the ball, on synthetic bodies and on cases of
  # MITCAD_GEOMETRY_CASES, with the port's patches 1380 and 1381
  # (mitcad#1371).
  add_executable(test_fillet_three_face_corner tests/test_fillet_three_face_corner.cpp)
  target_link_libraries(test_fillet_three_face_corner PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_three_face_corner COMMAND test_fillet_three_face_corner)
  set_tests_properties(geometry.fillet_three_face_corner PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A fin's side strips, narrower than the ball, rounded against its root's
  # chamfer, the ball rolled on along their edges past the transition pieces
  # of the arcs' chamfer, on a case of MITCAD_GEOMETRY_CASES, with the port's
  # patch 1420 (mitcad#1388).
  add_executable(test_fillet_fin_end_strips tests/test_fillet_fin_end_strips.cpp)
  target_link_libraries(test_fillet_fin_end_strips PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_fin_end_strips COMMAND test_fillet_fin_end_strips)
  set_tests_properties(geometry.fillet_fin_end_strips PROPERTIES TIMEOUT 600)
  # A concave rounding along an acute corner whose edge turns towards the
  # ball, walked along the balls' centres, with the port's patch 1440
  # (mitcad#1396); a case of MITCAD_GEOMETRY_CASES too.
  add_executable(test_fillet_acute_turn tests/test_fillet_acute_turn.cpp)
  target_link_libraries(test_fillet_acute_turn PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_acute_turn COMMAND test_fillet_acute_turn)
  set_tests_properties(geometry.fillet_acute_turn PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A plate's side edge rounded down to a sloped face past a groove beside
  # the slot's wall, on pieces of the stored body and of the import's body
  # (the same faces parametrised otherwise) of a case of
  # MITCAD_GEOMETRY_CASES, with the port's patch 1460 (mitcad#1395).
  add_executable(test_fillet_end_reversed_faces tests/test_fillet_end_reversed_faces.cpp)
  target_link_libraries(test_fillet_end_reversed_faces PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_end_reversed_faces COMMAND test_fillet_end_reversed_faces)
  set_tests_properties(geometry.fillet_end_reversed_faces PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A box's vertical edge rounded up through the mitre of two earlier
  # roundings of its top edges of the same radius, the spine either way,
  # with the port's patch 1490 (mitcad#1397).
  add_executable(test_fillet_mitre_same_radius tests/test_fillet_mitre_same_radius.cpp)
  target_link_libraries(test_fillet_mitre_same_radius PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_mitre_same_radius COMMAND test_fillet_mitre_same_radius)
  set_tests_properties(geometry.fillet_mitre_same_radius PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A rounding whose contact leaves its face across a convex edge into the
  # face at its end, short of the end: the ball rolled on that edge and the
  # face at the end cutting it from where its contact turns back, on a case
  # of MITCAD_GEOMETRY_CASES, with the port's patch 1430 (mitcad#1389).
  add_executable(test_fillet_end_edge_turn tests/test_fillet_end_edge_turn.cpp)
  target_link_libraries(test_fillet_end_edge_turn PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_end_edge_turn COMMAND test_fillet_end_edge_turn)
  set_tests_properties(geometry.fillet_end_edge_turn PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # Intersection curves: an arc and a line, two cylinders whose crossing
  # branches, parallel normals refused (mitcad#1405).
  add_executable(test_intersection_curve tests/test_intersection_curve.cpp)
  target_link_libraries(test_intersection_curve PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.intersection_curve COMMAND test_intersection_curve)
endif()

if(BUILD_TESTING)
  # A convex edge rounded up into the mitre of two earlier convex roundings
  # tighter than the ball, closed by a torus, on a synthetic block and a
  # case of MITCAD_GEOMETRY_CASES, with the port's patch 1500 (mitcad#1414).
  add_executable(test_fillet_convex_mitre tests/test_fillet_convex_mitre.cpp)
  target_link_libraries(test_fillet_convex_mitre PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_convex_mitre COMMAND test_fillet_convex_mitre)
  set_tests_properties(geometry.fillet_convex_mitre PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A lid's rims rounded through the mitres of earlier roundings of the same
  # radius, the ball staying put at each corner, on synthetic lids and a
  # case of MITCAD_GEOMETRY_CASES, with the port's patches 1520 and 1521
  # (mitcad#1412).
  add_executable(test_fillet_stationary_ball tests/test_fillet_stationary_ball.cpp)
  target_link_libraries(test_fillet_stationary_ball PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_stationary_ball COMMAND test_fillet_stationary_ball)
  set_tests_properties(geometry.fillet_stationary_ball PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A hub's foot rounded past the earlier roundings of the rims of holes
  # that touch the hub, tighter than the ball: the ball rolled on over the
  # rim's torus, cut by the face along the foot, each end a point on the
  # foot's edge, one face with the piece walked before it, on a case of
  # MITCAD_GEOMETRY_CASES, with the port's patches 1530 and 1540
  # (mitcad#1413, mitcad#1418).
  add_executable(test_fillet_tight_tangent_end tests/test_fillet_tight_tangent_end.cpp)
  target_link_libraries(test_fillet_tight_tangent_end PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_tight_tangent_end COMMAND test_fillet_tight_tangent_end)
  set_tests_properties(geometry.fillet_tight_tangent_end PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A boss's top rim rounded down to its foot's earlier rounding where the
  # foot has one: the two roundings meet along that rounding's edge, with
  # the port's patch 1560 (mitcad#1419).
  add_executable(test_fillet_rim_down_to_foot tests/test_fillet_rim_down_to_foot.cpp)
  target_link_libraries(test_fillet_rim_down_to_foot PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_rim_down_to_foot COMMAND test_fillet_rim_down_to_foot)
  set_tests_properties(geometry.fillet_rim_down_to_foot PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A wall's top and bottom edges rounded past a concave corner of the wall
  # into a tip whose edges are rounded tighter than the ball: the blend run
  # on past the chain's end over both faces' surfaces and cut by the faces
  # it runs into, on a case of MITCAD_GEOMETRY_CASES, with the port's
  # patches 1580 and 1581 (mitcad#1425).
  add_executable(test_fillet_torus_side_ends tests/test_fillet_torus_side_ends.cpp)
  target_link_libraries(test_fillet_torus_side_ends PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_torus_side_ends COMMAND test_fillet_torus_side_ends)
  set_tests_properties(geometry.fillet_torus_side_ends PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A face's outline rounded past a cylinder's rims, or a wall's corners,
  # rounded with less than the radius where the other wall's edge with the
  # face is acute (the vertex blend and a torus) or obtuse (the rim's own
  # ball rolled on between the roundings), on a synthetic cut tube and block
  # and a case of MITCAD_GEOMETRY_CASES, with the port's patch 1590
  # (mitcad#1426).
  add_executable(test_fillet_tight_rim_corner tests/test_fillet_tight_rim_corner.cpp)
  target_link_libraries(test_fillet_tight_rim_corner PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_tight_rim_corner COMMAND test_fillet_tight_rim_corner)
  set_tests_properties(geometry.fillet_tight_rim_corner PROPERTIES TIMEOUT 600)
  # A wall's edges rounded through corners where the ball stays put in the
  # tube of an earlier rounding of its own radius: the walk goes on into
  # that ball, and two contours meet at its section, on a synthetic block
  # and on a case of MITCAD_GEOMETRY_CASES, with the port's patches 1600 and
  # 1601 (mitcad#1427).
  add_executable(test_fillet_still_ball_corners tests/test_fillet_still_ball_corners.cpp)
  target_link_libraries(test_fillet_still_ball_corners PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_still_ball_corners COMMAND test_fillet_still_ball_corners)
  set_tests_properties(geometry.fillet_still_ball_corners PROPERTIES TIMEOUT 600)
  # A concave groove's rim rounded past an earlier rounding of the ball's
  # radius and up to a wall's arc: the walk on from the standing ball's
  # sphere where the guide's normal planes pass it forwards, the corners
  # where the rim's roundings cross cut along their intersection, on a
  # synthetic leg and a case of MITCAD_GEOMETRY_CASES, with the port's
  # patches 1610 and 1611 (mitcad#1428).
  add_executable(test_fillet_groove_rim_corners tests/test_fillet_groove_rim_corners.cpp)
  target_link_libraries(test_fillet_groove_rim_corners PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_groove_rim_corners COMMAND test_fillet_groove_rim_corners)
  set_tests_properties(geometry.fillet_groove_rim_corners PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A rounding whose edge ends where a convex crease of its face begins:
  # the ball rolled on over the crease into the face at the end (a horn
  # torus), on a synthetic cut cylinder and a case of MITCAD_GEOMETRY_CASES,
  # with the port's patch 1620 (mitcad#1434).
  add_executable(test_fillet_crease_end_corner tests/test_fillet_crease_end_corner.cpp)
  target_link_libraries(test_fillet_crease_end_corner PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_crease_end_corner COMMAND test_fillet_crease_end_corner)
  set_tests_properties(geometry.fillet_crease_end_corner PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A pipe's Corners, mitered or rounded, along an L-shaped path
  # (mitcad#1448).
  add_executable(test_pipe_corners tests/test_pipe_corners.cpp)
  target_link_libraries(test_pipe_corners PRIVATE mitcad_geometry mitcad_geometry_analysis mitcad_warnings)
  add_test(NAME geometry.pipe_corners COMMAND test_pipe_corners)
endif()

if(BUILD_TESTING)
  # A block's foot edges on a plate rounded (mitcad#1458): three of them
  # meet in mitres at the block's convex corners without the concave
  # check's warning of a rounding hanging out; with the ends' alone, the
  # front's foot edge in the result comes from the foot edge before
  # (item_origin), the roundings are the fillet's own.
  add_executable(test_fillet_foot_mitre tests/test_fillet_foot_mitre.cpp)
  target_link_libraries(test_fillet_foot_mitre PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_foot_mitre COMMAND test_fillet_foot_mitre)
endif()

if(BUILD_TESTING)
  # A roof slab's section of 7 points extruded and rounded on all its edges
  # (mitcad#1482): it builds, or the error names the face whose two edges'
  # roundings reach across it.
  add_executable(test_fillet_roof_slab tests/test_fillet_roof_slab.cpp)
  target_link_libraries(test_fillet_roof_slab PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_roof_slab COMMAND test_fillet_roof_slab)
  set_tests_properties(geometry.fillet_roof_slab PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # Roundings riding on another's contact line and the corners at their ends
  # (mitcad#1499, the port's patches 1640 to 1642): a plate 1.3 thick
  # rounded with 1 on an end face's rims, with the vertical edges at both
  # its ends, and on all 12 edges; the roof slab's end-face pair along one
  # slope. Valid closed solids with checked volumes.
  add_executable(test_fillet_ride_corners tests/test_fillet_ride_corners.cpp)
  target_link_libraries(test_fillet_ride_corners PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_ride_corners COMMAND test_fillet_ride_corners)
  set_tests_properties(geometry.fillet_ride_corners PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # The roof slab's eave (mitcad#1500, mitcad#1530, the port's patches 1650
  # to 1653): its two long edges and its end edges rounded with 0.9 and 1,
  # the tip's rounding rolling on the obtuse edge; an end face's strip along
  # a slope and the eave's end edge; the tip's corner with the slope's end
  # edge. Valid closed solids with checked volumes.
  add_executable(test_fillet_eave tests/test_fillet_eave.cpp)
  target_link_libraries(test_fillet_eave PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_eave COMMAND test_fillet_eave)
  set_tests_properties(geometry.fillet_eave PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # A rib's four top edges rounded or bevelled by half its width (mitcad#1500,
  # the port's patch 1655): the long edges' contacts meet on one ridge edge,
  # the top face gone, no face of no area, the exact shape's volume. And a
  # cube rounded on all its edges by 4.9998 (mitcad#1922, patch 1701): the
  # join's contacts of no length no longer hang it, and it is never given
  # back unrounded as done.
  add_executable(test_fillet_rib_full_round tests/test_fillet_rib_full_round.cpp)
  target_link_libraries(test_fillet_rib_full_round PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_rib_full_round COMMAND test_fillet_rib_full_round)
  set_tests_properties(geometry.fillet_rib_full_round PROPERTIES TIMEOUT 600)
endif()

if(BUILD_TESTING)
  # Joins of B-spline-faced bodies repeated with the booleans' parallel mode
  # and the solid classifier's pool on (the port's patch 1282), and
  # classifiers of a joined solid loaded on several threads at once: the
  # crash reproduction's loops of mitcad#1553, shortened.
  add_executable(test_classifier_pool_joins tests/test_classifier_pool_joins.cpp)
  target_link_libraries(test_classifier_pool_joins PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.classifier_pool_joins COMMAND test_classifier_pool_joins)
  set_tests_properties(geometry.classifier_pool_joins PROPERTIES TIMEOUT 600)
  # A pipe's section stays square to its path (mitcad#1555): an 8 mm circle
  # along the intersection curves of a sine wave with a plainly bent arc and
  # with one bending a fraction of a millimetre: cuts across the side square
  # to the path within 0.01 degrees at 20 points, the volume the circle's
  # area times the path's length within 0.1 %.
  add_executable(test_pipe_square_section tests/test_pipe_square_section.cpp)
  target_link_libraries(test_pipe_square_section PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.pipe_square_section COMMAND test_pipe_square_section)
endif()

if(BUILD_TESTING)
  # A fillet's end cut by the faces round a notch (mitcad#1615, the port's
  # patch 1670): a flange's foot rounded past an end cap that stops short of
  # the flange's tip, the rounding crossing the cap's side's boundary with
  # the flange's inside past a vertex that splits it: one valid closed solid,
  # every edge on two faces, the exact shape's volume.
  add_executable(test_fillet_end_past_split_edge tests/test_fillet_end_past_split_edge.cpp)
  target_link_libraries(test_fillet_end_past_split_edge PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_end_past_split_edge COMMAND test_fillet_end_past_split_edge)
  # Thrown texts say what is wrong in words (mitcad#1612): no face, edge or
  # vertex named and no point printed outside placed_error() and its helpers
  # (mitcad/geometry/user_error.hpp); the scan of the sources and its own
  # check on planted ones.
  add_executable(test_user_text_scan tests/test_user_text_scan.cpp)
  target_link_libraries(test_user_text_scan PRIVATE mitcad_warnings)
  add_test(NAME geometry.user-text-scan COMMAND test_user_text_scan "${CMAKE_CURRENT_SOURCE_DIR}/src")
  add_test(NAME geometry.user-text-scan-self-test COMMAND test_user_text_scan --self-test)
endif()

if(BUILD_TESTING)
  # Pipes and sweeps that pass through themselves (mitcad#1697): a circle
  # along an arc tighter than its radius (alone, round a whole circle,
  # between two legs, hollow), a square along a chamfer too short for its
  # miters and a circle along a path that crosses itself, each one valid
  # solid without crossing faces whose volume is the union of its pieces
  # built from OCCT's primitives, within 1e-6; a corner that turns back
  # refused in words.
  add_executable(test_pipe_folds tests/test_pipe_folds.cpp)
  target_link_libraries(test_pipe_folds PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.pipe_folds COMMAND test_pipe_folds)
endif()

if(BUILD_TESTING)
  # Flex routes (mitcad#1713): a hose over the edge between two faces of a
  # block and round the block between opposite faces, through the air round
  # it, along a cylinder; a flat cable's flat side on the cylinder and a
  # half turn at a node; a turn the item cannot make returned with its
  # places; the solids' volumes; the ports of a route and of a pipe's end;
  # a wire from a pipe's end over a block's top and edge to its side, and
  # its hollow solid.
  add_executable(test_flex_route tests/test_flex_route.cpp)
  target_link_libraries(test_flex_route PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.flex_route COMMAND test_flex_route)
  # Its cases run 4-5 minutes under a release's load (238.5 s in 0.65.0,
  # past 300 s in 0.66.0-rc1 with the added cases).
  set_tests_properties(geometry.flex_route PROPERTIES TIMEOUT 900)
  # Flex drape (mitcad#1713): a hose draped over a cylinder from its pinned
  # ends, a wire hanging straight from one end, a pinned middle point that
  # stays, a stiff hose hanging wider than a limp wire, and the nodes of
  # the drape, through which the route is the chain's shape.
  add_executable(test_flex_drape tests/test_flex_drape.cpp)
  target_link_libraries(test_flex_drape PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.flex_drape COMMAND test_flex_drape)
  set_tests_properties(geometry.flex_drape PROPERTIES TIMEOUT 300)
  # Edges inside one smooth surface (mitcad#1773): a cylinder's, a sphere's,
  # a hole's and a smooth extruded profile's seam, the edge between two faces
  # on one plane and between two named pieces of one face; never a box's
  # edges, a rounded plate's, a teardrop's crease or two faces' on their own
  # surfaces.
  add_executable(test_smooth_seams tests/test_smooth_seams.cpp)
  target_link_libraries(test_smooth_seams PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.smooth_seams COMMAND test_smooth_seams)
  # The curve tools (mitcad#1777): a growing helix on its cone, a line's
  # helix the helicoid of its area, the helicoid and a cylinder meeting in
  # one helix on the cylinder, a face name that finds nothing refused.
  add_executable(test_curve_tools tests/test_curve_tools.cpp)
  target_link_libraries(test_curve_tools PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.curve_tools COMMAND test_curve_tools)
endif()

if(BUILD_TESTING)
  # Form bodies (mitcad#1757): six bulged bicubic patches of a cube sewn into
  # a valid solid with its faces named, a fillet on one of its edges and a
  # hole through it; five of them a sheet; a missing one and a broken patch
  # refused.
  add_executable(test_form tests/test_form.cpp)
  target_link_libraries(test_form PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.form COMMAND test_form)
  # Nearness told without measuring the whole distance (mitcad#1785):
  # within_distance agrees with BRepExtrema's distance at limits around it
  # for solids, faces (a B-spline one too), edges and vertices apart,
  # touching and inside, and a surface probe with a projection of each
  # point on B-spline, cylindrical and plane surfaces.
  add_executable(test_proximity tests/test_proximity.cpp)
  target_link_libraries(test_proximity PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.proximity COMMAND test_proximity)
endif()

if(BUILD_TESTING)
  # A fillet or chamfer wider than a face beside its edge takes that face
  # away (mitcad#1819), one table, one row per case, over the narrow face
  # (planar between parallel or other faces, a cylinder, a cone, a
  # B-spline strip), the chain (one edge, a tangent chain over rounded
  # corners, an octagon over sharp corners, an edge ending on a third face),
  # convex and concave, the dressup (chamfers: equal, two distances each
  # way, distance and angle; fillets) and the size (0.5w to 3w, and one past
  # the next face, which fails in words): the equal chamfer and the fillet
  # at every size on every narrow face, every dressup at the exact width,
  # 1.01w and past, and pairwise rows that put every two columns' values
  # together; each one valid closed solid whose volume is the reference's
  # built in the owner's order (the dressup on the picked edge with the
  # narrow face continued, then the face beyond grown out), exactly where
  # the face is planar along a straight chain. Three parts, one ctest each.
  add_executable(test_dressup_consumes_face tests/test_dressup_consumes_face.cpp)
  target_link_libraries(test_dressup_consumes_face PRIVATE mitcad_geometry mitcad_geometry_analysis mitcad_warnings)
  foreach(part IN ITEMS core kinds chains)
    add_test(NAME geometry.dressup_consumes_face.${part} COMMAND test_dressup_consumes_face --part ${part})
    set_tests_properties(geometry.dressup_consumes_face.${part} PROPERTIES TIMEOUT 300)
  endforeach()
  # Repeated work taken out of OCCT with the same results (mitcad#1823): a
  # blend's line read by index in constant time keeps its points' order and
  # addresses (port patch 1690), and a curve/surface intersection with the
  # surface's kept polyhedra gives the intersections made without them
  # (patch 1691).
  add_executable(test_kernel_repeated_work tests/test_kernel_repeated_work.cpp)
  target_link_libraries(test_kernel_repeated_work PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.kernel_repeated_work COMMAND test_kernel_repeated_work)
  # Fill Cavity (mitcad#1818): a block with a blind V slit beside a taller
  # wall, its empty spaces below the cap: the slit closed by the cap with
  # its volume and its walls' names, the space round the block open; a box
  # the body does not reach whole; the body unchanged.
  add_executable(test_fill_cavity tests/test_fill_cavity.cpp)
  target_link_libraries(test_fill_cavity PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fill_cavity COMMAND test_fill_cavity)
  # Roundings and a chamfer OCCT builds that five port patches once broke
  # (mitcad#1883, from OCCT's own test cases run against the port): a
  # partial revolve rounded up to a pole (0211, 0721) and from a cone's apex
  # (0620), a box's top rounded to a full round with vertical roundings at
  # its ends (0153), crossing cylinders rounded through their near tangent
  # points (0771), a throat chamfer's tolerance (0721).
  add_executable(test_fillet_occt_cases tests/test_fillet_occt_cases.cpp)
  target_link_libraries(test_fillet_occt_cases PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_occt_cases COMMAND test_fillet_occt_cases)
  # A fin's foot rounded higher than the flat stretch its curved side bends
  # into, its chain ending where the fin's top runs down to the plate: the
  # blend runs on over the flat stretch's plane, cut by the top, with OCCT
  # alone, on a case of MITCAD_GEOMETRY_CASES (port patch 1700,
  # mitcad#1926).
  add_executable(test_fillet_wall_run_on tests/test_fillet_wall_run_on.cpp)
  target_link_libraries(test_fillet_wall_run_on PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_wall_run_on COMMAND test_fillet_wall_run_on)
  # The same fin's top rounded along its outer and its inner edges, two
  # chains whose blends lie on the one top face, with OCCT alone: not
  # judged overlapping (port patch 1710); with the foot's rounding as well
  # the fillet returns (1711) (mitcad#1926).
  add_executable(test_fillet_top_chains tests/test_fillet_top_chains.cpp)
  target_link_libraries(test_fillet_top_chains PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_top_chains COMMAND test_fillet_top_chains)
  # Two roundings over a face narrower than both, the smaller ball rolling
  # tangent on the larger rounding's blend, which its contact cuts, the face
  # vanishing: convex with convex, concave with convex, on synthetic prisms
  # (port patch 1720, mitcad#1926); and from an end to a crossing of their
  # contacts only (port patch 1721).
  add_executable(test_fillet_tangent_ride tests/test_fillet_tangent_ride.cpp)
  target_link_libraries(test_fillet_tangent_ride PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_tangent_ride COMMAND test_fillet_tangent_ride)
  # A drafted block's rim rounded with a ball larger than the roundings of
  # its corners, the ball's centre beyond their axes: walked with its
  # sections in the guide's normal plane, with OCCT alone (port patch 1760,
  # mitcad#1977).
  add_executable(test_fillet_ball_beyond_support tests/test_fillet_ball_beyond_support.cpp)
  target_link_libraries(test_fillet_ball_beyond_support PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_ball_beyond_support COMMAND test_fillet_ball_beyond_support)
  # A block's foot and sloping top rounded beside a front face lower than
  # the two roundings together: their contacts cross before ends joined by
  # the front face's edge; reported as overlapping and built one after the
  # other, alone and against a wall whose foot is rounded too (port patch
  # 1780, mitcad#1987).
  add_executable(test_fillet_linked_ends tests/test_fillet_linked_ends.cpp)
  target_link_libraries(test_fillet_linked_ends PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_linked_ends COMMAND test_fillet_linked_ends)
endif()

if(BUILD_TESTING)
  # A fillet on a knife edge, two faces meeting tangentially at 0 degrees:
  # the ball placed where the wedge is two radii thick, the tip beyond its
  # contacts taken away, with OCCT alone, on synthetic prisms (port patch
  # 1770, mitcad#1984).
  add_executable(test_fillet_knife_edge tests/test_fillet_knife_edge.cpp)
  target_link_libraries(test_fillet_knife_edge PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_knife_edge COMMAND test_fillet_knife_edge)
endif()

if(BUILD_TESTING)
  # A knife edge running on into an ordinary edge on the same line (a bite
  # cut part of the way along a wall): OCCT's fillet refuses the corner
  # where the ball jumps (port patch 1790), MitCad's rounds the knife edge
  # first and the other edge on its result (mitcad#1988).
  add_executable(test_fillet_knife_chain tests/test_fillet_knife_chain.cpp)
  target_link_libraries(test_fillet_knife_chain PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_knife_chain COMMAND test_fillet_knife_chain)
endif()

if(BUILD_TESTING)
  # Regression tests of the OCCT port's patches (mitcad#1986): one file per
  # patch in tests/port, p<number>-<topic>.cpp, registering its tests under
  # the patch's number (port_test.hpp), one ctest geometry.port.<number> per
  # file. A new patch test adds its file and edits no list here.
  file(GLOB MITCAD_PORT_PATCH_TESTS CONFIGURE_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/tests/port/p*.cpp)
  add_executable(test_port_patches tests/port/main.cpp tests/port/port_test.hpp ${MITCAD_PORT_PATCH_TESTS})
  target_link_libraries(test_port_patches PRIVATE mitcad_geometry mitcad_warnings)
  foreach(test_file IN LISTS MITCAD_PORT_PATCH_TESTS)
    get_filename_component(test_stem ${test_file} NAME_WE)
    string(SUBSTRING ${test_stem} 1 4 test_part)
    add_test(NAME geometry.port.${test_part} COMMAND test_port_patches --part ${test_part})
    set_tests_properties(geometry.port.${test_part} PROPERTIES TIMEOUT 300)
  endforeach()
endif()

if(BUILD_TESTING)
  # A rib's sloped edge rounded 1 degree from a bent guide's end, with
  # OCCT's fillet alone, on a case of MITCAD_GEOMETRY_CASES: the corner's
  # set-back walks over the rim's end vertex (port patch 1801), a walk
  # starting on its face's edge ends there (1802) (mitcad#2018).
  add_executable(test_fillet_corner_walk_over_vertex tests/test_fillet_corner_walk_over_vertex.cpp)
  target_link_libraries(test_fillet_corner_walk_over_vertex PRIVATE mitcad_geometry mitcad_warnings)
  add_test(NAME geometry.fillet_corner_walk_over_vertex COMMAND test_fillet_corner_walk_over_vertex)

  # Boxes over the trimmed faces, not their surfaces' parameter rectangles:
  # synthetic faces of known extent, and a rounded bend's corner plate on a
  # case of MITCAD_GEOMETRY_CASES against a fine triangulation (mitcad#2020).
  add_executable(test_trimmed_box tests/test_trimmed_box.cpp)
  target_link_libraries(test_trimmed_box PRIVATE mitcad_geometry TKMesh mitcad_warnings)
  add_test(NAME geometry.trimmed_box COMMAND test_trimmed_box)
  set_tests_properties(geometry.trimmed_box PROPERTIES TIMEOUT 300)
endif()
