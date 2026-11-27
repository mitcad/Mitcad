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
  src/drawing_outline.hpp
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

# Pieces of a face a dressup takes away, left between its own faces, closed
# by a patch at each place (mitcad#1858).
target_sources(mitcad_geometry PRIVATE
  src/remnant_patch.cpp
  src/remnant_patch.hpp)

# Fillets and chamfers with Tangent Chain off on a chain that runs on: kept
# over the selected edges, running out over the next edge (mitcad#2037).
target_sources(mitcad_geometry PRIVATE
  src/runout_dressup.cpp
  src/runout_dressup.hpp)

# The end of a groove rounded as a ball mill ends it: the rounding closed
# round the tube's circle on the extended body, a sphere at the groove's
# radius (mitcad#2154).
target_sources(mitcad_geometry PRIVATE
  src/groove_end.cpp
  src/groove_end.hpp)

# Lofts' Tangent Edges: Merge, a section's tangent edges joined into one
# smooth edge by arc length (mitcad#2162).
target_sources(mitcad_geometry PRIVATE
  src/loft_merge.cpp
  src/loft_merge.hpp)

# Fillet's Full Round: the radius that rounds a face away between its two
# edges, worked out on the body on every build (mitcad#2279).
target_sources(mitcad_geometry PRIVATE
  src/full_round.cpp
  src/full_round.hpp)

# Fillets and chamfers of a straight edge ending on a flat face of a
# rounded body smaller than their section: refused in words where they are
# wider than the body there (mitcad#2229).
target_sources(mitcad_geometry PRIVATE
  src/end_face_dressup.cpp
  src/end_face_dressup.hpp)

# Fillets of edges whose faces come tangent inside them, turning from
# convex to concave there: split where the rounding vanishes (mitcad#2300).
target_sources(mitcad_geometry PRIVATE
  src/vanishing_split.cpp
  src/vanishing_split.hpp)

# Curved sheet faces developed: cylindrical and conical faces converted to
# sheet panels, and how a face bends two ways (mitcad#2435); oblique
# (eccentric) cones developed by their generators (mitcad#2491).
target_sources(mitcad_geometry PRIVATE
  src/sheet_develop.cpp
  include/mitcad/geometry/sheet_develop.hpp
  src/sheet_oblique_cone.cpp
  src/sheet_oblique_cone.hpp)

# Curvature continuous (G2) corners of fillets: the roundings set back to
# planes across their edges and a G2 patch of polynomial faces between them
# (mitcad#2332).
target_sources(mitcad_geometry PRIVATE
  src/g2_corner.cpp
  src/g2_corner.hpp)

# Isoparametric curves: a face's u or v curves as 3D curves, cut to the
# face (mitcad#2522).
target_sources(mitcad_geometry PRIVATE
  src/iso_curve.cpp
  include/mitcad/geometry/iso_curve.hpp)
# Continuity Analysis: the gap, the angle and the curvature difference
# across a body's edges, sampled along each between its faces (mitcad#2521).
target_sources(mitcad_geometry PRIVATE
  src/edge_continuity.cpp
  include/mitcad/geometry/edge_continuity.hpp)
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
  # The geometry's other C++ tests, in bundles by subject (mitcad#2497,
  # cmake/TestBundles.cmake): one program per bundle, linked as the
  # application links mitcad_geometry, each ctest running one test of it in
  # a process of its own (`test_geometry_<bundle> <test> [arguments]`, the
  # test's name its file's without `test_`). Each test is registered in
  # tests/bundled/<test file's stem>.cmake, included below; a new test goes
  # in the bundle of its subject:
  #   sheet        sheet metal, flexible parts, Form, Emboss
  #   surfaces     surface bodies, curves, projections, drawings, measures
  #   base         the kernel's guards, files read and written, analyses
  #   booleans     joins, booleans, classifiers, repeated parts
  #   loft_shell   lofts, pipes, shells and offsets
  #   chamfer      chamfers and the dressups' shared parts
  #   fillet_a_e, fillet_f_n, fillet_o_s, fillet_t_z
  #                fillets, by the first letter after `fillet_` in the name
  # A bundle that grows past about 30 tests is split, so that its program
  # stays well within the Windows linker's limits (LNK1140).
  foreach(bundle IN ITEMS sheet surfaces base booleans loft_shell chamfer fillet_a_e fillet_f_n fillet_o_s
                          fillet_t_z)
    mitcad_add_test_bundle(test_geometry_${bundle})
    target_link_libraries(test_geometry_${bundle} PRIVATE mitcad_geometry)
  endforeach()
endif()

# Data exchange and analysis services, separate libraries on top of OCCT.
add_subdirectory(io)
add_subdirectory(analysis)

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
  # A STEP file whose face bound the reader cannot resolve (port patch 1820,
  # mitcad#2028): geometry.port.1820 reads it through OCCT's document reader
  # with sub-shape names.
  target_link_libraries(test_port_patches PRIVATE TKXSBase TKDE TKDESTEP TKLCAF TKCAF TKXCAF)
endif()

# The tests of the geometry's bundles, each in a file of its own (above,
# mitcad#2497).
if(BUILD_TESTING)
  mitcad_include_bundled_tests("${CMAKE_CURRENT_SOURCE_DIR}/tests/bundled")
endif()

if(BUILD_TESTING)
  # How far a lofted body's side stands out beyond its two mirrored rails
  # (mitcad#2374), on a body exported as .brep: the outermost x of its level
  # sections against their x in the rails' plane y = 0. Not registered as a
  # test of its own: cli.loft_rails_reported runs it on GitHub #607's design
  # (with --most), and by hand it prints the levels.
  add_executable(test_probe_loft_outermost tests/probe_loft_outermost.cpp)
  target_link_libraries(test_probe_loft_outermost PRIVATE mitcad_geometry mitcad_warnings)
endif()

# Sheet faces sampled over their surface's parameters, for an approximate
# development by triangulation (mitcad#2442).
target_sources(mitcad_geometry PRIVATE
  src/sheet_face_grid.cpp
  include/mitcad/geometry/sheet_face_grid.hpp)
