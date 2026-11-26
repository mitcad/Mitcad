# SPDX-License-Identifier: MIT
# mitcad-cli: the model and the kernel without the user interface.
add_executable(mitcad-cli main.cpp library.cpp library.hpp projects.cpp projects.hpp)
# Paths and arguments as UTF-8 on Windows too (cmake/Utf8CodePage.cmake).
mitcad_utf8_code_page(mitcad-cli)
target_link_libraries(mitcad-cli PRIVATE mitcad_ffi mitcad_geometry mitcad_geometry_io mitcad_warnings)
# `mitcad-cli --version` (mitcad#1170).
target_compile_definitions(mitcad-cli PRIVATE MITCAD_VERSION="${PROJECT_VERSION}")

# mitcad-cli render (mitcad#48, docs/rendering.md "Final render"). It makes
# the scene and the job as File > Render Image does (the application's
# render/RenderBatch, with Qt Core and Gui, no widgets) and runs the render
# worker mitcad-render; it links none of the renderer's libraries.
set(app "${PROJECT_SOURCE_DIR}/app")
target_sources(mitcad-cli PRIVATE render.cpp render.hpp
  "${app}/framework/Appearances.cpp" "${app}/framework/Appearances.hpp"
  "${app}/render/RenderBatch.cpp" "${app}/render/RenderBatch.hpp"
  "${app}/render/RenderDevice.cpp" "${app}/render/RenderDevice.hpp"
  "${app}/render/RenderMesh.cpp" "${app}/render/RenderMesh.hpp"
  "${app}/render/RenderScene.cpp" "${app}/render/RenderScene.hpp")
set_target_properties(mitcad-cli PROPERTIES AUTOMOC ON)
target_include_directories(mitcad-cli PRIVATE "${app}")
target_include_directories(mitcad-cli SYSTEM PRIVATE "${MITCAD_CGLTF_INCLUDE_DIR}")
target_link_libraries(mitcad-cli PRIVATE Qt6::Gui TKMesh TKFillet TKPrim TKTopAlgo TKGeomBase TKG3d TKBRep)
if(APPLE)
  target_link_libraries(mitcad-cli PRIVATE mitcad_mac_work)
endif()
# A mixed installation at start-up (mitcad#1454): the OCCT DLLs loaded
# against this build's stamp (cmake/OcctBuildStamp.cmake), as mitcad.exe
# checks them.
target_sources(mitcad-cli PRIVATE "${app}/update/BuildStamp.hpp" "${app}/update/InstallationCheck.cpp"
               "${app}/update/InstallationCheck.hpp" "${app}/update/RunningPrograms.hpp")
mitcad_occt_build_stamp(mitcad-cli)

# The imports, which the import corpus tests run, apart from the other
# commands (tools/check-all.sh runs the corpus again when they change).
target_sources(mitcad-cli PRIVATE import.cpp import.hpp)
# Drawings with views, dimensions and a hole table (mitcad#471).
target_sources(mitcad-cli PRIVATE drawing.cpp drawing.hpp)
# Display meshings compared face by face (mitcad#644): the view's mesh
# options from app/view/ViewTessellation.hpp.
target_sources(mitcad-cli PRIVATE mesh_compare.cpp mesh_compare.hpp)

if(BUILD_TESTING)
  # Synthetic planar-face profiles with the real model/kernel bridge
  # (mitcad#1138). The same executable writes queried-key UI fixtures.
  add_executable(test_face_profiles test-face-profiles.cpp)
  mitcad_utf8_code_page(test_face_profiles)
  target_link_libraries(test_face_profiles PRIVATE mitcad_ffi mitcad_geometry Qt6::Core mitcad_warnings)
  if(APPLE)
    target_link_libraries(test_face_profiles PRIVATE mitcad_mac_work)
  endif()
  add_test(NAME cli.face_bounded_profiles COMMAND test_face_profiles)
  set_tests_properties(cli.face_bounded_profiles PROPERTIES TIMEOUT 180)

  set(scripts "${CMAKE_CURRENT_SOURCE_DIR}/tests")
  # Each script builds a part with commands and checks it with expectations.
  foreach(script block_fillet_chamfer join cut_split names_follow_parameters)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()
  # Profile features (F1), with the volumes of the T0 reference models.
  foreach(script f1_extrude f1_revolve f1_hole f1_thread f1_thread_chamfer f1_failures f1_tapped_hole_threads
                 f1_thread_part_chamfer)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()
  # A design saved before a modelled thread's pieces were numbered along
  # its axis: its other references hold, an old name of a whole turn's
  # piece is reported (mitcad#490).
  add_test(NAME cli.thread_old_piece_names COMMAND mitcad-cli run "${scripts}/thread_old_piece_names.json"
                                                   --open "${scripts}/thread_old_piece_names.mitcad")
  # Thin walls along open chains and closed loops of sketch curves (mitcad#463).
  add_test(NAME cli.f1_thin_curves COMMAND mitcad-cli run "${scripts}/f1_thin_curves.json")
  # A taper that closes the profile partway names the walls that meet (mitcad#1186).
  add_test(NAME cli.f1_taper_closes COMMAND mitcad-cli run "${scripts}/f1_taper_closes.json")
  # Press-fit holes for printed parts: hex, square and crush ribs (mitcad#1517).
  add_test(NAME cli.press_fit_holes COMMAND mitcad-cli run "${scripts}/press_fit_holes.json")
  # Assembly cut scope preserves component-local geometry and split ownership.
  set(cut_scope "${CMAKE_CURRENT_BINARY_DIR}/extrude-cut-scope.mitcad")
  add_test(NAME cli.extrude_cut_scope COMMAND mitcad-cli run "${scripts}/extrude_cut_scope.json" --save "${cut_scope}")
  set_tests_properties(cli.extrude_cut_scope PROPERTIES FIXTURES_SETUP cli_cut_scope)
  add_test(NAME cli.extrude_cut_scope_reopened COMMAND mitcad-cli run "${scripts}/extrude_cut_scope_reopened.json" --open "${cut_scope}")
  set_tests_properties(cli.extrude_cut_scope_reopened PROPERTIES FIXTURES_REQUIRED cli_cut_scope)

  # Transforms, patterns, mirrors, combine and primitives (F4): the volumes
  # of the T0 reference models (MITCAD_F3D_MODELS) and analytic ones.
  foreach(script f4_combine f4_mirror f4_mirror_feature f4_move f4_align_scale
                 f4_pattern_bodies f4_pattern_features f4_pattern_circular f4_primitives
                 pattern_at_points f4_pattern_face_copy f4_pattern_merging_join)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()
  # Copies of bodies joined to them, scale patterns and copies scaled
  # about their own centres (mitcad#539).
  add_test(NAME cli.f4_pattern_scaled COMMAND mitcad-cli run "${scripts}/f4_pattern_scaled.json")
  # Pattern on Path along a body's edges, with and without a tangent chain (mitcad#2105).
  add_test(NAME cli.f4_pattern_edge_path COMMAND mitcad-cli run "${scripts}/f4_pattern_edge_path.json")
  # Cone (mitcad#2239): a cone's and a frustum's volumes, its faces, a
  # negative height, a fillet on the base edge through a radius change,
  # equal radii refused, a cone cut from a box.
  add_test(NAME cli.primitive_cone COMMAND mitcad-cli run "${scripts}/primitive_cone.json")

  # Face operations (F2): the T0 models' volumes, names through edits, failures.
  foreach(script f2_fillet_chamfer f2_shell_draft f2_face_ops f2_names_follow_parameters f2_failures)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()
  # Offset Face on a piece of a split face: the seam becomes a step (mitcad#1928).
  foreach(script offset_split_face)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()

  # The general sketch (S1): drawing tools, planes, frames and projections
  # extruded, with analytic volumes.
  foreach(script s1_sketch_tools)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()
  # Sketch text, patterns and offsets (P3, P4): extruded letters against
  # their regions' areas, patterned holes and an ellipse's offset.
  add_test(NAME cli.p34_text_patterns COMMAND mitcad-cli run "${scripts}/p34_text_patterns.json")

  # Sweeps, lofts, pipes, coils, ribs and webs (F3): the T0 models' volumes
  # and analytic ones, names through parameter edits, failures.
  foreach(script f3_sweep f3_loft f3_pipe_coil f3_rib_web f3_names_follow_parameters f3_failures
                 f3_rib_extend_ends f3_rib_direction f3_web_corners f3_rib_web_pattern)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()
  # A rib's and a web's Draft Angle (mitcad#2102): drafted, edited, patterned.
  add_test(NAME cli.f3_rib_web_draft COMMAND mitcad-cli run "${scripts}/f3_rib_web_draft.json")
  # A rib's Direction Normal to Curves (mitcad#2398): a three-line chain with
  # a slanted middle offset 30 as its far side, sharp and round corners,
  # flipped, drafted and with Extend Ends; analytic volumes and the far
  # faces 30 from the curves' faces.
  add_test(NAME cli.f3_rib_normal COMMAND mitcad-cli run "${scripts}/f3_rib_normal.json")
  # A loft's Tangent Edges merged (mitcad#2162): a slot to a circle is one
  # side face, with two and three sections and a tangent end; fillets on the
  # merged faces build through the slot edited into a rectangle and back.
  add_test(NAME cli.f3_loft_tangent_edges COMMAND mitcad-cli run "${scripts}/f3_loft_tangent_edges.json")
  # Plastic part features and plastic rules (mitcad#459): analytic volumes,
  # the rules' defaults and warnings, refused rims and hooks.
  add_test(NAME cli.plastic COMMAND mitcad-cli run "${scripts}/plastic.json")
  # Gears (mitcad#472).
  foreach(script gears gears_link)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()
  # Timing belt pulleys (mitcad#562).
  add_test(NAME cli.pulleys COMMAND mitcad-cli run "${scripts}/pulleys.json")
  # Printable bearings (mitcad#577): the bodies solids that do not touch and
  # keep their clearances; the 608's bodies written as the parts of one
  # 3MF object.
  set(bearings "${CMAKE_CURRENT_BINARY_DIR}/bearings")
  file(MAKE_DIRECTORY "${bearings}")
  add_test(NAME cli.bearings
    COMMAND mitcad-cli run "${scripts}/bearings.json" --save "${bearings}/bearings.mitcad")
  set_tests_properties(cli.bearings PROPERTIES FIXTURES_SETUP cli_bearings)
  add_test(NAME cli.bearings_3mf
    COMMAND mitcad-cli export "${bearings}/bearings.mitcad" "${bearings}/bearing.3mf"
            --bodies F1.b0,F1.b1,F1.b2,F1.b3,F1.b4,F1.b5,F1.b6,F1.b7,F1.b8,F1.b9,F1.b10,F1.b11)
  set_tests_properties(cli.bearings_3mf PROPERTIES FIXTURES_REQUIRED cli_bearings PASS_REGULAR_EXPRESSION
    "\"format\":\"3mf\",\"meshes\":\\[{\"body\":\"F1\\.b0\"[^]]*\"body\":\"F1\\.b1\"[^]]*\"body\":\"F1\\.b11\"[^]]*\\]")
  # Mortise and Tenon (mitcad#1012): crossing boards, blind, through and with
  # rounded corners, the mortise the tenon grown by the clearance; refusals.
  add_test(NAME cli.mortise_tenon COMMAND mitcad-cli run "${scripts}/mortise_tenon.json")
  # The curve tools (mitcad#1777): a Helix Curve on a cylinder's face and a
  # growing one about an axis, pipes along them; a surface Helix of a line,
  # the helicoid's area; its Intersection Curve with a cylinder, a pipe along
  # it as along the Helix Curve.
  add_test(NAME cli.curve_tools COMMAND mitcad-cli run "${scripts}/curve_tools.json")

  # Components and occurrences (F6): placements, subassemblies, timeline
  # moves, components made by features and copies, with world-space
  # volumes, boxes and centres of mass; then external components: the
  # assembly saved, inserted linked and as a copy, edited, and the link
  # following the edit when the design opens again.
  add_test(NAME cli.f6_components COMMAND mitcad-cli run "${scripts}/f6_components.json")
  # Derives: linked copies of other components' bodies (mitcad#324).
  add_test(NAME cli.f6_derive COMMAND mitcad-cli run "${scripts}/f6_derive.json")
  # The BUILD tab (mitcad#375): profiles placed on sides and ends, bound,
  # mitred and connected, from the synthetic sources of build_parts.mitcad;
  # build_corners: a connector with its nut at each corner of a frame
  # (mitcad#1115).
  foreach(script build_frame build_connect build_corners)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json"
                                          --open "${scripts}/build_parts.mitcad")
  endforeach()
  # A design of the first rule's connectors opens when placing them again
  # is refused: the report with a warning (mitcad#1151).
  add_test(NAME cli.build_upgrade_refused
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DSCRIPTS=${scripts}"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/build-upgrade-refused"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/build-upgrade-refused-test.cmake")
  # Explicit move destinations keep real OCCT geometry and external support
  # links in place across nested rotations, undo/redo and save/reopen (#201).
  set(move_destination "${CMAKE_CURRENT_BINARY_DIR}/move-destination.mitcad")
  add_test(NAME cli.move_destination COMMAND mitcad-cli run "${scripts}/move_destination.json"
           --save "${move_destination}")
  set_tests_properties(cli.move_destination PROPERTIES FIXTURES_SETUP cli_move_destination)
  add_test(NAME cli.move_destination_reopened
           COMMAND mitcad-cli run "${scripts}/move_destination_reopened.json" --open "${move_destination}")
  set_tests_properties(cli.move_destination_reopened PROPERTIES FIXTURES_REQUIRED cli_move_destination)
  # Moving an occurrence to another parent keeps real OCCT geometry and the
  # links through it in place across turned parents, undo/redo and
  # save/reopen (mitcad#219).
  set(reparent_occurrence "${CMAKE_CURRENT_BINARY_DIR}/reparent-occurrence.mitcad")
  add_test(NAME cli.reparent_occurrence COMMAND mitcad-cli run "${scripts}/reparent_occurrence.json"
           --save "${reparent_occurrence}")
  set_tests_properties(cli.reparent_occurrence PROPERTIES FIXTURES_SETUP cli_reparent_occurrence)
  add_test(NAME cli.reparent_occurrence_reopened
           COMMAND mitcad-cli run "${scripts}/reparent_occurrence_reopened.json" --open "${reparent_occurrence}")
  set_tests_properties(cli.reparent_occurrence_reopened PROPERTIES FIXTURES_REQUIRED cli_reparent_occurrence)
  # Texture map sets embedded once per image, saved and opened again (mitcad#313).
  set(texture_maps "${CMAKE_CURRENT_BINARY_DIR}/texture-maps.mitcad")
  add_test(NAME cli.texture_maps COMMAND mitcad-cli run "${scripts}/texture_maps.json" --save "${texture_maps}")
  set_tests_properties(cli.texture_maps PROPERTIES FIXTURES_SETUP cli_texture_maps)
  add_test(NAME cli.texture_maps_reopened
           COMMAND mitcad-cli run "${scripts}/texture_maps_reopened.json" --open "${texture_maps}")
  set_tests_properties(cli.texture_maps_reopened PROPERTIES FIXTURES_REQUIRED cli_texture_maps)
  set(f6 "${CMAKE_CURRENT_BINARY_DIR}/f6")
  file(MAKE_DIRECTORY "${f6}")
  add_test(NAME cli.f6_assembly COMMAND mitcad-cli run "${scripts}/f6_assembly.json" --save "${f6}/plate.mitcad")
  set_tests_properties(cli.f6_assembly PROPERTIES FIXTURES_SETUP cli_f6_part)
  add_test(NAME cli.f6_insert COMMAND mitcad-cli run "${scripts}/f6_insert.json" --save "${f6}/linked.mitcad")
  set_tests_properties(cli.f6_insert PROPERTIES WORKING_DIRECTORY "${f6}"
    FIXTURES_REQUIRED cli_f6_part FIXTURES_SETUP cli_f6_linked)
  add_test(NAME cli.f6_part_edit COMMAND mitcad-cli run "${scripts}/f6_part_edit.json"
                                         --open "${f6}/plate.mitcad" --save "${f6}/plate.mitcad")
  set_tests_properties(cli.f6_part_edit PROPERTIES FIXTURES_REQUIRED cli_f6_linked FIXTURES_SETUP cli_f6_edited)
  add_test(NAME cli.f6_link_update COMMAND mitcad-cli info "${f6}/linked.mitcad")
  set_tests_properties(cli.f6_link_update PROPERTIES FIXTURES_REQUIRED cli_f6_edited PASS_REGULAR_EXPRESSION
    "Warning: plate is updated from plate\\.mitcad.*Placed bodies: 8.*plate:1/Plate:1/Body1 \\(F2\\.b0\\): volume 8000\\.000 mm3")

  # A version 1 file of V0 opens and recomputes to the same geometry.
  add_test(NAME cli.info_v1 COMMAND mitcad-cli info "${scripts}/v1_boss.mitcad")
  set_tests_properties(cli.info_v1 PROPERTIES PASS_REGULAR_EXPRESSION
    "F6 Fillet1 \\(fillet\\): ok.*Body1 \\(F2\\.b0\\): volume 48403\\.451 mm3")
  # OCCT's crash handlers (catch_occt_crashes): the same file with its
  # fillet crashing inside the geometry kernel (a test hook) reports the
  # fillet failed and the bodies before it, instead of crashing. On Linux
  # OCCT's handler names the signal (on macOS it is SIGSEGV or SIGBUS, as a
  # fault can arrive as either). On Windows a crash outside OCCT's own
  # code (compiled with /EHa) reaches OCCT's handler through the C
  # runtime's SIGSEGV, which names the access violation (with the OCCT
  # port's patch 0034; without it OCCT 8 locked its mutex twice and the
  # error was "resource deadlock would occur").
  if(WIN32)
    set(crash_error "error: fillet: ACCESS VIOLATION")
  elseif(APPLE)
    set(crash_error "error: fillet: SIG(SEGV|BUS)")
  else()
    set(crash_error "error: fillet: SIGSEGV")
  endif()
  add_test(NAME cli.occt_crash COMMAND mitcad-cli info "${scripts}/v1_boss.mitcad")
  set_tests_properties(cli.occt_crash PROPERTIES ENVIRONMENT "MITCAD_TEST_OCCT_CRASH=fillet"
    PASS_REGULAR_EXPRESSION "F6 Fillet1 \\(fillet\\): ${crash_error}.*Body1 \\(F2\\.b0\\): volume")
  # So is a crash in the section analysis' booleans (mitcad#392): the
  # section query of the analysis script fails with the error.
  # Since the bodies are clipped in parallel (mitcad#438) Windows names the
  # body and may gather several threads' crashes ("Body1: Multiple
  # exceptions: OSD_Exception_ACCESS_VIOLATION: ACCESS VIOLATION ...").
  if(WIN32)
    set(crash_error "(section|Body[0-9]+): .*ACCESS VIOLATION")
  elseif(APPLE)
    set(crash_error "section: SIG(SEGV|BUS)")
  else()
    set(crash_error "section: SIGSEGV")
  endif()
  add_test(NAME cli.occt_crash_section COMMAND mitcad-cli run "${scripts}/analysis.json")
  set_tests_properties(cli.occt_crash_section PROPERTIES ENVIRONMENT "MITCAD_TEST_OCCT_CRASH=section"
    PASS_REGULAR_EXPRESSION "steps\\[[0-9]+\\]: .*${crash_error}")
  # A fillet whose kernel call does not return (a test hang of a minute,
  # deaf to any cancel, mitcad#1917) is given up at the kernel's time limit:
  # the feature fails in words, the bodies are reported, and the process
  # ends at once although the call still runs (without its static
  # destructors).
  add_test(NAME cli.kernel_time_limit COMMAND mitcad-cli info "${scripts}/v1_boss.mitcad")
  set_tests_properties(cli.kernel_time_limit PROPERTIES
    ENVIRONMENT "MITCAD_TEST_KERNEL_HANG=60;MITCAD_KERNEL_TIME_LIMIT=1" TIMEOUT 30
    PASS_REGULAR_EXPRESSION
      "F6 Fillet1 \\(fillet\\): error: the geometry kernel did not finish within 1 s. the operation was given up.*Body1 \\(F2\\.b0\\): volume")

  # Saving the result of a script, then reading it back.
  set(saved "${CMAKE_CURRENT_BINARY_DIR}/join.mitcad")
  add_test(NAME cli.save COMMAND mitcad-cli run "${scripts}/join.json" --save "${saved}")
  set_tests_properties(cli.save PROPERTIES FIXTURES_SETUP cli_saved)
  add_test(NAME cli.info_saved COMMAND mitcad-cli info "${saved}" --json)
  set_tests_properties(cli.info_saved PROPERTIES FIXTURES_REQUIRED cli_saved
    PASS_REGULAR_EXPRESSION "\"name\": \"Body1\"")

  add_test(NAME cli.failed_expectation COMMAND mitcad-cli run "${scripts}/failing.json")
  set_tests_properties(cli.failed_expectation PROPERTIES WILL_FAIL TRUE)

  # An item put on another print plate says which plate it left
  # (mitcad#1595); the line comes only after every expectation passed.
  add_test(NAME cli.plate_move COMMAND mitcad-cli run "${scripts}/plate_move.json")
  set_tests_properties(cli.plate_move PROPERTIES PASS_REGULAR_EXPRESSION
    "Plates: Body1 moved from one to two \\(count 2\\)\n.*Body1 \\(F1\\.b0\\)")

  # Construction geometry and analysis (F5): datums checked through queries,
  # physical properties, measurement and comparison with a STEP file.
  foreach(script construction analysis holed_block face_separation face_geometry print_min_gap)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()
  add_test(NAME cli.properties COMMAND mitcad-cli run "${scripts}/analysis.json" --properties)
  set_tests_properties(cli.properties PROPERTIES PASS_REGULAR_EXPRESSION
    "Physical properties:.*Body1 \\(F2\\.b0\\): Steel, density 7\\.85 g/cm3.*mass 0\\.376800 kg.*Body2 \\(F4\\.b0\\): Aluminum")
  add_test(NAME cli.properties_json
    COMMAND mitcad-cli info "${scripts}/v1_boss.mitcad" --json --properties --density 1)
  set_tests_properties(cli.properties_json PROPERTIES PASS_REGULAR_EXPRESSION
    "\"properties\": {.*\"density\": 1\\.0")
  set(top "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})")
  set(bottom "F2:start(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})")
  add_test(NAME cli.measure
    COMMAND mitcad-cli measure "${scripts}/holed_block.json" "Body1/${top}" "Body1/${bottom}")
  # ASCII only: on Windows ctest decodes the output in the console code page,
  # so the UTF-8 "°" after the angle would not match.
  set_tests_properties(cli.measure PROPERTIES PASS_REGULAR_EXPRESSION
    "area 1386\\.902.*Distance: 20\\.000000 mm.*Angle: 180\\.000000")
  set(step "${CMAKE_CURRENT_BINARY_DIR}/holed-block.step")
  add_test(NAME cli.compare_step_file COMMAND mitcad-geomtool make holed-block "${step}")
  set_tests_properties(cli.compare_step_file PROPERTIES FIXTURES_SETUP cli_step)
  add_test(NAME cli.compare_step COMMAND mitcad-cli compare-step "${scripts}/holed_block.json"
    "${step}" --max-deviation 1e-4 --max-relative 1e-6)
  set_tests_properties(cli.compare_step PROPERTIES FIXTURES_REQUIRED cli_step
    PASS_REGULAR_EXPRESSION "Within the limits")
  # A different body exceeds the limit: exit status 1.
  add_test(NAME cli.compare_step_differs COMMAND mitcad-cli compare-step
    "${scripts}/construction.json" "${step}" --body F2.b0 --step-body 0 --max-deviation 0.01 --json)
  set_tests_properties(cli.compare_step_differs PROPERTIES FIXTURES_REQUIRED cli_step WILL_FAIL TRUE)
  # Data exchange: round trips through every format inside a script (files
  # in its working directory), then the export, import and import-f3d
  # commands on its design and on a small .f3d file.
  set(exchange "${CMAKE_CURRENT_BINARY_DIR}/exchange")
  file(MAKE_DIRECTORY "${exchange}")
  add_test(NAME cli.exchange
    COMMAND mitcad-cli run "${scripts}/exchange.json" --save "${exchange}/exchange.mitcad")
  set_tests_properties(cli.exchange PROPERTIES WORKING_DIRECTORY "${exchange}"
    FIXTURES_SETUP cli_exchange)
  add_test(NAME cli.export
    COMMAND mitcad-cli export "${exchange}/exchange.mitcad" "${exchange}/out.step"
            --bodies Bracket,F4.b0 --schema ap242 --unit in)
  set_tests_properties(cli.export PROPERTIES FIXTURES_REQUIRED cli_exchange
    FIXTURES_SETUP cli_exported PASS_REGULAR_EXPRESSION "\"bodies\":\\[\"F2\\.b0\",\"F4\\.b0\"\\]")
  add_test(NAME cli.import COMMAND mitcad-cli import "${exchange}/out.step" --structure base
                                   --save "${exchange}/imported.mitcad")
  set_tests_properties(cli.import PROPERTIES FIXTURES_REQUIRED cli_exported PASS_REGULAR_EXPRESSION
    "Bracket \\(F1\\.b0\\): volume 23946\\.350 mm3.*Bracket \\(2\\) \\(F1\\.b1\\): volume 47892\\.699 mm3")
  # The same file, an assembly that places both bodies' parts (one moved),
  # with its structure kept (mitcad#1274): a component per part.
  add_test(NAME cli.import_components COMMAND mitcad-cli import "${exchange}/out.step")
  set_tests_properties(cli.import_components PROPERTIES FIXTURES_REQUIRED cli_exported PASS_REGULAR_EXPRESSION
    "C1 Bracket: 1 features, 1 occurrences.*C2 Bracket \\(2\\): 1 features, 1 occurrences.*Bracket:1/Bracket \\(F1\\.b0\\): volume 23946\\.350 mm3, center of mass \\[79\\.958")
  # Paths with letters outside ASCII, which failed on Windows before the
  # UTF-8 code page (cmake/Utf8CodePage.cmake).
  add_test(NAME cli.non_ascii_paths
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DSCRIPT=${scripts}/join.json"
            "-DDIR=${CMAKE_CURRENT_BINARY_DIR}/non-ascii-paths"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/non-ascii-paths-test.cmake")
  add_test(NAME cli.export_mesh
    COMMAND mitcad-cli export "${exchange}/exchange.mitcad" "${exchange}/out.stl"
            --refinement low --ascii)
  set_tests_properties(cli.export_mesh PROPERTIES FIXTURES_REQUIRED cli_exchange
    PASS_REGULAR_EXPRESSION "\"format\":\"stl\"")
  # 3MF for slicers (mitcad#13): the bodies as the parts of one object; the
  # Bracket's mesh (23946.35 mm3 after the script's change) within the
  # refinement's deviation.
  add_test(NAME cli.export_3mf
    COMMAND mitcad-cli export "${exchange}/exchange.mitcad" "${exchange}/out.3mf" --refinement high)
  set_tests_properties(cli.export_3mf PROPERTIES FIXTURES_REQUIRED cli_exchange PASS_REGULAR_EXPRESSION
    "\"format\":\"3mf\",\"meshes\":\\[{\"body\":\"F2\\.b0\",\"name\":\"Bracket\",\"triangles\":[0-9]+,\"volume\":2394[56]\\.")
  # Mesh bodies (an STL file's triangles) in surface operations
  # (mitcad#399): refused by name, left out of all bodies.
  set(mesh_bodies "${CMAKE_CURRENT_BINARY_DIR}/mesh-bodies")
  file(MAKE_DIRECTORY "${mesh_bodies}")
  add_test(NAME cli.mesh_bodies COMMAND mitcad-cli run "${scripts}/mesh_bodies.json")
  set_tests_properties(cli.mesh_bodies PROPERTIES WORKING_DIRECTORY "${mesh_bodies}")
  # USDZ for AR viewers (mitcad#123): the exchange design's shown bodies
  # as shaded meshes with their materials.
  add_test(NAME cli.export_usdz
    COMMAND mitcad-cli export "${exchange}/exchange.mitcad" "${exchange}/out.usdz" --refinement low)
  set_tests_properties(cli.export_usdz PROPERTIES FIXTURES_REQUIRED cli_exchange PASS_REGULAR_EXPRESSION
    "\"format\":\"usdz\",\"materials\":\\[[^]]+\\].*\"warnings\":\\[\\]")
  # Animated USDZ (mitcad#1597): two boxes, one moved over three frames by
  # a run script; two_boxes.usdz stays in the folder for OpenUSD's reading
  # (tools/usdz-check/check-animation.py, docs/development.md).
  set(usdz_animation "${CMAKE_CURRENT_BINARY_DIR}/usdz-animation")
  file(MAKE_DIRECTORY "${usdz_animation}")
  add_test(NAME cli.export_usdz_animation COMMAND mitcad-cli run "${scripts}/usdz_animation.json")
  set_tests_properties(cli.export_usdz_animation PROPERTIES WORKING_DIRECTORY "${usdz_animation}")
  # Exports in a coordinate system's frame (mitcad#1775): a block written
  # to STEP, IGES and STL in a frame at its corner and read back there.
  set(coordinate_system "${CMAKE_CURRENT_BINARY_DIR}/coordinate-system")
  file(MAKE_DIRECTORY "${coordinate_system}")
  add_test(NAME cli.export_coordinate_system COMMAND mitcad-cli run "${scripts}/coordinate_system.json")
  set_tests_properties(cli.export_coordinate_system PROPERTIES WORKING_DIRECTORY "${coordinate_system}")
  # The bodies in their components' coordinates (mitcad#19).
  add_test(NAME cli.export_component
    COMMAND mitcad-cli export "${exchange}/exchange.mitcad" "${exchange}/local-out.step"
            --bodies Bracket --coordinates component)
  set_tests_properties(cli.export_component PROPERTIES FIXTURES_REQUIRED cli_exchange
    PASS_REGULAR_EXPRESSION "\"bodies\":\\[\"F2\\.b0\"\\],\"error\":null,\"format\":\"step\"")
  add_test(NAME cli.export_sketch
    COMMAND mitcad-cli export-sketch "${exchange}/exchange.mitcad" Sketch1 "${exchange}/sketch.dxf" --r12)
  set_tests_properties(cli.export_sketch PROPERTIES FIXTURES_REQUIRED cli_exchange
    PASS_REGULAR_EXPRESSION "\"entities\":4")
  # Design files of version 3 (P12a): the exchange design's base features
  # converted into a project's B-rep store and back.
  add_test(NAME cli.project_v3
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DSOURCE=${exchange}/exchange.mitcad"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/project-v3" -P "${CMAKE_CURRENT_SOURCE_DIR}/project-test.cmake")
  set_tests_properties(cli.project_v3 PROPERTIES FIXTURES_REQUIRED cli_exchange)
  # Version history (P12b): versions of the exchange design in a project
  # with history, an older version opened and restored; checked with the
  # system's git where it is installed.
  find_package(Git QUIET)
  add_test(NAME cli.version_history
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DSOURCE=${exchange}/exchange.mitcad"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/version-history" "-DGIT=${GIT_EXECUTABLE}"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/history-test.cmake")
  set_tests_properties(cli.version_history PROPERTIES FIXTURES_REQUIRED cli_exchange)
  # Comparison of versions (P12c): two versions of a block, a version and
  # the saved file, and two files, as text and JSON, with volumes and areas.
  add_test(NAME cli.diff
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/diff"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/diff-test.cmake")
  # Remote repositories (P12 remote): the exchange design's project connected to a
  # bare repository, opened from it, pushed and fetched both ways; needs the
  # system's git.
  if(GIT_FOUND)
    add_test(NAME cli.remote
      COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DGIT=${GIT_EXECUTABLE}"
              "-DSOURCE=${exchange}/exchange.mitcad" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/remote"
              -P "${CMAKE_CURRENT_SOURCE_DIR}/remote-test.cmake")
    set_tests_properties(cli.remote PROPERTIES FIXTURES_REQUIRED cli_exchange)
  endif()
  # A project's folders and designs (mitcad#144): made, listed, moved and
  # deleted as versions, every refusal, and with git a Cloud project's push.
  add_test(NAME cli.project_folders
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DGIT=${GIT_EXECUTABLE}"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/project-folders" -P "${CMAKE_CURRENT_SOURCE_DIR}/folders-test.cmake")

  # .f3d bodies without history: Mitcad's own ASM test bodies in an .f3d.
  add_test(NAME cli.f3d_file COMMAND test_exchange --write-f3d "${exchange}/bodies.f3d")
  set_tests_properties(cli.f3d_file PROPERTIES FIXTURES_SETUP cli_f3d)
  add_test(NAME cli.import_f3d COMMAND mitcad-cli import-f3d "${exchange}/bodies.f3d" --bodies-only
                                       --save "${exchange}/bodies.mitcad")
  set_tests_properties(cli.import_f3d PROPERTIES FIXTURES_REQUIRED cli_f3d FIXTURES_SETUP cli_f3d_saved
    PASS_REGULAR_EXPRESSION
    "Imported 2 bodies of bodies\\.f3d.*Body1 \\(F1\\.b0\\): volume 1000\\.000 mm3.*Body2 \\(F2\\.b0\\): volume 6283\\.185 mm3")
  add_test(NAME cli.info_f3d COMMAND mitcad-cli info "${exchange}/bodies.mitcad")
  set_tests_properties(cli.info_f3d PROPERTIES FIXTURES_REQUIRED cli_f3d_saved PASS_REGULAR_EXPRESSION
    "F1 Base1 \\(base\\): ok.*Body1 \\(F1\\.b0\\): volume 1000\\.000 mm3.*Body2 \\(F2\\.b0\\): volume 6283\\.185 mm3")
  # Without --bodies-only the timeline import (T1): this file has no
  # timeline, so its stored bodies come in as one base feature, with the
  # report's warning about it as its import note (mitcad#170).
  add_test(NAME cli.import_f3d_timeline COMMAND mitcad-cli import-f3d "${exchange}/bodies.f3d")
  set_tests_properties(cli.import_f3d_timeline PROPERTIES FIXTURES_REQUIRED cli_f3d PASS_REGULAR_EXPRESSION
    "bodies\\.f3d: 0 timeline items.*F1 Stored bodies \\(base\\): warning: Nothing replayed: the file's bodies come in as they are \\(F1\\).*Body1 \\(F1\\.b0\\): volume 1000\\.000 mm3")
  # What retries on the file's exact state do with the bodies that match
  # it (mitcad#283): keep or replace; anything else is refused.
  add_test(NAME cli.import_f3d_retry_bodies
    COMMAND mitcad-cli import-f3d "${exchange}/bodies.f3d" --retry-bodies replace)
  set_tests_properties(cli.import_f3d_retry_bodies PROPERTIES FIXTURES_REQUIRED cli_f3d PASS_REGULAR_EXPRESSION
    "bodies\\.f3d: 0 timeline items")
  add_test(NAME cli.import_f3d_retry_bodies_refused
    COMMAND mitcad-cli import-f3d "${exchange}/bodies.f3d" --retry-bodies ask)
  set_tests_properties(cli.import_f3d_retry_bodies_refused PROPERTIES FIXTURES_REQUIRED cli_f3d
    PASS_REGULAR_EXPRESSION "--retry-bodies needs keep or replace")
  # The own-start time scale (mitcad#847): taken with a factor, refused
  # without one above 0.
  add_test(NAME cli.import_f3d_own_start_time_scale
    COMMAND mitcad-cli import-f3d "${exchange}/bodies.f3d" --own-start-time-scale 3)
  set_tests_properties(cli.import_f3d_own_start_time_scale PROPERTIES FIXTURES_REQUIRED cli_f3d
    PASS_REGULAR_EXPRESSION "bodies\\.f3d: 0 timeline items")
  add_test(NAME cli.import_f3d_own_start_time_scale_refused
    COMMAND mitcad-cli import-f3d "${exchange}/bodies.f3d" --own-start-time-scale 0)
  set_tests_properties(cli.import_f3d_own_start_time_scale_refused PROPERTIES FIXTURES_REQUIRED cli_f3d
    PASS_REGULAR_EXPRESSION "--own-start-time-scale needs a factor greater than 0")
  # A rounding whose result deviates from the file's (mitcad#379): a
  # synthetic design (a cube whose history bevels an edge, its timeline a
  # dump with a rounding of that edge) comes in with the file's bevel and
  # the deviation listed by default, and with the rounding kept parametric
  # with --accept-deviations, the final body keeping its deviation.
  add_test(NAME cli.f3d_deviation_files COMMAND test_exchange --write-f3d-deviation "${exchange}/deviation")
  set_tests_properties(cli.f3d_deviation_files PROPERTIES FIXTURES_SETUP cli_f3d_deviation)
  add_test(NAME cli.import_f3d_deviation
    COMMAND mitcad-cli import-f3d "${exchange}/deviation/Rounding.f3d" --dump "${exchange}/deviation/Rounding.dump.json")
  set_tests_properties(cli.import_f3d_deviation PROPERTIES FIXTURES_REQUIRED cli_f3d_deviation
    PASS_REGULAR_EXPRESSION
    "Fillet1 +FilletFeature +fallback[^\n]*\n +deviating result \\(not kept\\): volume change -8\\.5841 mm³ against the file's -20\\.0000 \\(\\+57\\.1 %\\).*Body1 \\(F2\\.b0\\): volume 980\\.000 mm3")
  add_test(NAME cli.import_f3d_deviation_kept
    COMMAND mitcad-cli import-f3d "${exchange}/deviation/Rounding.f3d" --dump "${exchange}/deviation/Rounding.dump.json"
            --accept-deviations 2)
  set_tests_properties(cli.import_f3d_deviation_kept PROPERTIES FIXTURES_REQUIRED cli_f3d_deviation
    PASS_REGULAR_EXPRESSION
    "Fillet1 +FilletFeature +parametric -> F3 \\(not verified\\): its result deviates from the file's. kept parametric at the user's choice: volume change -8\\.5841.*F3 Fillet1 \\(fillet\\): warning: Deviation from the file accepted at import.*Body1 \\(F2\\.b0\\): volume 991\\.416 mm3")
  add_test(NAME cli.import_f3d_deviation_refused
    COMMAND mitcad-cli import-f3d "${exchange}/deviation/Rounding.f3d" --accept-deviations 2,,3)
  set_tests_properties(cli.import_f3d_deviation_refused PROPERTIES FIXTURES_REQUIRED cli_f3d_deviation
    PASS_REGULAR_EXPRESSION "--accept-deviations needs all, none or timeline indices separated by commas")
  # After an import whose geometry kernel hung (mitcad#82): the exit status
  # of the import (1 when every try hung, 0 when one finished) while the
  # threads the hang watchdog gave up still run, not a crash at exit.
  add_test(NAME cli.import_f3d_hang_exit
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DFILE=${exchange}/bodies.f3d"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/hang-exit-test.cmake")
  set_tests_properties(cli.import_f3d_hang_exit PROPERTIES FIXTURES_REQUIRED cli_f3d)
  # Components of other documents (mitcad#196): a synthetic assembly that
  # places a part once and a sub-assembly (which places the part) twice,
  # with its reference folder next to it; then the same with the folder
  # named otherwise and without references.json: the documents are
  # missing (their components stay empty, the report names them) until the
  # folder is given with --references, where the files' GUIDs tell them.
  add_test(NAME cli.f3d_references_files
    COMMAND test_exchange --write-f3d-references "${exchange}/references")
  add_test(NAME cli.f3d_references_moved_files
    COMMAND test_exchange --write-f3d-references "${exchange}/references-moved" parts)
  set_tests_properties(cli.f3d_references_files cli.f3d_references_moved_files PROPERTIES
    FIXTURES_SETUP cli_f3d_references)
  add_test(NAME cli.import_f3d_references
    COMMAND mitcad-cli import-f3d "${exchange}/references/Assembly v1.f3d")
  set_tests_properties(cli.import_f3d_references PROPERTIES FIXTURES_REQUIRED cli_f3d_references
    PASS_REGULAR_EXPRESSION
    "3 of other documents \\(3 filled from their files\\).*documents referred to: 2 linked from their files, 0 not.*Part \\(version 2\\): linked, occurrences: 1, from [^\n]*Part v2\\.f3d \\(reference folder\\).*Holder \\(version 1\\): linked, occurrences: 2.*Placed bodies: 3.*center of mass \\[55\\.000, 5\\.000, 5\\.000\\].*center of mass \\[5\\.000, 55\\.000, 25\\.000\\].*center of mass \\[5\\.000, 105\\.000, 25\\.000\\]")
  add_test(NAME cli.import_f3d_references_missing
    COMMAND mitcad-cli import-f3d "${exchange}/references-moved/Assembly v1.f3d")
  set_tests_properties(cli.import_f3d_references_missing PROPERTIES FIXTURES_REQUIRED cli_f3d_references
    PASS_REGULAR_EXPRESSION
    "documents referred to: 0 linked from their files, 2 not.*the document of Inserted component \\(version 2\\): missing.*warning: components of other documents stay empty.*Placed bodies: 0")
  add_test(NAME cli.import_f3d_references_folder
    COMMAND mitcad-cli import-f3d "${exchange}/references-moved/Assembly v1.f3d"
            --references "${exchange}/references-moved/parts")
  set_tests_properties(cli.import_f3d_references_folder PROPERTIES FIXTURES_REQUIRED cli_f3d_references
    PASS_REGULAR_EXPRESSION
    "documents referred to: 2 linked from their files, 0 not.*\\(by its GUID\\).*Placed bodies: 3")
  # The same assembly in an .f3z package with the documents it refers to
  # (mitcad#229), whose manifests list the GUIDs of the documents they refer
  # to as well as their own, and whose root sorts first: each reference
  # comes from its own design of the package (by DesignDescription.json,
  # named from it; without it by the GUIDs, never the root or a design
  # being imported), the root is never imported for a reference, and the
  # part (referred to by the assembly and the holder) is imported once.
  add_test(NAME cli.f3d_references_package_files
    COMMAND test_exchange --write-f3d-package "${exchange}/package")
  set_tests_properties(cli.f3d_references_package_files PROPERTIES FIXTURES_SETUP cli_f3d_package)
  add_test(NAME cli.import_f3d_references_package
    COMMAND mitcad-cli import-f3d "${exchange}/package/Package.f3z")
  set_tests_properties(cli.import_f3d_references_package PROPERTIES FIXTURES_REQUIRED cli_f3d_package
    ENVIRONMENT "MITCAD_IMPORT_TRACE=1"
    PASS_REGULAR_EXPRESSION
    "import: the document urn:mitcad\\.test:fs\\.file:vf\\.part\\?version=2 again, imported before.*3 of other documents \\(3 filled from their files\\).*documents referred to: 2 linked from their files, 0 not.*Packaged part \\(version 2\\): linked, occurrences: 1, from [^\n]*Package\\.f3z \\(package\\).*Packaged holder \\(version 1\\): linked, occurrences: 2, from [^\n]*Package\\.f3z \\(package\\).*Placed bodies: 3.*center of mass \\[55\\.000, 5\\.000, 5\\.000\\].*center of mass \\[5\\.000, 55\\.000, 25\\.000\\].*center of mass \\[5\\.000, 105\\.000, 25\\.000\\]"
    FAIL_REGULAR_EXPRESSION
    "from [^\n]*!0a5e3b10\\);vf\\.part\\?version=2 from.*vf\\.part\\?version=2 from;refers back")
  add_test(NAME cli.import_f3d_references_package_plain
    COMMAND mitcad-cli import-f3d "${exchange}/package/Package-plain.f3z" --design 0a5e3b10)
  set_tests_properties(cli.import_f3d_references_package_plain PROPERTIES FIXTURES_REQUIRED cli_f3d_package
    ENVIRONMENT "MITCAD_IMPORT_TRACE=1"
    PASS_REGULAR_EXPRESSION
    "documents referred to: 2 linked from their files, 0 not.*occurrences: 1, from [^\n]*Package-plain\\.f3z \\(package\\).*Placed bodies: 3"
    FAIL_REGULAR_EXPRESSION
    "from [^\n]*!0a5e3b10\\);vf\\.part\\?version=2 from.*vf\\.part\\?version=2 from;refers back")

  # The .ipt import (mitcad#60): Mitcad's own test part (a cube and a
  # cylinder, inches, aluminium), saved and opened, and compared with a
  # STEP file of its bodies. Real part files under MITCAD_IPT_CORPUS are
  # imported and compared with their STEP references (ipt-corpus.cmake),
  # several at once; skipped without the corpus.
  add_test(NAME cli.ipt_file COMMAND test_exchange --write-ipt "${exchange}/part.ipt")
  set_tests_properties(cli.ipt_file PROPERTIES FIXTURES_SETUP cli_ipt)
  add_test(NAME cli.import_ipt COMMAND mitcad-cli import-ipt "${exchange}/part.ipt" --save "${exchange}/part.mitcad")
  set_tests_properties(cli.import_ipt PROPERTIES FIXTURES_REQUIRED cli_ipt FIXTURES_SETUP cli_ipt_saved
    PASS_REGULAR_EXPRESSION
    "Imported 2 bodies of part\\.ipt \\(2 solids, 2 valid. 0 not built\\).*part number: MITCAD-TEST-1. material: Aluminum \\(MitCad's aluminum\\). units: in.*Body1 \\(F1\\.b0\\): valid solid, volume 1000\\.000 mm3.*Body2 \\(F2\\.b0\\): valid solid, volume 6283\\.185 mm3")
  add_test(NAME cli.info_ipt COMMAND mitcad-cli info "${exchange}/part.mitcad")
  set_tests_properties(cli.info_ipt PROPERTIES FIXTURES_REQUIRED cli_ipt_saved
    PASS_REGULAR_EXPRESSION "F1 Base1 \\(base\\): ok.*F2 Base2 \\(base\\): ok.*Body2 \\(F2\\.b0\\): volume 6283\\.185 mm3")
  add_test(NAME cli.export_ipt COMMAND mitcad-cli export "${exchange}/part.mitcad" "${exchange}/part.step")
  set_tests_properties(cli.export_ipt PROPERTIES FIXTURES_REQUIRED cli_ipt_saved FIXTURES_SETUP cli_ipt_step)
  add_test(NAME cli.import_ipt_reference
    COMMAND mitcad-cli import-ipt "${exchange}/part.ipt" --reference "${exchange}/part.step" --deviation)
  set_tests_properties(cli.import_ipt_reference PROPERTIES FIXTURES_REQUIRED cli_ipt_step PASS_REGULAR_EXPRESSION
    "reference part\\.step: 2 STEP solids, largest relative difference [0-9.e+-]+ \\(limit 1e-6\\), surface deviation [0-9.e+-]+ mm: pass")
  # A part with a design (mitcad#60, stages 2 and 3): the parameter, the
  # sketch and the extrusion replayed parametric, making the stored cube.
  add_test(NAME cli.ipt_design_file COMMAND test_exchange --write-ipt-design "${exchange}/designed.ipt")
  set_tests_properties(cli.ipt_design_file PROPERTIES FIXTURES_SETUP cli_ipt_design)
  # The replay's hang watchdog (--hang-limit): a try that hangs comparing
  # the final bodies is given up, and the next one compares by volume.
  add_test(NAME cli.import_ipt_hang_exit
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DFILE=${exchange}/designed.ipt"
            -DMODE=import-ipt -P "${CMAKE_CURRENT_SOURCE_DIR}/hang-exit-test.cmake")
  set_tests_properties(cli.import_ipt_hang_exit PROPERTIES FIXTURES_REQUIRED cli_ipt_design)
  add_test(NAME cli.import_ipt_design COMMAND mitcad-cli import-ipt "${exchange}/designed.ipt")
  set_tests_properties(cli.import_ipt_design PROPERTIES FIXTURES_REQUIRED cli_ipt_design PASS_REGULAR_EXPRESSION
    "Imported 1 bodies of designed\\.ipt.*parameters: 4 model, 1 user.*expressions: 5 translated, 5 agree.*2 timeline items: 2 parametric, 0 partial, 0 fallback, 0 skipped.*F1 Sketch1 \\(sketch\\): ok.*F2 Extrusion1 \\(extrude\\): ok")
  # Design files of other formats as components (mitcad#1894): both test
  # parts imported and saved, then inserted as copies into one design, the
  # app's Import into this design; one undo step takes both out.
  add_test(NAME cli.import_ipt_design_saved
    COMMAND mitcad-cli import-ipt "${exchange}/designed.ipt" --save "${exchange}/designed.mitcad")
  set_tests_properties(cli.import_ipt_design_saved PROPERTIES FIXTURES_REQUIRED cli_ipt_design
    FIXTURES_SETUP cli_ipt_design_saved)
  add_test(NAME cli.import_into_design COMMAND mitcad-cli run "${scripts}/import_into_design.json")
  set_tests_properties(cli.import_into_design PROPERTIES WORKING_DIRECTORY "${exchange}"
    FIXTURES_REQUIRED "cli_ipt_saved;cli_ipt_design_saved")
  # Dropped on a face while a component that is not the root is active: the
  # placement in that component's coordinates lands at the drop point.
  add_test(NAME cli.import_into_design_cube
    COMMAND mitcad-cli run "${PROJECT_SOURCE_DIR}/tools/ui-scripts/import-into-design.json"
            --save "${exchange}/cube.mitcad")
  set_tests_properties(cli.import_into_design_cube PROPERTIES FIXTURES_SETUP cli_import_into_design_cube)
  add_test(NAME cli.import_into_design_drop COMMAND mitcad-cli run "${scripts}/import_into_design_drop.json")
  set_tests_properties(cli.import_into_design_drop PROPERTIES WORKING_DIRECTORY "${exchange}"
    FIXTURES_REQUIRED cli_import_into_design_cube)
  add_test(NAME ipt.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/ipt-corpus" -P "${CMAKE_CURRENT_SOURCE_DIR}/ipt-corpus.cmake")
  set_tests_properties(ipt.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "ipt corpus: skipped")
  # The .iam import (mitcad#60, stage 4): a test project of assemblies saved
  # on another machine (mitcad_ipt::testassembly): parts found relative to
  # the assembly and by name, one missing, a sub-assembly, hidden, grounded
  # and suppressed occurrences; every placement checked against the one
  # the file displays and every placed part against its stored range box. Real assemblies under
  # MITCAD_IAM_CORPUS (else MITCAD_IPT_CORPUS): iam-corpus.cmake.
  add_test(NAME cli.iam_files COMMAND test_exchange --write-iam "${exchange}/iam")
  set_tests_properties(cli.iam_files PROPERTIES FIXTURES_SETUP cli_iam)
  add_test(NAME cli.import_iam
    COMMAND mitcad-cli import-iam "${exchange}/iam/top.iam" --save "${exchange}/iam/top.mitcad")
  set_tests_properties(cli.import_iam PROPERTIES FIXTURES_REQUIRED cli_iam FIXTURES_SETUP cli_iam_saved
    PASS_REGULAR_EXPRESSION
    "Imported assembly top\\.iam: 7 occurrences placed \\(1 suppressed, 1 hidden, 1 of missing files, 0 not placed\\). 4 part files \\(3 imported, 1 missing, 0 failed. 3 identities confirmed, 0 differ\\), 1 sub-assemblies. units: mm.*files found: 1 by name, 1 missing, 3 relative to the assembly.*check: 7 of 7 placements agree with the displayed ones.*check: 5 of 5 parts. bodies lie within their stored range boxes")
  add_test(NAME cli.info_iam COMMAND mitcad-cli info "${exchange}/iam/top.mitcad")
  set_tests_properties(cli.info_iam PROPERTIES FIXTURES_REQUIRED cli_iam_saved
    PASS_REGULAR_EXPRESSION "F1 Base1 \\(base\\): ok.*Bodies: 5")
  # With --history, each part's design replayed: the cube's sketch and
  # extrusion.
  add_test(NAME cli.import_iam_history COMMAND mitcad-cli import-iam "${exchange}/iam/top.iam" --history)
  set_tests_properties(cli.import_iam_history PROPERTIES FIXTURES_REQUIRED cli_iam PASS_REGULAR_EXPRESSION
    "part [^\n]*cube\\.ipt [^\n]*imported: 1 bodies \\(1 valid, 0 not built\\). features 2 parametric")
  # The parts' outcomes with --history (mitcad#507): the three part files
  # counted before the first is imported and traced (part n of 3), the cube
  # replayed, the pair (twice) as its stored bodies, having no design; and
  # with the cube's replay made to fail, the cube as its stored bodies too.
  add_test(NAME cli.import_iam_history_parts COMMAND mitcad-cli import-iam "${exchange}/iam/top.iam" --history)
  set_tests_properties(cli.import_iam_history_parts PROPERTIES FIXTURES_REQUIRED cli_iam
    ENVIRONMENT "MITCAD_IMPORT_TRACE=1" PASS_REGULAR_EXPRESSION
    "part [123] of 3: cube\\.ipt: replayed in [0-9.]+ s.*parts with their design history: 1 replayed, 2 as their stored bodies.*cube\\.ipt [^\n]*features 2 parametric. replayed: 2 items, [0-9]+ verified")
  add_test(NAME cli.import_iam_history_fallback COMMAND mitcad-cli import-iam "${exchange}/iam/top.iam" --history)
  set_tests_properties(cli.import_iam_history_fallback PROPERTIES FIXTURES_REQUIRED cli_iam
    ENVIRONMENT "MITCAD_IMPORT_TRACE=1;MITCAD_TEST_IAM_REPLAY_FAIL=cube.ipt" PASS_REGULAR_EXPRESSION
    "part [123] of 3: cube\\.ipt: bodies in [0-9.]+ s.*parts with their design history: 0 replayed, 3 as their stored bodies.*cube\\.ipt [^\n]*imported: 1 bodies \\(1 valid, 0 not built\\). bodies only: the replay failed: MITCAD_TEST_IAM_REPLAY_FAIL names it")
  add_test(NAME cli.import_ipt_iam COMMAND mitcad-cli import-ipt "${exchange}/iam/sub/sub.iam")
  set_tests_properties(cli.import_ipt_iam PROPERTIES FIXTURES_REQUIRED cli_iam PASS_REGULAR_EXPRESSION
    "Imported assembly sub\\.iam: 2 occurrences placed.*1 part files \\(1 imported, 0 missing.*check: 2 of 2 placements agree.*check: 2 of 2 parts. bodies lie within their stored range boxes")
  # Placements that mirror or scale (mitcad#1903): the cube mirrored and
  # scaled, the sub-assembly mirrored, each a copy of the bodies where the
  # file has them, with a warning; the placed bodies' boxes from the saved
  # design.
  add_test(NAME cli.import_iam_mirrored
    COMMAND mitcad-cli import-iam "${exchange}/iam/mirrored.iam" --save "${exchange}/iam/mirrored.mitcad")
  set_tests_properties(cli.import_iam_mirrored PROPERTIES FIXTURES_REQUIRED cli_iam FIXTURES_SETUP cli_iam_mirrored
    PASS_REGULAR_EXPRESSION
    "Imported assembly mirrored\\.iam: 6 occurrences placed \\(0 suppressed, 0 hidden, 0 of missing files, 0 not placed\\).*check: 5 of 5 placements agree with the displayed ones.*check: 5 of 5 parts. bodies lie within their stored range boxes.*warning: cube:2: the placement mirrors or scales it. a copy is placed where the file has it.*warning: cube:3: the placement mirrors or scales it.*warning: sub:1: the placement mirrors or scales it")
  add_test(NAME cli.info_iam_mirrored COMMAND mitcad-cli info "${exchange}/iam/mirrored.mitcad")
  set_tests_properties(cli.info_iam_mirrored PROPERTIES FIXTURES_REQUIRED cli_iam_mirrored
    PASS_REGULAR_EXPRESSION "cube \\(1\\):1/[^\n]*\n *bounding box \\[40\\.000, 0\\.000, 0\\.000\\] - \\[50\\.000, 10\\.000, 10\\.000\\].*cube \\(2\\):1/[^\n]*volume 8000\\.000 mm3[^\n]*\n *bounding box \\[0\\.000, 200\\.000, 0\\.000\\] - \\[20\\.000, 220\\.000, 20\\.000\\].*sub:1/pair:1/[^\n]*\n *bounding box \\[0\\.000, -110\\.000, 0\\.000\\] - \\[10\\.000, -100\\.000, 10\\.000\\]")
  # An assembly converted from a building model whose parts got no bodies
  # (mitcad#1913): each part without geometry says so, the one its
  # translation report lists with what the report says of it, and the part
  # whose result lists a body its B-rep record lacks as bodies not found.
  add_test(NAME cli.import_iam_converted COMMAND mitcad-cli import-iam "${exchange}/iam/converted.iam")
  set_tests_properties(cli.import_iam_converted PROPERTIES FIXTURES_REQUIRED cli_iam
    PASS_REGULAR_EXPRESSION
    "Imported assembly converted\\.iam: 5 occurrences placed \\(0 suppressed, 0 hidden, 0 of missing files, 0 not placed\\). 4 part files \\(4 imported, 0 missing, 0 failed.*\n  parts without geometry: 2 \\(1 as a translation report says\\). parts that list bodies not found: 1\n.*part [^\n]*cube\\.ipt [^\n]*imported: 1 bodies \\(1 valid, 0 not built\\)\n.*part [^\n]*empty\\.ipt [^\n]*2 occurrences[^\n]*imported: 0 bodies \\(0 valid, 0 not built\\). no geometry: the translation report says: No Bodies Read From File\\.\n.*part [^\n]*bare\\.ipt [^\n]*imported: 0 bodies \\(0 valid, 0 not built\\). no geometry in the file\n.*part [^\n]*listed\\.ipt [^\n]*imported: 0 bodies \\(0 valid, 0 not built\\). the bodies it lists were not found\n.*warning: parts without geometry: 1: the translation report the assembly keeps, of its conversion from a \\.rvt file, says of each: No Bodies Read From File\\.\n.*warning: parts without geometry: 1: their files hold no bodies[^\n]*\n.*warning: parts that list bodies the import did not find in them: 1")
  # The part alone: its listed body not found, said in words.
  add_test(NAME cli.import_ipt_listed COMMAND mitcad-cli import-ipt "${exchange}/iam/listed.ipt")
  set_tests_properties(cli.import_ipt_listed PROPERTIES FIXTURES_REQUIRED cli_iam PASS_REGULAR_EXPRESSION
    "Imported 0 bodies of listed\\.ipt.*warning: the part lists 1 bodies that neither its B-rep record nor a mesh feature holds: they were not imported")
  add_test(NAME iam.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/iam-corpus" -P "${CMAKE_CURRENT_SOURCE_DIR}/iam-corpus.cmake")
  set_tests_properties(iam.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "iam corpus: skipped")
  # Reorient Design on the test files' bench (c71542a1) and drawer
  # (1dc60c2a) assemblies, found by content id under MITCAD_IAM_CORPUS (else MITCAD_IPT_CORPUS)
  # (mitcad#537): every offered turn taken, the drawer also turned twice.
  add_test(NAME iam.corpus_reorient
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/iam-corpus-reorient" -P "${CMAKE_CURRENT_SOURCE_DIR}/reorient-corpus.cmake")
  set_tests_properties(iam.corpus_reorient PROPERTIES SKIP_REGULAR_EXPRESSION "reorient corpus: skipped" TIMEOUT 3000)

  # The IFC import (mitcad#273): the reader's test building (written by
  # mitcad-cli at the test, never kept), every product accounted for: the
  # storeys as components, the wall's extrusion with its opening cut, the
  # window type's two occurrences, the clipped column, the faceted cube and
  # the tetrahedron mesh, with their volumes. Real files under
  # MITCAD_IFC_CORPUS: ifc-corpus.cmake.
  set(ifc "${CMAKE_CURRENT_BINARY_DIR}/ifc")
  file(MAKE_DIRECTORY "${ifc}")
  add_test(NAME cli.ifc_file COMMAND mitcad-cli import-ifc --write-test "${ifc}/building.ifc")
  set_tests_properties(cli.ifc_file PROPERTIES FIXTURES_SETUP cli_ifc)
  add_test(NAME cli.import_ifc COMMAND mitcad-cli import-ifc "${ifc}/building.ifc" --save "${ifc}/building.mitcad")
  set_tests_properties(cli.import_ifc PROPERTIES FIXTURES_REQUIRED cli_ifc FIXTURES_SETUP cli_ifc_saved
    PASS_REGULAR_EXPRESSION
    "Imported building\\.ifc \\(IFC4 IFC4, view ReferenceView_V1\\.2, from Mitcad test\\): 16 products in 4 components.*outcomes: 1 axes, 4 component, 1 cut, 1 mesh, 2 not_imported, 2 occurrence, 4 parametric, 1 stored.*check: 1 of 1 element volumes agree.*#100 IFCWALL 'Wall A' 1WALL00000000000000000: parametric \\(1 bodies, 4 features, volume 2800000000\\.0 mm3\\).*#400 IFCCOLUMN 'Column' [^\n]*: parametric \\(1 bodies, 4 features, volume 400000000\\.0 mm3\\).*#500 [^\n]*: stored \\(1 bodies, 1 features, volume 1000000000\\.0 mm3\\).*#600 [^\n]*: mesh \\(1 bodies, 1 features, volume 166666666\\.7 mm3\\).*map #140 'Window type': 2 occurrences")
  add_test(NAME cli.info_ifc COMMAND mitcad-cli info "${ifc}/building.mitcad")
  set_tests_properties(cli.info_ifc PROPERTIES FIXTURES_REQUIRED cli_ifc_saved
    PASS_REGULAR_EXPRESSION "Wall A 1WALL00000000000000000")
  add_test(NAME ifc.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/ifc-corpus"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/ifc-corpus.cmake")
  set_tests_properties(ifc.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "ifc corpus: skipped")
  # The .rvt import (mitcad#274): the reader's test project (two levels, a
  # column, a wall, a surface's saved mesh) one component per level, and
  # its test family's metadata. The corpus's MIT-licensed project (by its
  # content id under MITCAD_RVT_CORPUS): every building element on its
  # level; skipped without the corpus.
  add_test(NAME cli.rvt_files COMMAND test_exchange --write-rvt "${exchange}/project.rvt")
  add_test(NAME cli.rfa_file COMMAND test_exchange --write-rfa "${exchange}/family.rfa")
  set_tests_properties(cli.rvt_files cli.rfa_file PROPERTIES FIXTURES_SETUP cli_rvt)
  add_test(NAME cli.import_rvt
    COMMAND mitcad-cli import-rvt "${exchange}/project.rvt" --save "${exchange}/project.mitcad")
  set_tests_properties(cli.import_rvt PROPERTIES FIXTURES_REQUIRED cli_rvt PASS_REGULAR_EXPRESSION
    "project\\.rvt \\(rvt\\), saved by release 2024.*Imported 3 bodies: 1 saved meshes, 1 boxes at a family type's transform, 1 boxes.*Ground: 2 bodies.*Upper: 1 bodies")
  add_test(NAME cli.import_rfa COMMAND mitcad-cli import-rvt "${exchange}/family.rfa")
  set_tests_properties(cli.import_rfa PROPERTIES FIXTURES_REQUIRED cli_rvt PASS_REGULAR_EXPRESSION
    "family\\.rfa \\(rfa\\), saved by release 2020.*family: Test Family & Co.*a family file: its metadata only")
  # The building test project (mitcad#275): its walls and floor rebuilt as
  # features checked against the geometry the file keeps of them (else
  # their stored boxes), its levels' planes, its grid's axis, its
  # material's appearance and its link found next to it.
  add_test(NAME cli.rvt_building_files COMMAND test_exchange --write-rvt-building "${exchange}/rvt-building")
  set_tests_properties(cli.rvt_building_files PROPERTIES FIXTURES_SETUP cli_rvt_building)
  add_test(NAME cli.import_rvt_building
    COMMAND mitcad-cli import-rvt "${exchange}/rvt-building/building.rvt" --save "${exchange}/rvt-building/building.mitcad")
  set_tests_properties(cli.import_rvt_building PROPERTIES FIXTURES_REQUIRED cli_rvt_building PASS_REGULAR_EXPRESSION
    "Built 6 elements as features \\(2 floor, 4 wall\\), checked against the geometry the file keeps of them \\(else their stored boxes\\). of those kinds none came in as meshes or boxes.*4 level planes, 2 grid axes, 0 grid arcs, 2 appearances.*link linked\\.rvt: linked\\.rvt, 4 elements")
  # The corpus's project and its link check run in child processes at once
  # (mitcad_run_parallel, mitcad#875), as do the .dwg corpus's drawings.
  add_test(NAME rvt.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/rvt-corpus"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/rvt-corpus.cmake")
  set_tests_properties(rvt.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "rvt corpus: skipped")
  # The .dwg import (mitcad#453): the reader's test drawing (a block of a
  # cube inserted in another block with a cylinder, both inserted twice and
  # as an array, an insert scaled by 2, 3D faces, a polyface and a
  # subdivision mesh, a line) in each container layout: two components
  # with their occurrences, six bodies, the line left out. The trial
  # models under MITCAD_DWG_CORPUS (dwg-corpus.cmake): skipped without it.
  set(dwg "${CMAKE_CURRENT_BINARY_DIR}/dwg")
  file(MAKE_DIRECTORY "${dwg}")
  foreach(version R14 R2000 R2004 R2007 R2010 R2013 R2018)
    add_test(NAME cli.dwg_file_${version} COMMAND mitcad-cli import-dwg --write-test "${dwg}/test-${version}.dwg" --version ${version})
    set_tests_properties(cli.dwg_file_${version} PROPERTIES FIXTURES_SETUP cli_dwg_${version})
    add_test(NAME cli.import_dwg_${version}
      COMMAND mitcad-cli import-dwg "${dwg}/test-${version}.dwg" --save "${dwg}/test-${version}.mitcad" --check
              --expect "length_mm=82.732,chord_distance_mm=44,tube_diameter_mm=2,mass_kg=0.0306,density_kg_m3=1000000,solids=3,components=2")
    set_tests_properties(cli.import_dwg_${version} PROPERTIES FIXTURES_REQUIRED cli_dwg_${version} PASS_REGULAR_EXPRESSION
      "Imported test-${version}\\.dwg \\(dwg ${version} \\(AC[0-9]+\\)\\): 6 bodies \\(3 solids, 3 meshes. 6 valid\\), 2 components.*Component Segment: 1 bodies, 2 occurrences \\(2 placements\\).*Component Brace: 1 bodies, 8 occurrences \\(10 placements\\).*Left out: 1 LINE.*Checks: 11 of 11 pass")
  endforeach()
  # A solid none of whose faces can be built (its point records hold no
  # positions, so every loop is lost) is left out as not built, and the
  # drawing's other bodies come in (mitcad#589: such a body ended the whole
  # import, or came in as a solid of infinite planes).
  add_test(NAME cli.dwg_file_empty_solid
    COMMAND mitcad-cli import-dwg --write-test "${dwg}/empty-solid.dwg" --case empty-solid)
  set_tests_properties(cli.dwg_file_empty_solid PROPERTIES FIXTURES_SETUP cli_dwg_empty_solid)
  add_test(NAME cli.import_dwg_empty_solid COMMAND mitcad-cli import-dwg "${dwg}/empty-solid.dwg")
  set_tests_properties(cli.import_dwg_empty_solid PROPERTIES FIXTURES_REQUIRED cli_dwg_empty_solid PASS_REGULAR_EXPRESSION
    "Imported empty-solid\\.dwg \\(dwg R2018 \\(AC1032\\)\\): 6 bodies \\(3 solids, 3 meshes. 6 valid\\), 2 components.*Not built: 3DSOLID 64: its B-rep record holds no body with faces.*Check every body built: FAIL")
  add_test(NAME dwg.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/dwg-corpus"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/dwg-corpus.cmake")
  set_tests_properties(dwg.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "dwg corpus: skipped")

  # The .3dm import (mitcad#1715): the reader's sample model (a box B-rep,
  # a cylinder B-rep on a surface of revolution, an extruded plate with a
  # hole and a round extrusion, a mesh, a curve, a block placed twice), in
  # centimetres: five bodies of the model, the block a component with its
  # body and two occurrences, the curve left out by design. A block that
  # holds an instance of itself becomes a component once, that instance
  # left out (not placed without end). The models listed under
  # MITCAD_3DM_CORPUS (3dm-corpus.cmake): skipped without it.
  set(dir_3dm "${CMAKE_CURRENT_BINARY_DIR}/3dm")
  file(MAKE_DIRECTORY "${dir_3dm}")
  add_test(NAME cli.3dm_file COMMAND mitcad-cli import-3dm --write-test "${dir_3dm}/sample.3dm")
  set_tests_properties(cli.3dm_file PROPERTIES FIXTURES_SETUP cli_3dm)
  add_test(NAME cli.import_3dm
    COMMAND mitcad-cli import-3dm "${dir_3dm}/sample.3dm" --save "${dir_3dm}/sample.mitcad" --check)
  set_tests_properties(cli.import_3dm PROPERTIES FIXTURES_REQUIRED cli_3dm PASS_REGULAR_EXPRESSION
    "Imported sample\\.3dm \\(\\.3dm version 70, MitCad test file\\): 6 bodies \\(5 solids, 1 meshes. 6 valid\\), 1 components.*Units: centimeters \\(10\\.0 mm per unit\\).*Component Bolt: 1 bodies, 2 occurrences.*Left out: 1 curve.*Checks: 3 of 3 pass")
  add_test(NAME cli.3dm_file_block_in_itself
    COMMAND mitcad-cli import-3dm --write-test "${dir_3dm}/block-in-itself.3dm" --case block-in-itself)
  set_tests_properties(cli.3dm_file_block_in_itself PROPERTIES FIXTURES_SETUP cli_3dm_block_in_itself)
  add_test(NAME cli.import_3dm_block_in_itself COMMAND mitcad-cli import-3dm "${dir_3dm}/block-in-itself.3dm")
  set_tests_properties(cli.import_3dm_block_in_itself PROPERTIES FIXTURES_REQUIRED cli_3dm_block_in_itself
    PASS_REGULAR_EXPRESSION
    "Imported block-in-itself\\.3dm \\(\\.3dm version 70, MitCad test file\\): 1 bodies \\(1 solids, 0 meshes. 1 valid\\), 1 components.*Component Loop: 1 bodies, 1 occurrences.*Left out: 1 block instance \\(a block placed inside itself\\).*Checks: 2 of 3 pass")
  # A SubD object (mitcad#1757): the test writer's cube net of archive
  # version 80 (a creased edge, a semi-sharp edge, a corner, a vertex on no
  # face) comes in as an editable Form feature named after its object, its
  # cage closed with both creases and the corner, built as a solid by the
  # kernel; the vertex left out and the averaged sharpness are warnings.
  add_test(NAME cli.3dm_file_subd
    COMMAND mitcad-cli import-3dm --write-test "${dir_3dm}/subd.3dm" --case subd)
  set_tests_properties(cli.3dm_file_subd PROPERTIES FIXTURES_SETUP cli_3dm_subd)
  add_test(NAME cli.import_3dm_subd
    COMMAND mitcad-cli import-3dm "${dir_3dm}/subd.3dm" --save "${dir_3dm}/subd.mitcad" --check)
  set_tests_properties(cli.import_3dm_subd PROPERTIES FIXTURES_REQUIRED cli_3dm_subd
    FIXTURES_SETUP cli_3dm_subd_saved PASS_REGULAR_EXPRESSION
    "Imported subd\\.3dm \\(\\.3dm version 80, MitCad test file\\): 0 bodies.*Form blob: 8 vertices, 6 faces, 2 creased edges, 1 corners, closed.*Objects taken: SubD 1.*Warning: the SubD \"blob\" on the layer Solids: a vertex of the SubD on no face was left out.*Checks: 3 of 3 pass")
  # The imported Form is an editable Form: its cage as the file's, a vertex
  # moved with form.move, the body rebuilt.
  add_test(NAME cli.import_3dm_subd_edit
    COMMAND mitcad-cli run "${scripts}/import_3dm_subd_form.json" --open "${dir_3dm}/subd.mitcad")
  set_tests_properties(cli.import_3dm_subd_edit PROPERTIES FIXTURES_REQUIRED cli_3dm_subd_saved)
  # Blocks of the review mitcad#1838: a block member on a switched-off
  # layer is left out (once, though the block is placed twice), so the
  # file's two B-reps are one taken and one left out; a block placed once
  # at scale 2 is copied in place, its 1000 mm3 box 8000 mm3 in the report.
  add_test(NAME cli.3dm_file_block_hidden_layer
    COMMAND mitcad-cli import-3dm --write-test "${dir_3dm}/block-hidden-layer.3dm" --case block-hidden-layer)
  set_tests_properties(cli.3dm_file_block_hidden_layer PROPERTIES FIXTURES_SETUP cli_3dm_block_hidden_layer)
  add_test(NAME cli.import_3dm_block_hidden_layer
    COMMAND mitcad-cli import-3dm "${dir_3dm}/block-hidden-layer.3dm" --check)
  set_tests_properties(cli.import_3dm_block_hidden_layer PROPERTIES FIXTURES_REQUIRED cli_3dm_block_hidden_layer
    PASS_REGULAR_EXPRESSION
    "Imported block-hidden-layer\\.3dm \\(\\.3dm version 70, MitCad test file\\): 1 bodies \\(1 solids, 0 meshes. 1 valid\\), 1 components.*Component Bracket: 1 bodies, 2 occurrences.*Objects taken: B-rep 1, block instance 2\n.*Left out: 1 B-rep \\(hidden in the file\\).*Volume of the solids: 1000\\.0 mm3.*Checks: 3 of 3 pass")
  add_test(NAME cli.3dm_file_block_scaled
    COMMAND mitcad-cli import-3dm --write-test "${dir_3dm}/block-scaled.3dm" --case block-scaled)
  set_tests_properties(cli.3dm_file_block_scaled PROPERTIES FIXTURES_SETUP cli_3dm_block_scaled)
  add_test(NAME cli.import_3dm_block_scaled
    COMMAND mitcad-cli import-3dm "${dir_3dm}/block-scaled.3dm" --check)
  set_tests_properties(cli.import_3dm_block_scaled PROPERTIES FIXTURES_REQUIRED cli_3dm_block_scaled
    PASS_REGULAR_EXPRESSION
    "Imported block-scaled\\.3dm \\(\\.3dm version 70, MitCad test file\\): 1 bodies \\(1 solids, 0 meshes. 1 valid\\), 0 components.*Objects taken: B-rep 1, block instance 1\n.*Volume of the solids: 8000\\.0 mm3.*Checks: 3 of 3 pass")
  add_test(NAME 3dm.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/3dm-corpus"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/3dm-corpus.cmake")
  set_tests_properties(3dm.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "3dm corpus: skipped")
  # The public-domain glTF sample (mitcad#1826, core/gltf/samples, CC0)
  # through OCCT: a stand of two mesh bodies as a component turned Z up and
  # a marker of the design, metres as millimetres.
  add_test(NAME cli.import_gltf COMMAND mitcad-cli run "${scripts}/gltf_sample.json")
  set_tests_properties(cli.import_gltf PROPERTIES WORKING_DIRECTORY "${PROJECT_SOURCE_DIR}/core/gltf/samples")
  # Real, openly licensed glTF files listed under MITCAD_GLTF_CORPUS
  # (gltf-corpus.cmake), each checked by its script of facts read from the
  # file: skipped without it.
  add_test(NAME gltf.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/gltf-corpus"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/gltf-corpus.cmake")
  set_tests_properties(gltf.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "gltf corpus: skipped")
  # The .serp import (mitcad#1870) through OCCT: the test file of
  # serp-test-file (B-reps OCCT wrote in its binary format: a box, a
  # B-spline surface, a block placed twice; a mesh, a hidden object, a
  # text and a point cloud, layout and history record left out; in
  # centimetres) imported with import_file and checked by
  # tests/serp_sample.json; the same file as a newer format version is
  # refused in words. Real files listed under MITCAD_SERP_CORPUS
  # (serp-corpus.cmake): skipped without it.
  set(dir_serp "${CMAKE_CURRENT_BINARY_DIR}/serp")
  file(MAKE_DIRECTORY "${dir_serp}")
  add_test(NAME cli.serp_file COMMAND mitcad-cli serp-test-file "${dir_serp}/sample.serp")
  set_tests_properties(cli.serp_file PROPERTIES FIXTURES_SETUP cli_serp)
  add_test(NAME cli.import_serp COMMAND mitcad-cli run "${scripts}/serp_sample.json")
  set_tests_properties(cli.import_serp PROPERTIES FIXTURES_REQUIRED cli_serp WORKING_DIRECTORY "${dir_serp}")
  add_test(NAME cli.serp_file_newer
    COMMAND mitcad-cli serp-test-file "${dir_serp}/newer.serp" --case newer)
  set_tests_properties(cli.serp_file_newer PROPERTIES FIXTURES_SETUP cli_serp_newer)
  add_test(NAME cli.import_serp_newer COMMAND mitcad-cli import "${dir_serp}/newer.serp")
  set_tests_properties(cli.import_serp_newer PROPERTIES FIXTURES_REQUIRED cli_serp_newer
    PASS_REGULAR_EXPRESSION "newer\\.serp: the file was made by a newer version of its program")
  add_test(NAME serp.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/serp-corpus"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/serp-corpus.cmake")
  set_tests_properties(serp.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "serp corpus: skipped")
  # Plane surfaces in the ground's plane (mitcad#1738): the test writer's
  # three plane surfaces drawn facing up at z 0, each in another of the
  # file's forms, come in facing up and render lit from above, not black
  # (the ground a hair under the lowest bodies). Needs Python (its PNG
  # reader is tools/libraries'); the render is skipped without a worker.
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.render_3dm_plane_surfaces
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/render-3dm-sheets-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>" --worker "$<TARGET_FILE:mitcad-render>"
              --libraries "${PROJECT_SOURCE_DIR}/tools/libraries"
              --work "${CMAKE_CURRENT_BINARY_DIR}/render-3dm-plane-surfaces")
    set_tests_properties(cli.render_3dm_plane_surfaces PROPERTIES TIMEOUT 120
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1" SKIP_REGULAR_EXPRESSION "3dm plane surfaces: skipped")
  endif()

  # FreeCAD .FCStd import: a small document made by the test (B-rep shapes
  # from mitcad-geomtool), imported against a reference, saved and opened.
  add_test(NAME cli.import_fcstd
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DGEOMTOOL=$<TARGET_FILE:mitcad-geomtool>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fcstd" -P "${CMAKE_CURRENT_SOURCE_DIR}/fcstd-test.cmake")
  # FreeCAD documents under MITCAD_FCSTD_CORPUS (the reference models of
  # tools/freecad-export with their dumps, FreeCAD's examples): every file
  # imports, and those with a dump match it; skipped without the corpus.
  # The files are checked several at once (mitcad_run_parallel, mitcad#70).
  add_test(NAME freecad.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fcstd-corpus" -P "${CMAKE_CURRENT_SOURCE_DIR}/fcstd-corpus.cmake")
  set_tests_properties(freecad.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "FreeCAD corpus: skipped")
  # Single FreeCAD documents, found by their content ids under the folders
  # of MITCAD_FCSTD_CORPUS and the folders beside them (reported ones):
  # no fallback and the Body's volume as FreeCAD's within 1e-6 relative
  # (mitcad#997); skipped without them.
  add_test(NAME freecad.corpus_designs
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fcstd-designs" -P "${CMAKE_CURRENT_SOURCE_DIR}/fcstd-designs.cmake")
  set_tests_properties(freecad.corpus_designs PROPERTIES SKIP_REGULAR_EXPRESSION "FreeCAD designs: skipped")
  # FreeCAD documents reported with import faults, found by their content
  # ids under MITCAD_FCSTD_CORPUS and its `reported` folders: the objects
  # each fault was about come in as the report expects (mitcad#998);
  # skipped without the files.
  add_test(NAME freecad.reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fcstd-reported" -P "${CMAKE_CURRENT_SOURCE_DIR}/fcstd-reported.cmake")
  set_tests_properties(freecad.reported PROPERTIES SKIP_REGULAR_EXPRESSION "reported FreeCAD files: skipped")

  # Designs saved by earlier versions under MITCAD_DESIGN_CORPUS (project
  # folders): every one opens with no sketch failing (mitcad#249); skipped
  # without the corpus.
  add_test(NAME designs.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" -P "${CMAKE_CURRENT_SOURCE_DIR}/designs-corpus.cmake")
  set_tests_properties(designs.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "design corpus: skipped")

  # .solidcraft designs with their history (mitcad#450): the test designs
  # written by hand for the import, checked against the values worked out
  # for them; and the designs under MITCAD_SOLIDCRAFT_CORPUS, checked against
  # their expected values where they have them (skipped without the corpus).
  add_test(NAME cli.import_solidcraft
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DDESIGNS=${CMAKE_SOURCE_DIR}/core/solidcraft/tests/designs"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/solidcraft" -P "${CMAKE_CURRENT_SOURCE_DIR}/solidcraft-test.cmake")
  add_test(NAME solidcraft.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/solidcraft-corpus"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/solidcraft-corpus.cmake")
  set_tests_properties(solidcraft.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "SolidCraft corpus: skipped")

  # Single .f3d designs checked item by item (mitcad#311), found by their
  # content ids under MITCAD_F3D_CORPUS and MITCAD_F3D_BACKUP and the
  # `-reported` folders beside them (mitcad#1175): the items
  # named come in parametric, none after a retry on the file's exact state,
  # and reading the file and applying its appearances take less time than
  # the replay, and a design with a time limit (seconds in a release build,
  # three times that in a debug build) opens within it (mitcad#642);
  # skipped without the designs. The designs run at once, each in a child
  # process (mitcad#832); the imports try an item from its own start, the
  # first route, SLOWER times as long too (--own-start-time-scale,
  # mitcad#847), so that a debug build's item started beside the rest of
  # the corpus step does not run out of that time (a debug build's path
  # pattern of b832916a takes 18 s by itself, the route 30 s).
  if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(f3d_designs_slower 3)
  else()
    set(f3d_designs_slower 1)
  endif()
  # The designs at once again (RUNNER, mitcad#874): in 0.32.0-rc2 a thread's
  # sizing offset ran past its 2 s on the wall clock beside the others, the
  # thread took the tube's route and a rounding along it (db73863f's
  # Fillet4) ran out of its item time. The import's time limits are on a
  # clock that leaves the load out now, and a definition cut short at its
  # share goes on on its worker for the second round.
  add_test(NAME f3d.corpus_designs
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/f3d-designs"
            "-DSLOWER=${f3d_designs_slower}" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/f3d-designs.cmake")
  set_tests_properties(f3d.corpus_designs PROPERTIES SKIP_REGULAR_EXPRESSION "f3d designs: skipped")

  # .f3d designs driven by their user parameters (mitcad#641), found by
  # their content ids likewise: imported, their parameters changed and
  # recomputed, every feature without an error and each body's bounding box
  # as the parameters derive it (or the reference export of the change);
  # skipped without the designs.
  add_test(NAME f3d.corpus_parameters
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/f3d-parameters"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/f3d-parameters.cmake")
  set_tests_properties(f3d.corpus_parameters PROPERTIES SKIP_REGULAR_EXPRESSION "f3d parameters: skipped")

  # The reference sheet-metal hems of every form (mitcad#1378), found by
  # their content ids in MITCAD_F3D_MODELS: each hem parametric and
  # verified against the file's history; skipped without the designs.
  add_test(NAME f3d.reference_hems
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/f3d-hems-test.cmake")
  set_tests_properties(f3d.reference_hems PROPERTIES SKIP_REGULAR_EXPRESSION "reference hems: skipped")

  # An inserted design moved by a joint on a part inside it (mitcad#1892),
  # found by its content id in MITCAD_F3D_BACKUP with its reference folder:
  # the inserted design lands where the file places it, not where the path
  # to one of its parts does; skipped without the design.
  add_test(NAME f3d.placed_inside
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/f3d-placed-inside"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/f3d-placed-inside-test.cmake")
  set_tests_properties(f3d.placed_inside PROPERTIES SKIP_REGULAR_EXPRESSION "f3d placed inside: skipped")

  # Recompute speed (P10): a generated model with the joins, patterns and
  # fillets that slowed down imported designs, saved, then opened with the
  # time of each feature reported (ctest -V, or Testing/Temporary/
  # LastTest.log). The time limit is about ten times the usual time (0.5 s
  # in a release build, 3.3 s in a Linux debug build), so only a large
  # slowdown fails: before T1d's patterns the model took 6.6 s in release.
  set(perf "${CMAKE_CURRENT_BINARY_DIR}/perf")
  file(MAKE_DIRECTORY "${perf}")
  add_test(NAME cli.perf_model COMMAND mitcad-cli run "${scripts}/perf_model.json" --save "${perf}/perf_model.mitcad")
  set_tests_properties(cli.perf_model PROPERTIES FIXTURES_SETUP cli_perf)
  if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(perf_limit 30)
    set(perf_join_limit 15)
    set(perf_holes_limit 10)
  else()
    set(perf_limit 5)
    set(perf_join_limit 5)
    set(perf_holes_limit 8)
  endif()
  add_test(NAME cli.perf_recompute COMMAND mitcad-cli info "${perf}/perf_model.mitcad" --timings)
  # The timed tests run alone (RUN_SERIAL), also under ctest -j.
  set_tests_properties(cli.perf_recompute PROPERTIES FIXTURES_REQUIRED cli_perf TIMEOUT ${perf_limit}
    RUN_SERIAL TRUE
    PASS_REGULAR_EXPRESSION "F12 Fillet2 \\(fillet\\): ok.*Recompute: 12 features in [0-9.]+ ms")
  # Joins near bodies they do not touch (T0d): a 90-copy pattern of pins on
  # a ring inside eight tubes, every pin within the tubes' boxes. The limit
  # is two to three times the usual time (5.9 s in a Linux debug build, 2 s
  # with the Windows build's RelWithDebInfo, 0.7 s in a Linux release
  # build); before T0d the model took 29.6 s in debug, 3.1 s in release.
  add_test(NAME cli.perf_join_model COMMAND mitcad-cli run "${scripts}/perf_join_pattern.json"
                                            --save "${perf}/perf_join.mitcad")
  set_tests_properties(cli.perf_join_model PROPERTIES FIXTURES_SETUP cli_perf_join)
  add_test(NAME cli.perf_join_recompute COMMAND mitcad-cli info "${perf}/perf_join.mitcad" --timings)
  set_tests_properties(cli.perf_join_recompute PROPERTIES FIXTURES_REQUIRED cli_perf_join
    TIMEOUT ${perf_join_limit} RUN_SERIAL TRUE
    PASS_REGULAR_EXPRESSION "F17 CircularPattern2 \\(circular_pattern\\): ok.*Recompute: 17 features in [0-9.]+ ms")

  # Perforated plates (mitcad#72): a 35 x 35 hole pattern, then cuts and a
  # join next to the holes. 2 s in a release build (6 s before mitcad#72),
  # 3 s in a Linux debug build; the limit is about three times the usual
  # time.
  add_test(NAME cli.perf_holes_model COMMAND mitcad-cli run "${scripts}/perf_holes.json"
                                             --save "${perf}/perf_holes.mitcad")
  set_tests_properties(cli.perf_holes_model PROPERTIES FIXTURES_SETUP cli_perf_holes)
  add_test(NAME cli.perf_holes_recompute COMMAND mitcad-cli info "${perf}/perf_holes.mitcad" --timings)
  set_tests_properties(cli.perf_holes_recompute PROPERTIES FIXTURES_REQUIRED cli_perf_holes
    TIMEOUT ${perf_holes_limit} RUN_SERIAL TRUE
    PASS_REGULAR_EXPRESSION "F11 Extrude5 \\(extrude\\): ok.*Recompute: 11 features in [0-9.]+ ms")
  # Their display meshing (mitcad#884): the body meshed twice by the view's
  # final tier (planes through Delabella, checked), every face triangulated
  # and no plane made again by the default insertion, which takes minutes
  # on the two planes of 1,187 holes. 5 s in a Linux release build with the
  # open (each meshing 1.3 s, 2.7 s before OCCT port patches 1080 and
  # 1081); the limit only catches a plane gone to the default insertion or
  # a large slowdown.
  if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(perf_holes_mesh_limit 120)
  else()
    set(perf_holes_mesh_limit 30)
  endif()
  add_test(NAME cli.perf_holes_mesh COMMAND mitcad-cli mesh-compare "${perf}/perf_holes.mitcad" --a checked
                                            --b checked)
  # No plane redone: the five square notch walls, whose corners are
  # cocircular, failed in Delabella where long double has 53 bits (MSVC,
  # ARM) until OCCT port patch 1082 (mitcad#1919).
  set_tests_properties(cli.perf_holes_mesh PROPERTIES FIXTURES_REQUIRED cli_perf_holes
    TIMEOUT ${perf_holes_mesh_limit} RUN_SERIAL TRUE
    PASS_REGULAR_EXPRESSION "1 bodies, 1257 faces \\(42 planes\\), without a triangulation a 0 b 0..*planes redone a 0 b 0 \\(a checked, b checked\\)")
  if(WIN32)
    set_property(TEST cli.perf_holes_mesh APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()

  # Sketches with many patterned circles (mitcad#670): one circle and a
  # 10 x 10 rectangular sketch pattern of it, a full solve and drag steps,
  # each step's time printed (run --timings; ctest -V). The _5 and _20
  # scripts are the same at 25 and 400 circles, for measurements, as is
  # tools/ui-sketch-pattern-speed-test.sh in the application. In mitcad#670
  # the script took about 1 s in a Linux release build and 1.5 s in a Linux
  # debug build; the limit only catches a large slowdown.
  add_test(NAME cli.perf_sketch_pattern COMMAND mitcad-cli run "${scripts}/perf_sketch_pattern_10.json" --timings)
  set_tests_properties(cli.perf_sketch_pattern PROPERTIES TIMEOUT 20 RUN_SERIAL TRUE
    PASS_REGULAR_EXPRESSION "Timing steps\\[16\\] sketch\\.drag: [0-9.]+ ms.*F1 Sketch1 \\(sketch\\): ok")
  # The same at 20 x 20, 400 circles (mitcad#677): the solver's cost grows
  # about linearly with the copies since each copy is bound to its
  # neighbour. The sketch query of step 18 (a solve and the analysis of the
  # whole sketch) took 650 ms in a Linux release build with every copy
  # bound to the original and takes about 5 ms now: it must stay under
  # 100 ms. The drag steps also find the sketch's regions (about 0.75 s of
  # each step in mitcad#677, mitcad#678); the time limit only catches a
  # large slowdown.
  add_test(NAME cli.perf_sketch_pattern_20 COMMAND mitcad-cli run "${scripts}/perf_sketch_pattern_20.json" --timings)
  set_tests_properties(cli.perf_sketch_pattern_20 PROPERTIES TIMEOUT 120 RUN_SERIAL TRUE
    PASS_REGULAR_EXPRESSION "Timing steps\\[18\\] expect sketch: [0-9]?[0-9]\\.[0-9]+ ms.*F1 Sketch1 \\(sketch\\): ok")

  # The result store (P7d): the perf model opened with an empty store fills
  # it (every result the store takes, however quick), opened again takes
  # them from it with the same bodies (the four sketches are evaluated), and
  # a copy with its last fillet edited evaluates only that after them.
  set(store "${perf}/store")
  set(volume "Body1 \\(F2\\.b0\\): volume 80967\\.896 mm3")
  add_test(NAME cli.store_clean COMMAND "${CMAKE_COMMAND}" -E rm -rf "${store}")
  set_tests_properties(cli.store_clean PROPERTIES FIXTURES_SETUP cli_store_clean)
  add_test(NAME cli.store_fill COMMAND mitcad-cli info "${perf}/perf_model.mitcad"
                                       --result-store "${store}" --persist-min-ms 0)
  set_tests_properties(cli.store_fill PROPERTIES FIXTURES_REQUIRED "cli_perf;cli_store_clean"
    FIXTURES_SETUP cli_store
    PASS_REGULAR_EXPRESSION "restored from store: 0, evaluated: 12. stored: 8 .*${volume}")
  add_test(NAME cli.store_hit COMMAND mitcad-cli info "${perf}/perf_model.mitcad"
                                      --result-store "${store}" --persist-min-ms 0)
  set_tests_properties(cli.store_hit PROPERTIES FIXTURES_REQUIRED cli_store
    PASS_REGULAR_EXPRESSION "restored from store: 8, evaluated: 4. stored: 0 .*F12 Fillet2 \\(fillet\\): ok.*${volume}")
  add_test(NAME cli.store_edit_file COMMAND mitcad-cli run "${scripts}/store_edit.json"
                                            --open "${perf}/perf_model.mitcad" --save "${perf}/perf_edit.mitcad")
  set_tests_properties(cli.store_edit_file PROPERTIES FIXTURES_REQUIRED cli_perf FIXTURES_SETUP cli_store_edited)
  add_test(NAME cli.store_edit COMMAND mitcad-cli info "${perf}/perf_edit.mitcad"
                                       --result-store "${store}" --persist-min-ms 0)
  set_tests_properties(cli.store_edit PROPERTIES FIXTURES_REQUIRED "cli_store;cli_store_edited"
    PASS_REGULAR_EXPRESSION "restored from store: 7, evaluated: 5. stored: 1 .*F12 Fillet2 \\(fillet\\): ok")
  # The caches' diagnostics (the cache query): where the results came from,
  # the process's memory and the store's files.
  add_test(NAME cli.store_diagnostics COMMAND mitcad-cli info "${perf}/perf_model.mitcad"
                                              --result-store "${store}" --diagnostics)
  set_tests_properties(cli.store_diagnostics PROPERTIES FIXTURES_REQUIRED cli_store
    PASS_REGULAR_EXPRESSION
    "Cache: {\"last_recompute\":{\"cached\":0,.*\"restored\":8,\"restored_packed\":[0-9]+},\"memory\":{.*\"process\":{\"heap\":[0-9]+,\"peak_resident\":[0-9]+,\"resident\":[1-9][0-9]*},\"store\":{.*\"disk\":{.*\"by_type\":\\[{.*\"type\":\"(rectangular|circular)_pattern\"")

  # Small UI gaps (P9): a full round and a chamfer as wide as a rib build at
  # their size; a dome on a boss built 0.1 % smaller is information; a plane
  # at 90 radians warns.
  add_test(NAME cli.p9_warnings COMMAND mitcad-cli run "${scripts}/p9_warnings.json")

  # A block's end rounded to a half round in one fillet keeps its top and
  # bottom named, as in two fillets (mitcad#821).
  add_test(NAME cli.fillet_meeting_roundings COMMAND mitcad-cli run "${scripts}/fillet_meeting_roundings.json")

  # Lofts with end conditions and rails (P2): analytic volumes, rails, and
  # a conditioned loft following an edit of the face it starts from.
  add_test(NAME cli.p2_loft_conditions COMMAND mitcad-cli run "${scripts}/p2_loft_conditions.json")

  # Decimal commas in values and expressions (mitcad#2): the same volumes
  # as with points, and expressions kept with points.
  add_test(NAME cli.decimal_commas COMMAND mitcad-cli run "${scripts}/decimal_commas.json")

  # The final render from the command line (mitcad#48): a design rendered by
  # the render worker in every format.
  add_test(NAME cli.render
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORKER=$<TARGET_FILE:mitcad-render>"
            "-DSCRIPT=${scripts}/render_scene.json" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/render"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/render-test.cmake")
  # Seconds normally; a hung GPU fails it in two minutes (mitcad#146).
  set_tests_properties(cli.render PROPERTIES TIMEOUT 120)
  # Texture map sets in the final render (mitcad#314): maps that change the
  # image, a missing one and one that is no image left out.
  add_test(NAME cli.render_texture_maps
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORKER=$<TARGET_FILE:mitcad-render>"
            "-DSCRIPT=${scripts}/render_texture_maps.json" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/render-texture-maps"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/render-texture-maps-test.cmake")
  set_tests_properties(cli.render_texture_maps PROPERTIES TIMEOUT 120)

  # Joints between occurrences (mitcad#55): joints placed on the geometry
  # kernel's faces and edges, saved and opened again.
  set(joints "${CMAKE_CURRENT_BINARY_DIR}/joints")
  file(MAKE_DIRECTORY "${joints}")
  add_test(NAME cli.joints COMMAND mitcad-cli run "${scripts}/joints.json" --save "${joints}/joints.mitcad")
  set_tests_properties(cli.joints PROPERTIES FIXTURES_SETUP cli_joints)
  add_test(NAME cli.joints_info COMMAND mitcad-cli info "${joints}/joints.mitcad")
  set_tests_properties(cli.joints_info PROPERTIES FIXTURES_REQUIRED cli_joints PASS_REGULAR_EXPRESSION
    "F6 Joint1 \\(joint\\): ok.*F7 Joint2 \\(joint\\): ok")
  # A new joint's moving side where its origins meet at the offset, a new
  # offset that moves it and a drag kept although a move in the timeline
  # places it (mitcad#1745, a reported roller assembly reduced).
  add_test(NAME cli.joint_start COMMAND mitcad-cli run "${scripts}/joint_start.json")

  # Motion studies and contact sets (mitcad#454): a study played and a
  # slider stopped at a wall by a contact set on the kernel's bodies, saved
  # and opened again.
  set(motion_contact "${CMAKE_CURRENT_BINARY_DIR}/motion-contact")
  file(MAKE_DIRECTORY "${motion_contact}")
  add_test(NAME cli.motion_contact COMMAND mitcad-cli run "${scripts}/motion_contact.json"
           --save "${motion_contact}/motion_contact.mitcad")
  set_tests_properties(cli.motion_contact PROPERTIES FIXTURES_SETUP cli_motion_contact)
  add_test(NAME cli.motion_contact_info COMMAND mitcad-cli info "${motion_contact}/motion_contact.mitcad")
  set_tests_properties(cli.motion_contact_info PROPERTIES FIXTURES_REQUIRED cli_motion_contact
    PASS_REGULAR_EXPRESSION "F7 Joint1 \\(joint\\): ok")

  # Motion links (mitcad#466): a rack and pinion sized by the pinion's round
  # face, driven from either side, saved and opened again.
  set(motion_links "${CMAKE_CURRENT_BINARY_DIR}/motion-links")
  file(MAKE_DIRECTORY "${motion_links}")
  add_test(NAME cli.motion_links COMMAND mitcad-cli run "${scripts}/motion_links.json"
           --save "${motion_links}/motion_links.mitcad")
  set_tests_properties(cli.motion_links PROPERTIES FIXTURES_SETUP cli_motion_links)
  add_test(NAME cli.motion_links_info COMMAND mitcad-cli info "${motion_links}/motion_links.mitcad")
  set_tests_properties(cli.motion_links_info PROPERTIES FIXTURES_REQUIRED cli_motion_links
    PASS_REGULAR_EXPRESSION "F9 MotionLink1 \\(motion_link\\): ok")

  # Emboss (mitcad#458): a rectangle and a text wrapped onto a rod, raised
  # and sunk, a square on its end face, with analytic volumes.
  add_test(NAME cli.emboss COMMAND mitcad-cli run "${scripts}/emboss.json")
  # Emboss by projection (mitcad#1091): a square and a text projected onto
  # a ball, a square onto a B-spline face and one partly off it, a square
  # along a direction onto a box's top.
  add_test(NAME cli.emboss_projection COMMAND mitcad-cli run "${scripts}/emboss_projection.json")

  # Boundary Fill (mitcad#457): the cells of overlapping boxes as new
  # bodies, a cell of two solids cut from and joined to a plate, saved and
  # opened again.
  set(boundary_fill "${CMAKE_CURRENT_BINARY_DIR}/boundary-fill")
  file(MAKE_DIRECTORY "${boundary_fill}")
  add_test(NAME cli.boundary_fill COMMAND mitcad-cli run "${scripts}/boundary_fill.json"
           --save "${boundary_fill}/boundary_fill.mitcad")
  set_tests_properties(cli.boundary_fill PROPERTIES FIXTURES_SETUP cli_boundary_fill)
  add_test(NAME cli.boundary_fill_info COMMAND mitcad-cli info "${boundary_fill}/boundary_fill.mitcad")
  set_tests_properties(cli.boundary_fill_info PROPERTIES FIXTURES_REQUIRED cli_boundary_fill
    PASS_REGULAR_EXPRESSION "F3 BoundaryFill1 \\(boundary_fill\\): ok.*F7 BoundaryFill2 \\(boundary_fill\\): ok")

  # Assembly constraints (mitcad#148): a pin inserted into a hole, on its
  # axis with the depth free and dragged along it, conflicting rows named,
  # saved and opened again.
  set(constraints "${CMAKE_CURRENT_BINARY_DIR}/constraints")
  file(MAKE_DIRECTORY "${constraints}")
  add_test(NAME cli.constraints COMMAND mitcad-cli run "${scripts}/constraints.json"
           --save "${constraints}/constraints.mitcad")
  set_tests_properties(cli.constraints PROPERTIES FIXTURES_SETUP cli_constraints)
  add_test(NAME cli.constraints_info COMMAND mitcad-cli info "${constraints}/constraints.mitcad")
  set_tests_properties(cli.constraints_info PROPERTIES FIXTURES_REQUIRED cli_constraints
    PASS_REGULAR_EXPRESSION "F6 Constraint2 \\(constraint_set\\): ok.*F8 Constraint3 \\(constraint_set\\): ok")

  # Component libraries (mitcad#64, mitcad#63): a small fastener library
  # made by the generator, fetched, its parts inserted, compared and
  # updated; needs git and Python.
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(GIT_FOUND AND Python3_Interpreter_FOUND)
    add_test(NAME cli.library
      COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DGIT=${GIT_EXECUTABLE}"
              "-DPYTHON=${Python3_EXECUTABLE}"
              "-DGENERATOR=${PROJECT_SOURCE_DIR}/tools/libraries/make-fastener-library.py"
              "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/library" -P "${CMAKE_CURRENT_SOURCE_DIR}/library-test.cmake")
    # Mitcad's profile library (mitcad#375): the generator on synthetic
    # sections, a bracket and a panel made in the test; the library checked,
    # fetched, searched and its connectors fitted to junctions.
    add_test(NAME cli.profile-library
      COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DGIT=${GIT_EXECUTABLE}"
              "-DPYTHON=${Python3_EXECUTABLE}"
              "-DGENERATOR=${PROJECT_SOURCE_DIR}/tools/libraries/make-profile-library.py"
              "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/profile-library"
              -P "${CMAKE_CURRENT_SOURCE_DIR}/profile-library-test.cmake")
    # The timber parts (mitcad#478): the generator on the test's timber
    # group files (boards, a sheet, wood fasteners, a bracket, a hanger, a
    # post base); their manifest fields, designs and pickers.
    add_test(NAME cli.timber-library
      COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DGIT=${GIT_EXECUTABLE}"
              "-DPYTHON=${Python3_EXECUTABLE}"
              "-DGENERATOR=${PROJECT_SOURCE_DIR}/tools/libraries/make-profile-library.py"
              "-DGROUPS=${CMAKE_CURRENT_SOURCE_DIR}/tests/timber-groups"
              "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/timber-library"
              -P "${CMAKE_CURRENT_SOURCE_DIR}/timber-library-test.cmake")
    # The fasteners' rendered previews (mitcad#526): a socket screw and a
    # wood screw in the generators' studio, their size, coverage and
    # finish's colours; kept when unchanged; drawn without a render worker.
    # Skipped where the worker cannot render.
    add_test(NAME cli.library-previews
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/library-previews-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>" --worker "$<TARGET_FILE:mitcad-render>"
              --libraries "${PROJECT_SOURCE_DIR}/tools/libraries"
              --work "${CMAKE_CURRENT_BINARY_DIR}/library-previews")
    set_tests_properties(cli.library-previews PROPERTIES TIMEOUT 600
      SKIP_REGULAR_EXPRESSION "library previews: skipped")
    # A library version is tagged once (mitcad#1372): each generator with
    # --commit refuses a --version whose tag its --out repository already
    # has, naming the tag, before it makes anything; tools/libraries'
    # library_tags.py on a throwaway repository of the build tree.
    add_test(NAME cli.library-tags
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/library-tags-test.py"
              --libraries "${PROJECT_SOURCE_DIR}/tools/libraries" --git "${GIT_EXECUTABLE}"
              --work "${CMAKE_CURRENT_BINARY_DIR}/library-tags")
    set_tests_properties(cli.library-tags PROPERTIES ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
  endif()
  # The fastener generator's dimension tables (mitcad#900): sizes, pitches,
  # length series, each standard's rows and the earlier versions' rows;
  # needs only Python.
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.fastener-tables
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/fastener-tables-test.py"
              --libraries "${PROJECT_SOURCE_DIR}/tools/libraries")
    # The parts cut to length (mitcad#1950): the profile generator's length
    # option and stock rows for every part of the group files with
    # build.length; needs only Python.
    add_test(NAME cli.profile-length-options
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/profile-length-options-test.py"
              --libraries "${PROJECT_SOURCE_DIR}/tools/libraries")
    set_tests_properties(cli.profile-length-options PROPERTIES ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
  endif()
  # BUILD on the generated profile library (mitcad#376, mitcad#377): one
  # profile of each system and slot width with its frames and mass per
  # metre, and the linear parts on a small frame with their joints driven;
  # skipped without MITCAD_PROFILES_LIBRARY (the generated library's folder).
  add_test(NAME build.profiles
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/build-profiles"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/build-profiles-test.cmake")
  set_tests_properties(build.profiles PROPERTIES SKIP_REGULAR_EXPRESSION "build profiles: skipped")
  # The timber parts of the generated library (mitcad#478): every family's
  # sizes and marking, sections, densities, the fasteners and connectors;
  # skipped without MITCAD_PROFILES_LIBRARY.
  add_test(NAME build.timber
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/build-timber"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/build-timber-test.cmake")
  set_tests_properties(build.timber PROPERTIES SKIP_REGULAR_EXPRESSION "build timber: skipped")
  # A sketch edit that splits the regions an extrusion uses (mitcad#407):
  # every piece stays extruded, with a warning naming them.
  add_test(NAME cli.region_split_pieces COMMAND mitcad-cli run "${scripts}/region_split_pieces.json")
endif()

# Live updates (mitcad#89): mitcad-cli live test. Its tests need no broker:
# a password over plain text is refused, a broker that cannot be reached is
# reported (the client against brokers is tested in core.vcs).
target_sources(mitcad-cli PRIVATE live.cpp live.hpp)
if(BUILD_TESTING)
  add_test(NAME cli.live_insecure COMMAND mitcad-cli live test mqtt://127.0.0.1:9 --user someone)
  set_tests_properties(cli.live_insecure PROPERTIES ENVIRONMENT "MITCAD_MQTT_PASSWORD=secret"
    PASS_REGULAR_EXPRESSION "sent only over TLS")
  add_test(NAME cli.live_unreachable COMMAND mitcad-cli live test mqtt://127.0.0.1:9 --timeout 3)
  set_tests_properties(cli.live_unreachable PROPERTIES PASS_REGULAR_EXPRESSION
    "connect: failed \\([0-9]+ ms\\).*Warning: mqtt://127.0.0.1:9 is plain text.*Failed: ")
endif()

# Pattern at Points (mitcad#197): actual shared-component geometry in
# rotated nested occurrences, and the same definition opened in a fresh process.
if(BUILD_TESTING)
  set(point_components "${CMAKE_CURRENT_BINARY_DIR}/pattern_at_points_components.mitcad")
  add_test(NAME cli.pattern_at_points_components
    COMMAND mitcad-cli run "${scripts}/pattern_at_points_components.json" --save "${point_components}")
  set_tests_properties(cli.pattern_at_points_components PROPERTIES FIXTURES_SETUP cli_point_components)
  add_test(NAME cli.pattern_at_points_components_reopen
    COMMAND mitcad-cli run "${scripts}/pattern_at_points_components_reopen.json" --open "${point_components}")
  set_tests_properties(cli.pattern_at_points_components_reopen PROPERTIES FIXTURES_REQUIRED cli_point_components)
endif()

# Sheet Face (mitcad#206): analytic curved-hole/island geometry and rule
# lifecycle, including shared nested occurrences and independent copies.
if(BUILD_TESTING)
  foreach(sheet_case sheet_face_geometry sheet_face_components)
    set(sheet_file "${CMAKE_CURRENT_BINARY_DIR}/${sheet_case}.mitcad")
    add_test(NAME cli.${sheet_case}
      COMMAND mitcad-cli run "${scripts}/${sheet_case}.json" --save "${sheet_file}")
    set_tests_properties(cli.${sheet_case} PROPERTIES FIXTURES_SETUP cli_${sheet_case})
    add_test(NAME cli.${sheet_case}_reopen
      COMMAND mitcad-cli run "${scripts}/${sheet_case}_reopen.json" --open "${sheet_file}")
    set_tests_properties(cli.${sheet_case}_reopen PROPERTIES FIXTURES_REQUIRED cli_${sheet_case})
  endforeach()
endif()

# Folded sheet features (mitcad#207): independently calculated dimensions and
# volumes, both native Bend inputs, contour corners and persistence.
if(BUILD_TESTING)
  foreach(sheet_case sheet_flange_geometry sheet_fold_geometry sheet_fold_reliefs sheet_bend_coplanar
                     sheet_bend_positioned sheet_corner_round sheet_corner_chamfer
                     sheet_flange_components)
    set(sheet_file "${CMAKE_CURRENT_BINARY_DIR}/${sheet_case}.mitcad")
    add_test(NAME cli.${sheet_case}
      COMMAND mitcad-cli run "${scripts}/${sheet_case}.json" --save "${sheet_file}")
    set_tests_properties(cli.${sheet_case} PROPERTIES FIXTURES_SETUP cli_${sheet_case})
    add_test(NAME cli.${sheet_case}_reopen
      COMMAND mitcad-cli run "${scripts}/${sheet_case}_reopen.json" --open "${sheet_file}")
    set_tests_properties(cli.${sheet_case}_reopen PROPERTIES FIXTURES_REQUIRED cli_${sheet_case})
  endforeach()
  # A flange at the edge, the imports' flanges whose bend starts at the
  # selected edge (mitcad#211).
  add_test(NAME cli.sheet_flange_edge COMMAND mitcad-cli run "${scripts}/sheet_flange_edge.json")
  # A simple hole through a sheet keeps it a sheet (mitcad#211).
  add_test(NAME cli.sheet_hole COMMAND mitcad-cli run "${scripts}/sheet_hole.json")
  # A flange whose trim reaches an existing bend's seam: adjacent flanges,
  # a flange on a wall's side edge (mitcad#211).
  add_test(NAME cli.sheet_flange_adjacent COMMAND mitcad-cli run "${scripts}/sheet_flange_adjacent.json")
  add_test(NAME cli.sheet_flange_on_wall COMMAND mitcad-cli run "${scripts}/sheet_flange_on_wall.json")
  # Hems up to 180 degrees, the side-tangent position, the outer-tangent
  # datum and widths from the boundary's middle (mitcad#242).
  add_test(NAME cli.sheet_flange_hem COMMAND mitcad-cli run "${scripts}/sheet_flange_hem.json")
  # Bends past 180 degrees: rolled, teardrop, rope and double hems, their
  # volumes and flat lengths (mitcad#1378).
  add_test(NAME cli.sheet_flange_hem_past COMMAND mitcad-cli run "${scripts}/sheet_flange_hem_past.json")
  # The Hem command's forms (mitcad#2112): flat, open and rolled hems give
  # the imports' flanges' bodies, an edit through all six forms, refusals.
  add_test(NAME cli.sheet_hem_forms COMMAND mitcad-cli run "${scripts}/sheet_hem_forms.json")
  # Walls of one sheet that meet: a full open corner accepted, a face
  # contact and a penetration refused naming both walls (mitcad#1409).
  add_test(NAME cli.sheet_open_corner COMMAND mitcad-cli run "${scripts}/sheet_open_corner.json")
  # Corner joints of a closing flange: Open and Half Open (mitcad#1410).
  add_test(NAME cli.sheet_flange_corner COMMAND mitcad-cli run "${scripts}/sheet_flange_corner.json")
  # A flange on several connected edges with mitred corners and a corner
  # gap (mitcad#1872): a box's three edges Inside and Outside, gaps 0 and 1,
  # turned down, a wall running past, refusals; the sides of two faces a
  # bend joins, walls in one plane mitred on the bend's bisecting line.
  add_test(NAME cli.sheet_flange_edges COMMAND mitcad-cli run "${scripts}/sheet_flange_edges.json")
  add_test(NAME cli.sheet_flange_edges_bend COMMAND mitcad-cli run "${scripts}/sheet_flange_edges_bend.json")
  # A new flange whose free end meets an existing flange across a bend
  # stops the Corner Gap short of its wall, never cutting it (mitcad#1877).
  add_test(NAME cli.sheet_flange_edges_existing COMMAND mitcad-cli run "${scripts}/sheet_flange_edges_existing.json")
  # Edges whose ends face each other across a rip or a corner gap the sheet
  # records are one corner (mitcad#2098): a converted tray's rim and the rim
  # of a tray whose walls are a flange on several edges, both ways, every
  # Position.
  add_test(NAME cli.sheet_flange_edges_gap COMMAND mitcad-cli run "${scripts}/sheet_flange_edges_gap.json")
  # A face joined to a sheet in its panel's plane (mitcad#242).
  add_test(NAME cli.sheet_face_join COMMAND mitcad-cli run "${scripts}/sheet_face_join.json")
  # The sheet-domain adapter of ordinary features (mitcad#242).
  add_test(NAME cli.sheet_adapter COMMAND mitcad-cli run "${scripts}/sheet_adapter.json")
  # Mirrors and patterns of sheet features (mitcad#242).
  add_test(NAME cli.sheet_mirror COMMAND mitcad-cli run "${scripts}/sheet_mirror.json")
  add_test(NAME cli.sheet_mirror_flange COMMAND mitcad-cli run "${scripts}/sheet_mirror_flange.json")
  add_test(NAME cli.sheet_corner_whole COMMAND mitcad-cli run "${scripts}/sheet_corner_whole.json")
  # Ordinary fillets and chamfers on a sheet kept as overlays (mitcad#242).
  add_test(NAME cli.sheet_dressup_overlay COMMAND mitcad-cli run "${scripts}/sheet_dressup_overlay.json")
  # Coplanar panels of one face meeting along an arc (mitcad#242).
  add_test(NAME cli.sheet_curved_seam COMMAND mitcad-cli run "${scripts}/sheet_curved_seam.json")
  # A Face with a filled round hole and a Flange unfolds (mitcad#1942).
  add_test(NAME cli.sheet_unfold_disc COMMAND mitcad-cli run "${scripts}/sheet_unfold_disc.json")
  # Patterns and mirrors of features kept as sheet overlays (mitcad#242).
  add_test(NAME cli.sheet_pattern_straight_cut COMMAND mitcad-cli run "${scripts}/sheet_pattern_straight_cut.json")
  add_test(NAME cli.sheet_mirror_dressup COMMAND mitcad-cli run "${scripts}/sheet_mirror_dressup.json")
  # Straight sheet cuts, through a bend kept as an overlay (mitcad#242).
  set(sheet_file "${CMAKE_CURRENT_BINARY_DIR}/sheet_cut_straight.mitcad")
  add_test(NAME cli.sheet_cut_straight
    COMMAND mitcad-cli run "${scripts}/sheet_cut_straight.json" --save "${sheet_file}")
  set_tests_properties(cli.sheet_cut_straight PROPERTIES FIXTURES_SETUP cli_sheet_cut_straight)
  add_test(NAME cli.sheet_cut_straight_reopen
    COMMAND mitcad-cli run "${scripts}/sheet_cut_straight_reopen.json" --open "${sheet_file}")
  set_tests_properties(cli.sheet_cut_straight_reopen PROPERTIES FIXTURES_REQUIRED cli_sheet_cut_straight)
  # Overhangs of a flange wider than its boundary from the bend's tangent
  # line (the .ipt flanges', mitcad#242).
  add_test(NAME cli.sheet_flange_overhang COMMAND mitcad-cli run "${scripts}/sheet_flange_overhang.json")
  # Double bends between parallel panels, and mirrors of faces that make
  # new sheet bodies (mitcad#280).
  set(sheet_file "${CMAKE_CURRENT_BINARY_DIR}/sheet_double_bend.mitcad")
  add_test(NAME cli.sheet_double_bend
    COMMAND mitcad-cli run "${scripts}/sheet_double_bend.json" --save "${sheet_file}")
  set_tests_properties(cli.sheet_double_bend PROPERTIES FIXTURES_SETUP cli_sheet_double_bend)
  add_test(NAME cli.sheet_double_bend_reopen
    COMMAND mitcad-cli run "${scripts}/sheet_double_bend_reopen.json" --open "${sheet_file}")
  set_tests_properties(cli.sheet_double_bend_reopen PROPERTIES FIXTURES_REQUIRED cli_sheet_double_bend)
  add_test(NAME cli.sheet_double_bend_mirror COMMAND mitcad-cli run "${scripts}/sheet_double_bend_mirror.json")
  # A round past a short side, a fold line on the back surface and a cut
  # ending on the seam of a folded bend (mitcad#280).
  add_test(NAME cli.sheet_corner_round_past COMMAND mitcad-cli run "${scripts}/sheet_corner_round_past.json")
  add_test(NAME cli.sheet_fold_back COMMAND mitcad-cli run "${scripts}/sheet_fold_back.json")
  add_test(NAME cli.sheet_cut_on_seam COMMAND mitcad-cli run "${scripts}/sheet_cut_on_seam.json")
  # A fold whose line crosses a cut window (mitcad#288).
  add_test(NAME cli.sheet_fold_window COMMAND mitcad-cli run "${scripts}/sheet_fold_window.json")
  # Folds across edges oblique to their lines (mitcad#1424).
  add_test(NAME cli.sheet_fold_oblique COMMAND mitcad-cli run "${scripts}/sheet_fold_oblique.json")
  add_test(NAME cli.sheet_fold_oblique_polygon COMMAND mitcad-cli run "${scripts}/sheet_fold_oblique_polygon.json")
  # Flanges on edges whose sides meet them at other angles than 90 degrees (mitcad#1576).
  add_test(NAME cli.sheet_flange_oblique COMMAND mitcad-cli run "${scripts}/sheet_flange_oblique.json")
  # A fold whose sides an earlier fold's bend joins too, refused naming it (mitcad#1442).
  add_test(NAME cli.sheet_fold_loop COMMAND mitcad-cli run "${scripts}/sheet_fold_loop.json")
  # A pattern of a straight sheet cut with many copies, cut at once (mitcad#288).
  add_test(NAME cli.sheet_pattern_holes COMMAND mitcad-cli run "${scripts}/sheet_pattern_holes.json")
  # Ordinary cuts (Extrude, Hole) across a bend keep the sheet (mitcad#1628).
  add_test(NAME cli.sheet_cut_across_bend COMMAND mitcad-cli run "${scripts}/sheet_cut_across_bend.json")
  # Contour Flange, Convert to Sheet Metal and Perforate (mitcad#461).
  set(perforation_fixture "${CMAKE_CURRENT_BINARY_DIR}/sheet_perforate_outline.mitcad")
  add_test(NAME cli.sheet_perforate_outline_setup
    COMMAND mitcad-cli run "${PROJECT_SOURCE_DIR}/tools/ui-scripts/sheet-perforate.json" --save "${perforation_fixture}")
  set_tests_properties(cli.sheet_perforate_outline_setup PROPERTIES FIXTURES_SETUP cli_sheet_perforate_outline)
  add_test(NAME cli.sheet_perforate_outline
    COMMAND mitcad-cli run "${scripts}/sheet_perforate_outline.json" --open "${perforation_fixture}")
  set_tests_properties(cli.sheet_perforate_outline PROPERTIES FIXTURES_REQUIRED cli_sheet_perforate_outline)
  foreach(sheet_case sheet_contour sheet_perforate sheet_convert)
    set(sheet_file "${CMAKE_CURRENT_BINARY_DIR}/${sheet_case}.mitcad")
    add_test(NAME cli.${sheet_case}
      COMMAND mitcad-cli run "${scripts}/${sheet_case}.json" --save "${sheet_file}")
    set_tests_properties(cli.${sheet_case} PROPERTIES FIXTURES_SETUP cli_${sheet_case})
    add_test(NAME cli.${sheet_case}_reopen
      COMMAND mitcad-cli run "${scripts}/${sheet_case}_reopen.json" --open "${sheet_file}")
    set_tests_properties(cli.${sheet_case}_reopen PROPERTIES FIXTURES_REQUIRED cli_${sheet_case})
  endforeach()
  # Convert to Sheet Metal of a level and an inclined tray, its corners
  # ripped (mitcad#1949).
  add_test(NAME cli.sheet_convert_tray COMMAND mitcad-cli run "${scripts}/sheet_convert_tray.json")
  # Construction planes, axes and points on the edges and vertices of sheet
  # bodies, whose straight edges are B-spline curves (mitcad#1537).
  add_test(NAME cli.sheet_datums COMMAND mitcad-cli run "${scripts}/sheet_datums.json")
  # Flat faces of sheet bodies (planar B-spline walls among them) as the
  # planes of Mirror, Split Body, Align, sketches and construction planes
  # (mitcad#1554).
  add_test(NAME cli.sheet_face_planes COMMAND mitcad-cli run "${scripts}/sheet_face_planes.json")
  # Convert to Sheet Metal of cylindrical and conical faces, opened at their
  # seams, a hole developed, their Flat Patterns and DXFs; faces that cannot
  # be laid flat refused (mitcad#2435).
  add_test(NAME cli.sheet_convert_curved COMMAND mitcad-cli run "${scripts}/sheet_convert_curved.json")
endif()

# Sheet Unfold, Refold and Flat Pattern (mitcad#208): actual L, U and Z
# developments, partial opening, flat outlines, overlap refusal and persistence.
if(BUILD_TESTING)
  foreach(sheet_case sheet_unfold_l sheet_unfold_u sheet_unfold_z sheet_flat_overlap sheet_flat_components
                     sheet_cut_proof sheet_cut_partial)
    set(sheet_file "${CMAKE_CURRENT_BINARY_DIR}/${sheet_case}.mitcad")
    add_test(NAME cli.${sheet_case}
      COMMAND mitcad-cli run "${scripts}/${sheet_case}.json" --save "${sheet_file}")
    set_tests_properties(cli.${sheet_case} PROPERTIES FIXTURES_SETUP cli_${sheet_case})
    add_test(NAME cli.${sheet_case}_reopen
      COMMAND mitcad-cli run "${scripts}/${sheet_case}_reopen.json" --open "${sheet_file}")
    set_tests_properties(cli.${sheet_case}_reopen PROPERTIES FIXTURES_REQUIRED cli_${sheet_case})
  endforeach()
endif()

# Production sheet Cut and corner reliefs (mitcad#209): several sketches and
# contours across one and two bends, adjacency and rule edits, split bodies,
# removed domains, relief shapes and their refusals, each with a reopen pass.
if(BUILD_TESTING)
  foreach(sheet_case sheet_cut_multi sheet_cut_split sheet_corner_relief)
    set(sheet_file "${CMAKE_CURRENT_BINARY_DIR}/${sheet_case}.mitcad")
    add_test(NAME cli.${sheet_case}
      COMMAND mitcad-cli run "${scripts}/${sheet_case}.json" --save "${sheet_file}")
    set_tests_properties(cli.${sheet_case} PROPERTIES FIXTURES_SETUP cli_${sheet_case})
    add_test(NAME cli.${sheet_case}_reopen
      COMMAND mitcad-cli run "${scripts}/${sheet_case}_reopen.json" --open "${sheet_file}")
    set_tests_properties(cli.${sheet_case}_reopen PROPERTIES FIXTURES_REQUIRED cli_${sheet_case})
  endforeach()
endif()

# Flat Pattern DXF export (mitcad#210): contours, bend lines, layers and units
# read back from the files, rule edits, reopening, and the export-flat command
# (refused for an invalid pattern).
if(BUILD_TESTING)
  set(sheet_dxf "${CMAKE_CURRENT_BINARY_DIR}/sheet_flat_dxf")
  file(MAKE_DIRECTORY "${sheet_dxf}")
  add_test(NAME cli.sheet_flat_dxf
    COMMAND mitcad-cli run "${scripts}/sheet_flat_dxf.json" --save "${sheet_dxf}/sheet_flat_dxf.mitcad")
  set_tests_properties(cli.sheet_flat_dxf PROPERTIES WORKING_DIRECTORY "${sheet_dxf}"
    FIXTURES_SETUP cli_sheet_flat_dxf)
  add_test(NAME cli.sheet_flat_dxf_reopen
    COMMAND mitcad-cli run "${scripts}/sheet_flat_dxf_reopen.json" --open "${sheet_dxf}/sheet_flat_dxf.mitcad")
  set_tests_properties(cli.sheet_flat_dxf_reopen PROPERTIES WORKING_DIRECTORY "${sheet_dxf}"
    FIXTURES_REQUIRED cli_sheet_flat_dxf)
  add_test(NAME cli.export_flat
    COMMAND mitcad-cli export-flat "${sheet_dxf}/sheet_flat_dxf.mitcad" F3.b0 "${sheet_dxf}/flat-in.dxf"
            --unit in --tangents --json)
  set_tests_properties(cli.export_flat PROPERTIES FIXTURES_REQUIRED cli_sheet_flat_dxf
    PASS_REGULAR_EXPRESSION "\"entities\":8.*\"unit\":\"in\"")
  add_test(NAME cli.export_flat_invalid
    COMMAND mitcad-cli export-flat "${CMAKE_CURRENT_BINARY_DIR}/sheet_flat_overlap.mitcad" F3.b0
            "${sheet_dxf}/overlap.dxf")
  set_tests_properties(cli.export_flat_invalid PROPERTIES FIXTURES_REQUIRED cli_sheet_flat_overlap
    WILL_FAIL TRUE)
endif()

# Sheet-metal leftovers of mitcad#125: corner reliefs of bends without a common
# panel and of non-perpendicular bends, and Paste New of split pieces with their
# Flat Patterns, each with a reopen pass.
if(BUILD_TESTING)
  foreach(sheet_case sheet_corner_relief_paths sheet_corner_relief_angled sheet_copy_pieces)
    set(sheet_file "${CMAKE_CURRENT_BINARY_DIR}/${sheet_case}.mitcad")
    add_test(NAME cli.${sheet_case}
      COMMAND mitcad-cli run "${scripts}/${sheet_case}.json" --save "${sheet_file}")
    set_tests_properties(cli.${sheet_case} PROPERTIES FIXTURES_SETUP cli_${sheet_case})
    add_test(NAME cli.${sheet_case}_reopen
      COMMAND mitcad-cli run "${scripts}/${sheet_case}_reopen.json" --open "${sheet_file}")
    set_tests_properties(cli.${sheet_case}_reopen PROPERTIES FIXTURES_REQUIRED cli_${sheet_case})
  endforeach()
endif()

# References that find faces, edges and vertices a sketch change renamed
# (mitcad#272), through the OCCT kernel.
if(BUILD_TESTING)
  add_test(NAME cli.renamed_face_refs COMMAND mitcad-cli run "${scripts}/renamed_face_refs.json")
endif()

# Bounding solids (mitcad#460): the box of exact geometry, following its
# bodies' and its margin's edits, through the OCCT kernel.
if(BUILD_TESTING)
  add_test(NAME cli.bounding_solid COMMAND mitcad-cli run "${scripts}/bounding_solid.json")
endif()

# Convert Mesh (mitcad#462): a plate written to STL, read back as a mesh body
# and rebuilt as planes; a solid and a torus's mesh refused; the design saved
# and opened again.
if(BUILD_TESTING)
  set(mesh_convert "${CMAKE_CURRENT_BINARY_DIR}/mesh-convert")
  file(MAKE_DIRECTORY "${mesh_convert}")
  add_test(NAME cli.mesh_convert
    COMMAND mitcad-cli run "${scripts}/mesh_convert.json" --save "${mesh_convert}/mesh_convert.mitcad")
  set_tests_properties(cli.mesh_convert PROPERTIES WORKING_DIRECTORY "${mesh_convert}"
    FIXTURES_SETUP cli_mesh_convert)
  add_test(NAME cli.mesh_convert_reopen
    COMMAND mitcad-cli run "${scripts}/mesh_convert_reopen.json" --open "${mesh_convert}/mesh_convert.mitcad")
  set_tests_properties(cli.mesh_convert_reopen PROPERTIES FIXTURES_REQUIRED cli_mesh_convert)
endif()

# Convert Mesh's Faceted solid (mitcad#1082): the plate's STL mesh welded and
# merged into a new solid of 10 planar faces, the mesh body hidden; a
# torus's mesh made a faceted solid too.
if(BUILD_TESTING)
  set(mesh_convert_faceted "${CMAKE_CURRENT_BINARY_DIR}/mesh-convert-faceted")
  file(MAKE_DIRECTORY "${mesh_convert_faceted}")
  add_test(NAME cli.mesh_convert_faceted COMMAND mitcad-cli run "${scripts}/mesh_convert_faceted.json")
  set_tests_properties(cli.mesh_convert_faceted PROPERTIES WORKING_DIRECTORY "${mesh_convert_faceted}")
endif()

# Mesh tools for reverse engineering (mitcad#1871): a cylinder's and a
# box's STL meshes sectioned into sketches, repaired, decimated, cut by a
# lasso, segmented and their regions fitted with surfaces.
if(BUILD_TESTING)
  set(mesh_tools "${CMAKE_CURRENT_BINARY_DIR}/mesh-tools")
  file(MAKE_DIRECTORY "${mesh_tools}")
  add_test(NAME cli.mesh_tools COMMAND mitcad-cli run "${scripts}/mesh_tools.json")
  set_tests_properties(cli.mesh_tools PROPERTIES WORKING_DIRECTORY "${mesh_tools}")
endif()

# Wrap (mitcad#473): Extrude's wrap onto a rod as an emboss, raised and
# engraved, and profiles developed onto a cone by Emboss and Extrude.
if(BUILD_TESTING)
  add_test(NAME cli.wrap COMMAND mitcad-cli run "${scripts}/wrap.json")
endif()

# Sketch Move/Copy (mitcad#1563): Point to Point from a rectangle's corner to
# a circle's center, and a constrained line turned 90 degrees about its middle
# without a pivot keeps its length.
if(BUILD_TESTING)
  add_test(NAME cli.sketch_move_point_to_point COMMAND mitcad-cli run "${scripts}/sketch_move_point_to_point.json")
endif()

# Headless MCP stdio server (mitcad#124), and the real CLI transport test.
target_sources(mitcad-cli PRIVATE mcp.cpp mcp.hpp mcp-connect.cpp mcp-connect.hpp
  "${app}/mcp/LocalEndpoint.cpp" "${app}/mcp/LocalEndpoint.hpp")
find_package(Threads REQUIRED)
target_link_libraries(mitcad-cli PRIVATE Qt6::Network Threads::Threads)
if(WIN32)
  target_sources(mitcad-cli PRIVATE "${app}/mcp/WindowsEndpointSecurity.cpp" "${app}/mcp/WindowsEndpointSecurity.hpp")
  target_link_libraries(mitcad-cli PRIVATE advapi32)
endif()
if(BUILD_TESTING)
  # model_command's isError and failed_features, and add_feature after a
  # feature, through the stdio transport (mitcad#401, mitcad#403).
  add_test(NAME cli.mcp_results
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/mcp-results"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/mcp-results-test.cmake")
  if(WIN32)
    set_property(TEST cli.mcp_results APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  # Selections by geometry and the edges of a failed fillet (mitcad#1587).
  add_test(NAME cli.mcp_selections
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/mcp-selections"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/mcp-selections-test.cmake")
  if(WIN32)
    set_property(TEST cli.mcp_selections APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  # References between the entries of a command array (mitcad#1897).
  add_test(NAME cli.mcp_array_refs
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/mcp-array-refs"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/mcp-array-refs-test.cmake")
  if(WIN32)
    set_property(TEST cli.mcp_array_refs APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  # The memory guard of model commands with a lowered limit (mitcad#1480):
  # a refused command, the document as it was, the next calls answered.
  add_test(NAME cli.mcp_memory
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/mcp-memory"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/mcp-memory-test.cmake")
  if(WIN32)
    set_property(TEST cli.mcp_memory APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  # The guard on the join of a plate and 38 flattened spheres, whose
  # memory grows by gigabytes a second (mitcad#1493): refused in time.
  add_test(NAME cli.mcp_memory_boolean
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/mcp-memory-boolean"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/mcp-memory-boolean-test.cmake")
  set_tests_properties(cli.mcp_memory_boolean PROPERTIES TIMEOUT 300)
  if(WIN32)
    set_property(TEST cli.mcp_memory_boolean APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  # The same join under a limit of 4 GiB (mitcad#1529): one valid body.
  add_test(NAME cli.mcp_boolean_flattened_spheres
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DBUILT=ON"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/mcp-boolean-flattened-spheres"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/mcp-memory-boolean-test.cmake")
  set_tests_properties(cli.mcp_boolean_flattened_spheres PROPERTIES TIMEOUT 300)
  if(WIN32)
    set_property(TEST cli.mcp_boolean_flattened_spheres APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.mcp
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/mcp-test.py"
              "$<TARGET_FILE:mitcad-cli>" "${CMAKE_CURRENT_BINARY_DIR}/mcp")
    set_tests_properties(cli.mcp PROPERTIES TIMEOUT 180
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
    if(WIN32)
      set_property(TEST cli.mcp APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endif()
    # The MCP evaluation's metric extractor (mitcad#1507, tools/mcp-eval):
    # a small design with known values saved through the headless server
    # and read back, and the agents' transcripts.
    add_test(NAME cli.mcp-eval
      COMMAND "${Python3_EXECUTABLE}" -I "${PROJECT_SOURCE_DIR}/tools/mcp-eval/test_metrics.py"
              --cli "$<TARGET_FILE:mitcad-cli>" --work "${CMAKE_CURRENT_BINARY_DIR}/mcp-eval")
    set_tests_properties(cli.mcp-eval PROPERTIES TIMEOUT 180
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
    if(WIN32)
      set_property(TEST cli.mcp-eval APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endif()
  endif()
endif()

# model_render's ray-traced picture on the headless server (mitcad#1489):
# the scene and the job of mitcad-cli render through the render worker.
target_sources(mitcad-cli PRIVATE mcp-raytrace.cpp mcp-raytrace.hpp render-scene.hpp)
if(BUILD_TESTING AND Python3_Interpreter_FOUND)
  # A picture of the asked size from the worker within its time limit, the
  # fast mode's camera, and the error codes without a worker or with one
  # that fails.
  add_test(NAME cli.mcp_raytrace
    COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/mcp-raytrace-test.py"
            "$<TARGET_FILE:mitcad-cli>" "$<TARGET_FILE:mitcad-render>" "${CMAKE_CURRENT_BINARY_DIR}/mcp-raytrace")
  # Seconds normally; a hung GPU fails it in a few minutes (mitcad#146).
  set_tests_properties(cli.mcp_raytrace PROPERTIES TIMEOUT 300
    ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
  if(WIN32)
    set_property(TEST cli.mcp_raytrace APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Drawings with views, dimensions and a hole table (mitcad#471): the design
# tests/drawing.json makes, written as SVG, PDF and DXF by export-drawing
# and read back (drawing-test.cmake).
if(BUILD_TESTING)
  set(drawing "${CMAKE_CURRENT_BINARY_DIR}/drawing")
  file(MAKE_DIRECTORY "${drawing}")
  add_test(NAME cli.drawing_design
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/drawing.json" --save "${drawing}/drawing.mitcad")
  set_tests_properties(cli.drawing_design PROPERTIES FIXTURES_SETUP cli_drawing)
  add_test(NAME cli.export_drawing
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DDESIGN=${drawing}/drawing.mitcad"
            "-DWORK=${drawing}/out" -P "${CMAKE_CURRENT_SOURCE_DIR}/drawing-test.cmake")
  set_tests_properties(cli.export_drawing PROPERTIES FIXTURES_REQUIRED cli_drawing)
  # A view's automatic centre marks only on the holes it draws, not on
  # cross holes hidden inside a bar (mitcad#1286).
  add_test(NAME cli.drawing_hidden_holes
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPT=${CMAKE_CURRENT_SOURCE_DIR}/tests/drawing_cross_holes.json"
            "-DWORK=${drawing}/hidden-holes" -P "${CMAKE_CURRENT_SOURCE_DIR}/drawing-hidden-holes-test.cmake")
  # A drawing keeps what was put into it: neither visibility in the 3D view
  # nor a new placement changes a view, a deleted one is named, an older
  # file's view is pinned to what it showed (mitcad#1293).
  add_test(NAME cli.drawing_keeps_content
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPT=${CMAKE_CURRENT_SOURCE_DIR}/tests/drawing_keeps_content.json"
            "-DWORK=${drawing}/keeps-content" -P "${CMAKE_CURRENT_SOURCE_DIR}/drawing-keeps-content-test.cmake")
  if(WIN32)
    set_property(TEST cli.drawing_design cli.export_drawing cli.drawing_hidden_holes cli.drawing_keeps_content
      APPEND PROPERTY ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A laid-out drawing of an imported assembly (mitcad#2147): every shown
# placement of a component placed four times drawn, a hidden one left out,
# and the placements counted in export-drawing's line.
if(BUILD_TESTING)
  add_test(NAME cli.drawing_assembly
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DDESIGN=${CMAKE_CURRENT_SOURCE_DIR}/tests/drawing_assembly.solidcraft.json"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/drawing/assembly" -P "${CMAKE_CURRENT_SOURCE_DIR}/drawing-assembly-test.cmake")
  if(WIN32)
    set_property(TEST cli.drawing_assembly
      APPEND PROPERTY ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Reorient Design (mitcad#502): a design with a sketch on XY, a plane offset
# from XZ, a fillet, a hole and an imported body turned by quarter turns
# about each axis and back by undo (the imported body's file is written to
# the working directory).
if(BUILD_TESTING)
  set(reorient "${CMAKE_CURRENT_BINARY_DIR}/reorient")
  file(MAKE_DIRECTORY "${reorient}")
  add_test(NAME cli.reorient_design COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/reorient_design.json")
  set_tests_properties(cli.reorient_design PROPERTIES WORKING_DIRECTORY "${reorient}")
  if(WIN32)
    # Added after the block that gives every earlier cli test Qt's DLLs.
    set_property(TEST cli.reorient_design APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  # The pieces of a split face renumbered by turns, also of a turned design
  # (mitcad#537).
  add_test(NAME cli.reorient_pieces COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/reorient_pieces.json")
  set_tests_properties(cli.reorient_pieces PROPERTIES WORKING_DIRECTORY "${reorient}")
  if(WIN32)
    set_property(TEST cli.reorient_pieces APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Modern scoped Cut tools replay in their owning component (mitcad#569).
if(BUILD_TESTING)
  set(scoped_cut_replay "${CMAKE_CURRENT_BINARY_DIR}/scoped-cut-replay.mitcad")
  add_test(NAME cli.scoped_cut_replay
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/scoped_cut_replay.json" --save "${scoped_cut_replay}")
  set_tests_properties(cli.scoped_cut_replay PROPERTIES FIXTURES_SETUP cli_scoped_cut_replay)
  add_test(NAME cli.scoped_cut_replay_reopened
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/scoped_cut_replay_reopened.json" --open "${scoped_cut_replay}")
  set_tests_properties(cli.scoped_cut_replay_reopened PROPERTIES FIXTURES_REQUIRED cli_scoped_cut_replay)
  if(WIN32)
    set_property(TEST cli.scoped_cut_replay cli.scoped_cut_replay_reopened APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Measure with axes on the OCCT kernel (mitcad#647): parallel, skew and
# meeting cylinder axes, a cone's axis against an edge, a circle against a
# construction axis, a part placed as an occurrence.
if(BUILD_TESTING)
  add_test(NAME cli.measure_axes COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/measure_axes.json")
  if(WIN32)
    set_property(TEST cli.measure_axes APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Display meshings compared face by face (mitcad#644): the holed block's
# seven faces meshed with planes by the default insertion and through
# Delabella, then the totals over that report.
if(BUILD_TESTING)
  set(mesh_compare "${CMAKE_CURRENT_BINARY_DIR}/mesh-compare")
  file(MAKE_DIRECTORY "${mesh_compare}")
  add_test(NAME cli.mesh_compare COMMAND mitcad-cli mesh-compare "${CMAKE_CURRENT_SOURCE_DIR}/tests/holed_block.json"
    --report "${mesh_compare}/holed_block.json")
  set_tests_properties(cli.mesh_compare PROPERTIES FIXTURES_SETUP cli_mesh_compare PASS_REGULAR_EXPRESSION
    "1 bodies, 7 faces \\(6 planes\\), without a triangulation a 0 b 0")
  add_test(NAME cli.mesh_compare_summary COMMAND mitcad-cli mesh-compare --summary "${mesh_compare}")
  set_tests_properties(cli.mesh_compare_summary PROPERTIES FIXTURES_REQUIRED cli_mesh_compare
    PASS_REGULAR_EXPRESSION "designs compared \\| 1 / 1 \\|.*faces triangulated \\(b\\) \\| 7 / 7 \\|.*planes with as many triangles in \\(b\\) \\| 6 / 6 \\|.*planes whose area differs between \\(a\\) and \\(b\\) \\| 0 / 6 \\|")
  # The final tier as switched (Delabella checked) against the default: the
  # holed block's planes need no second triangulation.
  add_test(NAME cli.mesh_compare_checked COMMAND mitcad-cli mesh-compare
    "${CMAKE_CURRENT_SOURCE_DIR}/tests/holed_block.json" --b checked)
  set_tests_properties(cli.mesh_compare_checked PROPERTIES PASS_REGULAR_EXPRESSION
    "1 bodies, 7 faces \\(6 planes\\), without a triangulation a 0 b 0..*planes redone a 0 b 0 \\(a default, b checked\\)")
  if(WIN32)
    set_property(TEST cli.mesh_compare_checked APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    set_property(TEST cli.mesh_compare cli.mesh_compare_summary APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  # The display meshes of the blocks whose end is rounded to a half round,
  # in one fillet and in two (mitcad#821): every face triangulated.
  add_test(NAME cli.mesh_compare_meeting_roundings COMMAND mitcad-cli mesh-compare
    "${CMAKE_CURRENT_SOURCE_DIR}/tests/fillet_meeting_roundings.json")
  set_tests_properties(cli.mesh_compare_meeting_roundings PROPERTIES PASS_REGULAR_EXPRESSION
    "2 bodies, 12 faces \\([0-9]+ planes\\), without a triangulation a 0 b 0")
  if(WIN32)
    set_property(TEST cli.mesh_compare_meeting_roundings APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Cut sheets (mitcad#853): a plate with two holes and a slot through OCCT,
# each of its parts one outer and three inner loops at 1:1.
if(BUILD_TESTING)
  add_test(NAME cli.cut_sheet COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/cut_sheet.json")
  if(WIN32)
    set_property(TEST cli.cut_sheet APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Dogbones (mitcad#1011): relief cuts at a pocket's inside corners with
# analytic volumes, diagonal and along a side, picked by the body, a face
# and edges; outside and round edges refused.
if(BUILD_TESTING)
  add_test(NAME cli.dogbone COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/dogbone.json")
  if(WIN32)
    set_property(TEST cli.dogbone APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Fillets and chamfers that take a face away (mitcad#1106): a tray whose
# walls are as high and thick as the chamfer of their inner top edges, built
# at its size without a note, then rounded where the chamfer meets the
# outer wall.
if(BUILD_TESTING)
  add_test(NAME cli.dressup_takes_face COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/dressup_takes_face.json")
  if(WIN32)
    set_property(TEST cli.dressup_takes_face APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A chamfer as wide as the face beside it (mitcad#1783): an octagon prism's
# feet on a disc whose rim runs through its corners, bevelled as wide as
# the segments of the disc's top beside them, a little wider and narrower:
# the segments taken away, the volumes the exact shapes'.
if(BUILD_TESTING)
  add_test(NAME cli.chamfer_takes_segment COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/chamfer_takes_segment.json")
  if(WIN32)
    set_property(TEST cli.chamfer_takes_segment APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Parallel planes: the main plane gap and the separate closest-point
# minimum, including a synthetic stepped shell (mitcad#1161).
if(BUILD_TESTING)
  add_test(NAME cli.measure_planar COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/measure_planar.json")
  add_test(NAME cli.measure_planar_text
    COMMAND mitcad-cli measure "${CMAKE_CURRENT_SOURCE_DIR}/tests/measure_planar.json"
            "F1.b0/F1:top" "F1.b0/F4:top")
  set_tests_properties(cli.measure_planar_text PROPERTIES PASS_REGULAR_EXPRESSION
    "Distance: 16\\.000000 mm \\(main: offset\\).*Minimum distance: 21\\.931712 mm")
  if(WIN32)
    set_property(TEST cli.measure_planar cli.measure_planar_text APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A design in inches for the lengths shown in its unit (mitcad#1165): the
# fixture of tools/ui-units-inch-test.sh and its Windows twin, its bodies'
# volumes and the 1 in between the blocks' tops.
if(BUILD_TESTING)
  add_test(NAME cli.units_inch COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/units_inch.json")
  if(WIN32)
    set_property(TEST cli.units_inch APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Emboss wrap offsets (mitcad#1259): a rectangle wrapped onto a rod moved
# along the axis by a parameter, around it and turned on the face, its
# volume kept and its centre of mass where the offsets put it; refused on
# a planar face.
if(BUILD_TESTING)
  add_test(NAME cli.emboss_wrap_offsets COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/emboss_wrap_offsets.json")
  if(WIN32)
    set_property(TEST cli.emboss_wrap_offsets APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A thread's fit to its face (mitcad#1262): M80x2 modelled on a Ø87.1 ring
# with a Ø80.1 bore warns of the size and of the 3.5 mm wall it cut
# through, the thread made all the same; on a Ø80 cup with a Ø70 bore it
# does not warn.
if(BUILD_TESTING)
  add_test(NAME cli.thread_fit COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/thread_fit.json")
  if(WIN32)
    set_property(TEST cli.thread_fit APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Envelopes (mitcad#1271): a U frame's envelope keeps its tunnel open and
# closes its 5 mm holes with few triangles and follows the frame, a hollow
# box's has no cavity, an occurrence's lies where it is placed, through the
# OCCT kernel.
if(BUILD_TESTING)
  add_test(NAME cli.envelope COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/envelope.json")
  if(WIN32)
    set_property(TEST cli.envelope APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Geometry cases reported with a false warning (mitcad#1327) under
# MITCAD_GEOMETRY_CASES: the features listed in the script build without a
# warning (a three-fillet corner's 0.057 mm on R3, a ring that runs
# smoothly onto a rounded face); skipped without the cases.
if(BUILD_TESTING)
  add_test(NAME cli.geometry_cases
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" -P "${CMAKE_CURRENT_SOURCE_DIR}/geometry-cases-test.cmake")
  set_tests_properties(cli.geometry_cases PROPERTIES SKIP_REGULAR_EXPRESSION "geometry cases: skipped")
  if(WIN32)
    set_property(TEST cli.geometry_cases APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A design holding a linked design changed after it was saved, from an
# issue report (mitcad#1416) under MITCAD_MITCAD_REPORTED: opened, it takes
# the changed file's bodies with a load warning naming the file; skipped
# without the report's files.
if(BUILD_TESTING)
  add_test(NAME cli.linked_design_update_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/linked-design-update"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/linked-design-update-test.cmake")
  set_tests_properties(cli.linked_design_update_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "linked design update: skipped" TIMEOUT 900)
  if(WIN32)
    set_property(TEST cli.linked_design_update_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A drawing's views without the tangent edges of fillets while their
# Tangent Edges is off (mitcad#1438), on the reported design of GitHub #177
# under MITCAD_MITCAD_REPORTED: its drawing written, its body projected by
# test_drawing; skipped without the report's file.
if(BUILD_TESTING)
  add_test(NAME cli.drawing_tangent_edges_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DTEST=$<TARGET_FILE:test_drawing>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/drawing/tangent-edges"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/drawing-tangent-edges-test.cmake")
  set_tests_properties(cli.drawing_tangent_edges_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "drawing tangent edges: skipped" TIMEOUT 600)
  if(WIN32)
    set_property(TEST cli.drawing_tangent_edges_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Sheet faces whose base sketch's outline was edited (mitcad#1444): a face
# with a fold on it finds its changed profile as Extrude does, warns and
# keeps its panel's slot, so the fold and its face names stay; a joined
# face likewise, and fails naming its sketch when its profile is gone. The
# reported design of GitHub #183 under MITCAD_MITCAD_REPORTED opens without
# a failed feature; skipped without the report's file.
if(BUILD_TESTING)
  add_test(NAME cli.sheet_profile_found COMMAND mitcad-cli run "${scripts}/sheet_profile_found.json")
  add_test(NAME cli.sheet_profile_found_join COMMAND mitcad-cli run "${scripts}/sheet_profile_found_join.json")
  add_test(NAME cli.sheet_profile_found_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/sheet-profile-found"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/sheet-profile-found-test.cmake")
  set_tests_properties(cli.sheet_profile_found_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "sheet profile found: skipped" TIMEOUT 600)
  if(WIN32)
    set_property(TEST cli.sheet_profile_found cli.sheet_profile_found_join cli.sheet_profile_found_reported
      APPEND PROPERTY ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Mirrors and patterns of sheet metal bodies (mitcad#1450): copies as new
# sheet bodies that take sheet tools, and a mirror joined to its original as
# one sheet; then the reported design of GitHub #188 under
# MITCAD_MITCAD_REPORTED, as saved and with its mirror joined (skipped without
# the report's file).
if(BUILD_TESTING)
  foreach(script sheet_mirror_body sheet_mirror_body_join)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/${script}.json")
  endforeach()
  add_test(NAME cli.sheet_mirror_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/sheet-mirror-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/sheet-mirror-reported-test.cmake")
  set_tests_properties(cli.sheet_mirror_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "sheet mirror reported: skipped" TIMEOUT 600)
  if(WIN32)
    foreach(test cli.sheet_mirror_body cli.sheet_mirror_body_join cli.sheet_mirror_reported)
      set_property(TEST ${test} APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endforeach()
  endif()
endif()

# The rest of the copies of sheet metal bodies (mitcad#1462): joined copies'
# domains named stably across upstream edits, sheets holding a fillet or
# chamfer overlay, scaled copies (a warning), patterns of patterns of bodies
# and combine_any.
if(BUILD_TESTING)
  foreach(script sheet_copy_stable_slots sheet_copy_overlay sheet_copy_scaled
                 sheet_copy_patterns sheet_copy_join_any)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/${script}.json")
    if(WIN32)
      set_property(TEST cli.${script} APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endif()
  endforeach()
endif()

# A sheet joined with its mirror image where a bend crosses the mirror plane
# (mitcad#1558): a folded Face mirrored with Join about its wall the bend
# crosses, and the same pair joined with Combine, one valid sheet each with
# its Flat Pattern; overlaps refused. Then the reported design of GitHub
# #209 under MITCAD_MITCAD_REPORTED (skipped without the report's file).
if(BUILD_TESTING)
  add_test(NAME cli.sheet_mirror_bend_join
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/sheet_mirror_bend_join.json")
  add_test(NAME cli.sheet_mirror_bend_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/sheet-mirror-bend-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/sheet-mirror-bend-reported-test.cmake")
  set_tests_properties(cli.sheet_mirror_bend_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "sheet mirror bend reported: skipped" TIMEOUT 600)
  if(WIN32)
    foreach(test cli.sheet_mirror_bend_join cli.sheet_mirror_bend_reported)
      set_property(TEST ${test} APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endforeach()
  endif()
endif()

# Emboss's Taper Angle (mitcad#1547): a 10 x 10 square wrapped 45 deep onto
# a rod of diameter 50, untapered, widened 10 degrees and narrowed 5, its
# top's widths and volumes; -10 degrees collapses it; on a block's top 5
# degrees as an extrusion with that taper.
if(BUILD_TESTING)
  add_test(NAME cli.emboss_taper COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/emboss_taper.json")
  if(WIN32)
    set_property(TEST cli.emboss_taper APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# 3D sketches through their JSON commands (mitcad#1522): the bent route of
# lines and tangent arcs (W11) made from the ids the commands give out,
# edited, undone and redone, saved and opened again (W8).
if(BUILD_TESTING)
  set(sketch3d_route "${CMAKE_CURRENT_BINARY_DIR}/sketch3d-route")
  file(MAKE_DIRECTORY "${sketch3d_route}")
  add_test(NAME cli.sketch3d_route
    COMMAND mitcad-cli run "${scripts}/sketch3d_route.json" --save "${sketch3d_route}/sketch3d_route.mitcad")
  set_tests_properties(cli.sketch3d_route PROPERTIES WORKING_DIRECTORY "${sketch3d_route}"
    FIXTURES_SETUP cli_sketch3d_route)
  add_test(NAME cli.sketch3d_route_reopen
    COMMAND mitcad-cli run "${scripts}/sketch3d_route_reopen.json" --open "${sketch3d_route}/sketch3d_route.mitcad")
  set_tests_properties(cli.sketch3d_route_reopen PROPERTIES FIXTURES_REQUIRED cli_sketch3d_route)
  # Linked reference geometry (mitcad#1543, W12): a box's edge middle and a
  # point on its edge follow its resize; Replace Reference, Detach and a
  # lost source.
  add_test(NAME cli.sketch3d_links COMMAND mitcad-cli run "${scripts}/sketch3d_links.json")
  # Pipe and Sweep along a line, a tangent arc and a spline of a 3D sketch
  # (mitcad#1523): their volumes, either way, after an edit of the route.
  add_test(NAME cli.sketch3d_pipe COMMAND mitcad-cli run "${scripts}/sketch3d_pipe.json")
  # The commands the drawing tools send (mitcad#1525): typed Offsets with
  # an explicit zero, Axis and Length, a free end, a Midpoint snap, a
  # Point on Curve merged into its point's undo step, Keep Plane.
  add_test(NAME cli.sketch3d_tools COMMAND mitcad-cli run "${scripts}/sketch3d_tools.json")
  if(WIN32)
    foreach(test cli.sketch3d_route cli.sketch3d_route_reopen cli.sketch3d_links
                 cli.sketch3d_pipe cli.sketch3d_tools)
      set_property(TEST ${test} APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endforeach()
  endif()
endif()
# Editing a 3D sketch (mitcad#1526): a dimension edited and removed, a
# free line's true length, Fix and Unfix, a drag and a spline's tangent
# handle dragged as the editing context sends them.
if(BUILD_TESTING)
  add_test(NAME cli.sketch3d_edit COMMAND mitcad-cli run "${scripts}/sketch3d_edit.json")
  if(WIN32)
    set_property(TEST cli.sketch3d_edit APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()
# A planar sketch turned into a 3D sketch in place (mitcad#1544): a fully
# constrained rectangle kept fully constrained with its ids and in-plane
# dimensions, undone and redone, and a Pipe's path rewritten.
if(BUILD_TESTING)
  add_test(NAME cli.sketch3d_convert COMMAND mitcad-cli run "${scripts}/sketch3d_convert.json")
  if(WIN32)
    set_property(TEST cli.sketch3d_convert APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()
# The plane-bound drawing tools of a 3D sketch (mitcad#1550): a rectangle,
# an ellipse with its minor radius solved, a polygon, a circle and a text
# on the frame planes, one undo step each.
if(BUILD_TESTING)
  add_test(NAME cli.sketch3d_shapes COMMAND mitcad-cli run "${scripts}/sketch3d_shapes.json")
  if(WIN32)
    set_property(TEST cli.sketch3d_shapes APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()
# A 3D sketch's profiles and a 3D sketch back to 2D (mitcad#1550): an
# Extrude of a converted rectangle's region that follows its width, the
# sketch turned back into 2D keeping the profile, undone.
if(BUILD_TESTING)
  add_test(NAME cli.sketch3d_to_2d COMMAND mitcad-cli run "${scripts}/sketch3d_to_2d.json")
  if(WIN32)
    set_property(TEST cli.sketch3d_to_2d APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()
# A 3D sketch's Fillet (mitcad#1548): a three-segment route rounded at both
# corners and piped, the pipe's volume; the radius edited; undone.
if(BUILD_TESTING)
  add_test(NAME cli.sketch3d_fillet COMMAND mitcad-cli run "${scripts}/sketch3d_fillet.json")
  if(WIN32)
    set_property(TEST cli.sketch3d_fillet APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()
# A 3D sketch's point on a curved face of another body (mitcad#1549): a
# line end on a cylinder's side, piped; the cylinder widened, the end and
# the pipe follow; its lost face fails the sketch.
if(BUILD_TESTING)
  add_test(NAME cli.sketch3d_faces COMMAND mitcad-cli run "${scripts}/sketch3d_faces.json")
  if(WIN32)
    set_property(TEST cli.sketch3d_faces APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()
# The 3D route workflow as the Pipe and Sweep panels write it (mitcad#1527):
# a filleted route piped by one chained pick, reversed, and swept, the
# route's listing, BC edited, undone and redone, saved and opened again.
if(BUILD_TESTING)
  set(sketch3d_workflow "${CMAKE_CURRENT_BINARY_DIR}/sketch3d-workflow")
  file(MAKE_DIRECTORY "${sketch3d_workflow}")
  add_test(NAME cli.sketch3d_workflow
    COMMAND mitcad-cli run "${scripts}/sketch3d_workflow.json" --save "${sketch3d_workflow}/sketch3d_workflow.mitcad")
  set_tests_properties(cli.sketch3d_workflow PROPERTIES WORKING_DIRECTORY "${sketch3d_workflow}"
    FIXTURES_SETUP cli_sketch3d_workflow)
  add_test(NAME cli.sketch3d_workflow_reopen
    COMMAND mitcad-cli run "${scripts}/sketch3d_workflow_reopen.json" --open "${sketch3d_workflow}/sketch3d_workflow.mitcad")
  set_tests_properties(cli.sketch3d_workflow_reopen PROPERTIES FIXTURES_REQUIRED cli_sketch3d_workflow)
  if(WIN32)
    foreach(test cli.sketch3d_workflow cli.sketch3d_workflow_reopen)
      set_property(TEST ${test} APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endforeach()
  endif()
endif()
# The other inputs that take a 3D sketch's curves (mitcad#1550): a plane and
# a point along its route, an axis through its line and a point at its
# point, a path pattern along it, all following an edit; a Pipe round its
# circle, a closed path.
if(BUILD_TESTING)
  add_test(NAME cli.sketch3d_consumers COMMAND mitcad-cli run "${scripts}/sketch3d_consumers.json")
  if(WIN32)
    set_property(TEST cli.sketch3d_consumers APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()
# Ribs, webs, thin walls and surfaces of a 3D sketch's curves in its sketch
# plane (mitcad#1550): a web and thin walls along a route's curves in the
# plane following an edit, a surface extrusion of them, a rib along the
# curve out of the plane refused naming the sketch.
if(BUILD_TESTING)
  add_test(NAME cli.sketch3d_plane_curves COMMAND mitcad-cli run "${scripts}/sketch3d_plane_curves.json")
  if(WIN32)
    set_property(TEST cli.sketch3d_plane_curves APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Circular Pattern about two axes and copies that add nothing (mitcad#1588):
# a cube's copies at every product of the turns about Z and X; a ball's rod
# patterned about X and Z, whose turns about its own axis and products on
# one place are left out; a cubic lattice of one revolve (corner balls,
# centre ball and diagonal), one pipe, one pattern about two axes and a
# fillet at all 40 joints. Then the reported cubic lattice under
# MITCAD_MITCAD_REPORTED, as saved and as one pattern of Revolve1 (with its
# centre ball) and Pipe1 with Fillet1 on all its joints (skipped without
# the report's file).
if(BUILD_TESTING)
  add_test(NAME cli.f4_pattern_two_axes
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/f4_pattern_two_axes.json")
  add_test(NAME cli.f4_pattern_lattice
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/f4_pattern_lattice.json")
  add_test(NAME cli.pattern_two_axes_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/pattern-two-axes-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/pattern-two-axes-reported-test.cmake")
  set_tests_properties(cli.f4_pattern_lattice PROPERTIES TIMEOUT 600)
  set_tests_properties(cli.pattern_two_axes_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "pattern two axes reported: skipped" TIMEOUT 600)
  if(WIN32)
    foreach(test cli.f4_pattern_two_axes cli.f4_pattern_lattice cli.pattern_two_axes_reported)
      set_property(TEST ${test} APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endforeach()
  endif()
endif()

# Auto Fix of a thread on a hole too small for it (mitcad#1632) on the
# reported design under MITCAD_MITCAD_REPORTED: the bore's radius dimension
# 72/2 becomes 73.835/2, the wall stays 3 mm, nothing newly fails (skipped
# without the report's design).
if(BUILD_TESTING)
  add_test(NAME cli.thread_fit_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/thread-fit-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/thread-fit-reported-test.cmake")
  set_tests_properties(cli.thread_fit_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "thread fit reported: skipped" TIMEOUT 900)
  if(WIN32)
    set_property(TEST cli.thread_fit_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A Flange whose wall is its profile sketch (mitcad#1648): the sketch made
# on the wall, the same volume as the rectangular flange, and a tapered
# profile's volume.
if(BUILD_TESTING)
  add_test(NAME cli.sheet_flange_profile
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/sheet_flange_profile.json")
  if(WIN32)
    # After the shared PATH block above: Qt's DLLs for mitcad-cli.
    set_property(TEST cli.sheet_flange_profile APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A contour corner where a flange's bend starts (mitcad#1765): Corner Round
# and Corner Chamfer in the developed outline, through the bend and into the
# panel's side beyond it, with the Flat Pattern's area; the edge between
# the wall's and the bend's sides picked as that corner, and an ordinary
# Fillet and Chamfer on it keeping the sheet; the edge `sheet_corner_edges`
# gives for the stored corner (mitcad#1766).
if(BUILD_TESTING)
  foreach(name sheet_corner_beside_bend sheet_corner_beside_bend_fillet)
    add_test(NAME cli.${name}
      COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/${name}.json")
    if(WIN32)
      set_property(TEST cli.${name} APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endif()
  endforeach()
endif()

# The body geometry in sketch planes (mitcad#1656): two sketches'
# plane_geometry answers, in the plane and out of it, over bodies of many
# kinds, as the listing before mitcad#1656 gave them (enclosing boxes first,
# edges by index: the same answers). The spline where two cylinders cross
# is compared by where it runs, not by the kernel's approximation, which
# differs by platform (mitcad#1669).
if(BUILD_TESTING)
  add_test(NAME cli.plane_geometry_bodies
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/plane_geometry_bodies.json")
  if(WIN32)
    # After the shared PATH block above: Qt's DLLs for mitcad-cli.
    set_property(TEST cli.plane_geometry_bodies APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Occurrence placements kept rigid (mitcad#1664) on the reported jointed
# mechanism of GitHub #253 under MITCAD_MITCAD_REPORTED (two archives,
# unpacked into the build folder): as sent first it opens with every
# placement a rotation and a translation; the design older builds' drift
# made unopenable opens with six placements straightened; saved, each opens
# again without one to straighten (skipped without the report's archives).
if(BUILD_TESTING)
  add_test(NAME cli.placements_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/placements-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/placements-reported-test.cmake")
  set_tests_properties(cli.placements_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "placements reported: skipped" TIMEOUT 900)
  if(WIN32)
    set_property(TEST cli.placements_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Automatic assembly cuts leave alone what moves against them through
# joints (mitcad#1667), on the reported jointed mechanism of GitHub #253
# under MITCAD_MITCAD_REPORTED (an archive, unpacked into the build folder):
# Joint12 and Joint13 resolve and the link bodies keep their volume and
# faces through drags (skipped without the report's archive).
if(BUILD_TESTING)
  add_test(NAME cli.assembly_context_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/assembly-context-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/assembly-context-reported-test.cmake")
  set_tests_properties(cli.assembly_context_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "assembly context reported: skipped" TIMEOUT 900)
  if(WIN32)
    set_property(TEST cli.assembly_context_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Isolate leaves the components it hides out of a sketch's body geometry
# (mitcad#1702), on the reported design of GitHub #254 under
# MITCAD_MITCAD_REPORTED (an archive, unpacked into the build folder): the
# plane_geometry answer of a sketch in the isolated component lists none of
# the other components' edges, and lists them again after Unisolate
# (skipped without the report's archive).
if(BUILD_TESTING)
  add_test(NAME cli.isolated_sketch_geometry_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/isolated-sketch-geometry-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/isolated-sketch-geometry-reported-test.cmake")
  set_tests_properties(cli.isolated_sketch_geometry_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "isolated sketch geometry reported: skipped" TIMEOUT 900)
  if(WIN32)
    set_property(TEST cli.isolated_sketch_geometry_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A flat cable laid onto a thin wire's seam at its bend, as
# app.ui-windows-flex lays it in the Floating layout (mitcad#1713): its route
# runs past the node and back, and its solid is still built.
if(BUILD_TESTING)
  add_test(NAME cli.flex_cable_onto_wire COMMAND mitcad-cli run "${scripts}/flex_cable_onto_wire.json")
  if(WIN32)
    set_property(TEST cli.flex_cable_onto_wire APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Tapers on curves the draft cannot lean (mitcad#1749): an ellipse extruded
# at +5 and -20 degrees with the volumes of its offsets (Steiner), and a
# closed spline cut through it with a taper, its wall one face; lines and a
# spline meeting at sharp corners, mitred or trimmed, with their volumes.
if(BUILD_TESTING)
  add_test(NAME cli.f1_taper_curves COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/f1_taper_curves.json")
  if(WIN32)
    set_property(TEST cli.f1_taper_curves APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Annotations rolled back with the timeline (mitcad#1756), on the owner's
# reported design under MITCAD_MITCAD_REPORTED (sketch-mode-overlays/,
# copied into the build folder): with the marker right after the edited
# sketch its material labels on later features' bodies are rolled back and
# none is a warning; at the end none is rolled back (skipped without the
# report's folder).
if(BUILD_TESTING)
  add_test(NAME cli.annotations_rolled_back_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/annotations-rolled-back-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/annotations-rolled-back-reported-test.cmake")
  set_tests_properties(cli.annotations_rolled_back_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "annotations rolled back reported: skipped" TIMEOUT 900)
  if(WIN32)
    set_property(TEST cli.annotations_rolled_back_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Form (mitcad#1757): a box cage edited into a smooth solid and written to
# STEP; a creased one rounded along its crease and drilled; a symmetric one
# extruded with its mirror image, through the OCCT kernel. Files go to the
# test's working directory.
if(BUILD_TESTING)
  set(form_work "${CMAKE_CURRENT_BINARY_DIR}/form")
  file(MAKE_DIRECTORY "${form_work}")
  add_test(NAME cli.form COMMAND mitcad-cli run "${scripts}/form.json")
  set_tests_properties(cli.form PROPERTIES WORKING_DIRECTORY "${form_work}")
  if(WIN32)
    set_property(TEST cli.form APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()
# A pipe along a helix whose turns lie closer than its section is wide
# (mitcad#1788): the default 8 mm along four turns rising 5 a turn refused
# at once, before the geometry; 2 mm built with the volume of its length.
if(BUILD_TESTING)
  add_test(NAME cli.pipe_helix_turns COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/pipe_helix_turns.json")
  set_tests_properties(cli.pipe_helix_turns PROPERTIES TIMEOUT 120)
  if(WIN32)
    set_property(TEST cli.pipe_helix_turns APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()
endif()
# A large reported .f3d design imported whole (mitcad#1802), from the
# archive of github-329/ under MITCAD_MITCAD_REPORTED with the private
# expectations beside it (unpacked into the build folder): every stored
# body placed, named, in its body folder and shown as the file has it,
# nothing suppressed in the file unsuppressed, every sketch hidden from its
# light bulb, its inch unit kept and the Y-up design stood on Z up
# (mitcad#1937) (skipped without the report's folder).
if(BUILD_TESTING)
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.f3d_reported_house
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/f3d-reported-house-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>"
              --work "${CMAKE_CURRENT_BINARY_DIR}/f3d-reported-house")
    set_tests_properties(cli.f3d_reported_house PROPERTIES TIMEOUT 3000
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1" SKIP_REGULAR_EXPRESSION "f3d reported house: skipped")
    if(WIN32)
      set_property(TEST cli.f3d_reported_house APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endif()
    # The same design imported by the application's import worker, the
    # process the Import command starts (mitcad#2085: it stopped there in
    # the application while the CLI imported it), with the same checks.
    if(TARGET mitcad)
      add_test(NAME cli.f3d_reported_house_worker
        COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/f3d-reported-house-test.py"
                --cli "$<TARGET_FILE:mitcad-cli>" --worker "$<TARGET_FILE:mitcad>"
                --work "${CMAKE_CURRENT_BINARY_DIR}/f3d-reported-house-worker")
      set_tests_properties(cli.f3d_reported_house_worker PROPERTIES TIMEOUT 3000
        ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1" SKIP_REGULAR_EXPRESSION "f3d reported house: skipped")
      # (On Windows the application in the build folder loads Qt from
      # Qt's own bin folder, as the Windows UI tests' -QtBin gives it.)
      if(WIN32)
        set_property(TEST cli.f3d_reported_house_worker APPEND PROPERTY ENVIRONMENT_MODIFICATION
          "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
      endif()
    endif()
  endif()
endif()

# Fill Cavity (mitcad#1818) on the geometry kernel: a block with a blind V
# slit along a wall's foot and a pocket, filled up to a construction plane
# with analytic volumes; seeds that pick one, the cap moved and offset, and
# a tunnel open below the cap refused in words.
if(BUILD_TESTING)
  add_test(NAME cli.fill_cavity COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/fill_cavity.json")
  if(WIN32)
    set_property(TEST cli.fill_cavity APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Sketches on cylindrical and conical faces (mitcad#1768): a dimensioned
# rectangle raised and a text sunk on a rod's side in its developed
# coordinates, the sketch following the rod's diameter, the side split by a
# line around it, the curves' lengths on the face; a rectangle raised on a
# cone's development and the cone's face split by a square.
if(BUILD_TESTING)
  add_test(NAME cli.face_sketch COMMAND mitcad-cli run "${scripts}/face_sketch.json")
  if(WIN32)
    set_property(TEST cli.face_sketch APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Round faces whatever their stored surface (mitcad#1855): Axis Through
# Cylinder, Point at Center and Cylindrical joints on the wall of a hole cut
# from a sketch circle, in a plate and in a sheet body's flange, whose walls
# are B-spline cylinders.
if(BUILD_TESTING)
  add_test(NAME cli.round_faces_recognised
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/round_faces_recognised.json")
  if(WIN32)
    set_property(TEST cli.round_faces_recognised APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Rib Extend Ends from a boss's top edge (mitcad#1857, GitHub #345): an end
# whose continuation passes over the boss's top fills up to the wall below
# the edge, with no warning (f3_rib_extend_rim: two cylinders on a base,
# analytic volumes); the reported rib on its design (skipped without the
# report's archive under MITCAD_MITCAD_REPORTED).
if(BUILD_TESTING)
  add_test(NAME cli.f3_rib_extend_rim COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/f3_rib_extend_rim.json")
  add_test(NAME cli.rib_extend_rim_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/rib-extend-rim-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/rib-extend-rim-reported-test.cmake")
  set_tests_properties(cli.rib_extend_rim_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "rib extend rim reported: skipped" TIMEOUT 900)
  if(WIN32)
    foreach(test cli.f3_rib_extend_rim cli.rib_extend_rim_reported)
      set_property(TEST ${test} APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endforeach()
  endif()
endif()

# Assembly paths (mitcad#1737): a peg slid through a hole 0.2 mm larger, a
# peg whose lug rests on the rim, a keyed shaft turned in its keyway, on
# the OCCT kernel.
if(BUILD_TESTING)
  add_test(NAME cli.assembly_path COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/assembly_path.json")
  if(WIN32)
    set_property(TEST cli.assembly_path APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A rounding as large as the side faces beside its edges (GitHub #353,
# mitcad#1819) or whose rounded corners meet (GitHub #403, mitcad#1954, and
# GitHub #536's four upright edges as one feature and as two, mitcad#2312),
# and an edge of such a body rounded or bevelled past the flat faces at its
# ends (GitHub #566, mitcad#2229), and a handle's hole corner rounded past a
# narrow stretch onto the hole's round end (GitHub #667, mitcad#2419;
# geometry.fillet_narrow_face_hole_wall): the reported designs' dressups are built
# at their size, with no warning or
# information, the side faces taken away, or refused in words (skipped
# without the report's archives under MITCAD_MITCAD_REPORTED; the synthetic
# cases are geometry.suite.dressup_takes_face's blocks and stepped block).
if(BUILD_TESTING)
  add_test(NAME cli.fillet_exact_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-exact-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-exact-reported-test.cmake")
  set_tests_properties(cli.fillet_exact_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet exact reported: skipped" TIMEOUT 900)
  if(WIN32)
    set_property(TEST cli.fillet_exact_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A chamfer whose chain ends against a shelf's end wall (mitcad#1819): the
# owner's rebuilt case (skipped without `ip65-1819-chamfer/` under
# MITCAD_MITCAD_REPORTED; the synthetic cases are
# geometry.suite.dressup_takes_face's tube with a shelf in its bore).
if(BUILD_TESTING)
  add_test(NAME cli.chamfer_end_wall_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/chamfer-end-wall-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/chamfer-end-wall-reported-test.cmake")
  set_tests_properties(cli.chamfer_end_wall_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "chamfer end wall reported: skipped" TIMEOUT 900)
  if(WIN32)
    set_property(TEST cli.chamfer_end_wall_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Extrude of a sketch on a cylinder's side with Join and Cut and their
# bodies (mitcad#1908, GitHub mitcad/Mitcad#362): a rectangle and a circle
# raised as a Join with its participant and the panel's dormant cut
# targets, sunk as a Cut with its cut_scope, thin walls refused in words.
if(BUILD_TESTING)
  add_test(NAME cli.face_sketch_operations COMMAND mitcad-cli run "${scripts}/face_sketch_operations.json")
  if(WIN32)
    set_property(TEST cli.face_sketch_operations APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# STEP files keep the design's components (mitcad#1915): a design whose
# Arm sub-assembly (placing a Pin twice) is placed twice, the second turned,
# written to STEP (cli.step_tree_export) and read back into a new design
# (cli.step_tree_import): the same tree of occurrences, both Arms one
# component, every placed copy where the design had it within 1e-6 mm.
if(BUILD_TESTING)
  set(step_tree_dir "${CMAKE_CURRENT_BINARY_DIR}/step-tree")
  file(MAKE_DIRECTORY "${step_tree_dir}")
  add_test(NAME cli.step_tree_export
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/step_tree_export.json")
  set_tests_properties(cli.step_tree_export PROPERTIES WORKING_DIRECTORY "${step_tree_dir}"
    FIXTURES_SETUP cli_step_tree)
  add_test(NAME cli.step_tree_import
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/step_tree_import.json")
  set_tests_properties(cli.step_tree_import PROPERTIES WORKING_DIRECTORY "${step_tree_dir}"
    FIXTURES_REQUIRED cli_step_tree)
  if(WIN32)
    foreach(test cli.step_tree_export cli.step_tree_import)
      set_property(TEST ${test} APPEND PROPERTY ENVIRONMENT_MODIFICATION
        "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
    endforeach()
  endif()
endif()

# A FreeCAD document's bend radius changed after the import (mitcad#1930)
# on the reported bent guide of GitHub #372 under MITCAD_MITCAD_REPORTED
# (unpacked into the build folder): the profile sketch on a coordinate
# system that the radius moves follows it, nothing that rebuilt fails, and
# the body reaches 5 mm further (skipped without the report's archive);
# with the radius 13 mm smaller every feature rebuilds and the body stays
# within its pads (mitcad#2009, OCCT port patch 1800); so too at 30, 25, 24
# and 20 mm, the volume on the line through 32 and 45 mm (mitcad#2018,
# OCCT port patches 1801 and 1802).
if(BUILD_TESTING)
  add_test(NAME cli.fcstd_bend_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fcstd-bend-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fcstd-bend-reported-test.cmake")
  set_tests_properties(cli.fcstd_bend_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fcstd bend reported: skipped" TIMEOUT 900)
  if(WIN32)
    set_property(TEST cli.fcstd_bend_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# Edits of an imported .f3d design recompute (mitcad#1904, GitHub #371):
# the reported design imported, a cut that starts on a construction plane
# parametric, and the tower's height and two other dimensions edited
# (skipped without the report's archive under MITCAD_MITCAD_REPORTED).
if(BUILD_TESTING)
  add_test(NAME cli.f3d_edit_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/f3d-edit-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/f3d-edit-reported-test.cmake")
  set_tests_properties(cli.f3d_edit_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "f3d edit reported: skipped" TIMEOUT 900)
endif()

# Offset Face on two pieces of split faces at a corner (mitcad#1947,
# GitHub #396): the reported design's pieces by -1 and -4.5 mm (skipped
# without the report's archive under MITCAD_MITCAD_REPORTED).
if(BUILD_TESTING)
  add_test(NAME cli.offset_split_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/offset-split-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/offset-split-reported-test.cmake")
  set_tests_properties(cli.offset_split_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "offset split reported: skipped" TIMEOUT 900)
endif()

# A fillet on a knife edge, where a wall and a circle's cylinder meet
# tangentially (GitHub #415, mitcad#1984): the reported sketch extruded and
# rounded at the cusp, the tip beyond the ball taken away.
if(BUILD_TESTING)
  add_test(NAME cli.fillet_knife_edge COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/fillet_knife_edge.json")
endif()

# A knife edge running on into a square edge on its line, a bite cut part of
# the way along a wall (mitcad#1988): the chain rounded, the knife edge
# first, the square edge on its result.
if(BUILD_TESTING)
  add_test(NAME cli.fillet_knife_chain COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/fillet_knife_chain.json")
endif()

# Sheet parts by material and process (mitcad#1939): process profiles and
# their allowance methods, laser sheet's finger joints and T-nut slots, a
# kerf bend's slit lattice, the flat-pack generator and board's standard
# boxes, with OCCT's volumes and flat patterns.
if(BUILD_TESTING)
  foreach(script sheet_process_allowances sheet_laser_joints sheet_kerf_bend sheet_flat_pack sheet_board_box sheet_board_polypropylene sheet_glue_flap
                 sheet_board_0203 sheet_board_0206 sheet_board_0421 sheet_board_0426 sheet_board_0427
                 sheet_board_0470 sheet_board_0471 sheet_fold_beside_port)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/${script}.json")
  endforeach()
endif()

# The reported nut design's two roundings that left pieces between their own
# blend faces (mitcad#1858), found by its content id under
# MITCAD_MITCAD_REPORTED and recomputed in a copy: both ok, no warning, each
# under 10 s (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.dressup_blend_remnants_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/dressup-blend-remnants-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/dressup-blend-remnants-reported-test.cmake")
  set_tests_properties(cli.dressup_blend_remnants_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "dressup blend remnants reported: skipped" TIMEOUT 1800)
  if(WIN32)
    set_property(TEST cli.dressup_blend_remnants_reported APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

# A crash inside OCCT's STEP reader while a STEP file is opened (mitcad#2028,
# MITCAD_TEST_OCCT_CRASH=step): the import fails with the error that the
# STEP file could not be read, OCCT's message in its detail (what
# mitcad-cli prints), not a crash.
if(BUILD_TESTING)
  if(WIN32)
    set(step_crash_error "step: ACCESS VIOLATION")
  elseif(APPLE)
    set(step_crash_error "step: SIG(SEGV|BUS)")
  else()
    set(step_crash_error "step: SIGSEGV")
  endif()
  add_test(NAME cli.occt_crash_step COMMAND mitcad-cli import "${CMAKE_CURRENT_BINARY_DIR}/holed-block.step")
  set_tests_properties(cli.occt_crash_step PROPERTIES ENVIRONMENT "MITCAD_TEST_OCCT_CRASH=step"
    FIXTURES_REQUIRED cli_step PASS_REGULAR_EXPRESSION "mitcad-cli: .*cannot read STEP file .*${step_crash_error}")
endif()

# GitHub #449's design (mitcad#2039, port patch 1830), found by its content
# id under MITCAD_MITCAD_REPORTED and opened in a copy: the concave edge
# whose ends meet a wall's side running on tangentially into two curved
# faces rounded and chamfered with 1 mm, each one valid solid without a note
# (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.fillet_tangent_seam_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-tangent-seam-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-tangent-seam-reported-test.cmake")
  set_tests_properties(cli.fillet_tangent_seam_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet tangent seam reported: skipped" TIMEOUT 900)
endif()

# The bent guide of GitHub #372 near its smallest bend radius (mitcad#2038,
# GitHub #456): at 16.8 mm through the import and set_parameter every
# feature rebuilds within the pads; at 15 mm the Fillet is refused in words
# (skipped without the report's archive under MITCAD_MITCAD_REPORTED).
if(BUILD_TESTING)
  add_test(NAME cli.fcstd_bend_limit_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fcstd-bend-limit-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fcstd-bend-limit-reported-test.cmake")
  set_tests_properties(cli.fcstd_bend_limit_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fcstd bend limit reported: skipped" TIMEOUT 1800)
endif()

# The reported nut design's Chamfer8, which left 16 pieces of the end face
# at the corners of a chain rounded more tightly than it reaches without a
# word (mitcad#1821), found by its content id under MITCAD_MITCAD_REPORTED
# and opened in a copy: its bevels meet at a mitre there, no piece left and
# no warning (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.chamfer_remnants_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/chamfer-remnants-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/chamfer-remnants-reported-test.cmake")
  set_tests_properties(cli.chamfer_remnants_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "chamfer remnants reported: skipped" TIMEOUT 1800)
endif()

# GitHub #466's Shell (mitcad#2050): the reporter's plate with a cylinder on
# a rounded corner, its foot rounded, shelled 2 mm inside with the
# cylinder's top and the plate's bottom open, builds into one solid with
# 2 mm walls (skipped without the report's archive under
# MITCAD_MITCAD_REPORTED; the synthetic shells are geometry.shell_*).
if(BUILD_TESTING)
  add_test(NAME cli.shell_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/shell-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/shell-reported-test.cmake")
  set_tests_properties(cli.shell_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "shell reported: skipped" TIMEOUT 900)
endif()

# The owner's loft from a circle to a slot (mitcad#2141, mitcad#2143),
# found by its content ids under MITCAD_MITCAD_REPORTED and opened in a
# copy: the solid loft shells 2 mm inside with the slot's end removed, the
# surface loft thickens 1 mm to each side and symmetric (skipped without the
# designs).
if(BUILD_TESTING)
  add_test(NAME cli.loft_circle_slot_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/loft-circle-slot-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/loft-circle-slot-reported-test.cmake")
  set_tests_properties(cli.loft_circle_slot_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "loft circle slot reported: skipped" TIMEOUT 900)
endif()

# Extend of a planar surface body's slanted edge (mitcad#2056, GitHub #462):
# the reported design, found by its content id under MITCAD_MITCAD_REPORTED,
# its slanted edge extended by 26 along the long edges continued, the new
# end edge parallel to it (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.extend_slanted_plane_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/extend-slanted-plane-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/extend-slanted-plane-reported-test.cmake")
  set_tests_properties(cli.extend_slanted_plane_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "extend slanted plane reported: skipped" TIMEOUT 600)
endif()

# The reported design's sketch pattern with its spacing set to a driven
# length of its own sketch (mitcad#2057, GitHub #470), found by its content
# id under MITCAD_MITCAD_REPORTED and unpacked into a folder of its own:
# accepted, the copies that far and following a new measure (skipped
# without the design).
if(BUILD_TESTING)
  add_test(NAME cli.pattern_spacing_driven_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/pattern-spacing-driven-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/pattern-spacing-driven-reported-test.cmake")
  set_tests_properties(cli.pattern_spacing_driven_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "pattern spacing driven reported: skipped" TIMEOUT 300)
endif()

# A reported sheet-metal design keeps its sheet (mitcad#2117, GitHub #497),
# found by its content id under MITCAD_MITCAD_REPORTED: its edge flanges,
# Unfold and Refold parametric and verified, its bodies the file's by
# volume, and a valid sheet with a flat pattern at the end (skipped without
# the design).
if(BUILD_TESTING)
  add_test(NAME cli.f3d_sheet_unfold_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/f3d-sheet-unfold-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/f3d-sheet-unfold-reported-test.cmake")
  set_tests_properties(cli.f3d_sheet_unfold_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "f3d sheet unfold reported: skipped" TIMEOUT 1800)
endif()

# An Extrude Cut up to a face behind its profiles (mitcad#2118) on the
# reported design of GitHub mitcad/Mitcad#492, unpacked from its report's
# zip: the cut turns round to reach the face and cuts its slots (skipped
# without the design).
if(BUILD_TESTING)
  add_test(NAME cli.extrude_cut_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/extrude-cut-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/extrude-cut-reported-test.cmake")
  set_tests_properties(cli.extrude_cut_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "extrude cut reported: skipped" TIMEOUT 600)
endif()

# Mirror and patterns of a scoped Extrude Cut beside another component's
# body it does not reach (mitcad#2126, GitHub #495), on the reported design
# found by its content id under MITCAD_MITCAD_REPORTED and unpacked into a
# work folder: both repeat the pocket in its own component (skipped without
# the design).
if(BUILD_TESTING)
  add_test(NAME cli.scoped_cut_mirror_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/scoped-cut-mirror-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/scoped-cut-mirror-reported-test.cmake")
  set_tests_properties(cli.scoped_cut_mirror_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "scoped cut mirror reported: skipped" TIMEOUT 600)
endif()

# GitHub #517's Y tube (mitcad#2140, port patch 2140), found by its content
# id under MITCAD_MITCAD_REPORTED and opened in a copy: the crotch's Fillet1
# at 6 and 50 mm and the reporter's Shell (the three ends open, 2 mm inside,
# Sharp) after each, one valid solid without a note (skipped without the
# design).
if(BUILD_TESTING)
  add_test(NAME cli.y_tube_crotch_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/y-tube-crotch-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/y-tube-crotch-reported-test.cmake")
  set_tests_properties(cli.y_tube_crotch_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "y tube crotch reported: skipped" TIMEOUT 900)
endif()

# GitHub #527's design (mitcad#2174), found by its content id under
# MITCAD_MITCAD_REPORTED and copied: export-drawing's top view as the
# reporter wrote it draws each of the six placements of its component at
# its place and prints "1 bodies in 6 placements"; a hidden occurrence is
# left out (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.drawing_placements_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/drawing-placements-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/drawing-placements-reported-test.cmake")
  set_tests_properties(cli.drawing_placements_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "drawing placements reported: skipped" TIMEOUT 300)
endif()

# GitHub #526's reduced pipe network (mitcad#2175), a format-63 .sc design
# found by its content id under MITCAD_MITCAD_REPORTED: no newer-design
# warning, its three sketches 3D sketches, one Pipe per line of each round
# tube member with the file's diameter and wall, every feature parametric
# and every body valid (skipped without the file).
if(BUILD_TESTING)
  add_test(NAME cli.solidcraft_pipe_network_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/solidcraft-pipe-network-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/solidcraft-pipe-network-reported-test.cmake")
  set_tests_properties(cli.solidcraft_pipe_network_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "solidcraft pipe network reported: skipped" TIMEOUT 600)
endif()

# An extrude edited from Join to New Component takes its fillets along
# (mitcad#2185), on the reported design found by its content id in
# extrude-new-component-fillets/ under MITCAD_MITCAD_REPORTED: the fillets
# compute in the component it makes, on its body; undone, and edited back
# to Join, as saved; New Body puts them on its body without a move
# (skipped without the design).
if(BUILD_TESTING)
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.extrude_new_component_fillets_reported
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/extrude-new-component-fillets-reported-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>"
              --work "${CMAKE_CURRENT_BINARY_DIR}/extrude-new-component-fillets-reported")
    set_tests_properties(cli.extrude_new_component_fillets_reported PROPERTIES TIMEOUT 900
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1"
      SKIP_REGULAR_EXPRESSION "extrude new component fillets reported: skipped")
  endif()
endif()

# GitHub #560's design (mitcad#2208), found by its content id under
# MITCAD_MITCAD_REPORTED and opened in a copy: the rounding wider than the
# riser beside its edge, which goes on tangentially into a half disc, takes
# the riser away and rolls on over the disc at its exact size, before the
# design's other fillet (its own order), alone and after it: one valid
# solid without a note, the
# volume exact, within the body's box (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.fillet_narrow_face_tangent_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-narrow-face-tangent-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-narrow-face-tangent-reported-test.cmake")
  set_tests_properties(cli.fillet_narrow_face_tangent_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet narrow face tangent reported: skipped" TIMEOUT 900)
endif()

# mitcad-cli export to .glb and .gltf (mitcad#2210, gltf.cpp): File >
# Export's glTF writer (app/web/GltfExport.hpp) with the web page's bodies
# (app/web/WebBodies.cpp), reading the model as the main window answers it
# (app/web/DocumentContext.hpp); with the render sources above (the
# appearances, the meshes), not the page's viewer. test_gltf_export builds
# the same sources.
set(mitcad_cli_gltf_sources
  "${app}/framework/Command.cpp" "${app}/framework/Command.hpp"
  "${app}/web/DocumentContext.hpp" "${app}/web/GltfExport.cpp" "${app}/web/GltfExport.hpp"
  "${app}/web/WebBodies.cpp" "${app}/web/WebGlb.cpp" "${app}/web/WebGlb.hpp" "${app}/web/WebPage.hpp")
target_sources(mitcad-cli PRIVATE gltf.cpp gltf.hpp ${mitcad_cli_gltf_sources})
if(BUILD_TESTING)
  # mitcad-cli's .glb and .gltf files of a component placed three times
  # checked, and compared with the application's writer called as File >
  # Export calls it, without a window (gltf-export-test.cmake).
  add_executable(test_gltf_export test-gltf-export.cpp ${mitcad_cli_gltf_sources}
    "${app}/framework/Appearances.cpp" "${app}/framework/Appearances.hpp"
    "${app}/render/RenderBatch.cpp" "${app}/render/RenderBatch.hpp"
    "${app}/render/RenderDevice.cpp" "${app}/render/RenderDevice.hpp"
    "${app}/render/RenderMesh.cpp" "${app}/render/RenderMesh.hpp"
    "${app}/render/RenderScene.cpp" "${app}/render/RenderScene.hpp")
  mitcad_utf8_code_page(test_gltf_export)
  set_target_properties(test_gltf_export PROPERTIES AUTOMOC ON)
  target_include_directories(test_gltf_export PRIVATE "${app}")
  target_include_directories(test_gltf_export SYSTEM PRIVATE "${MITCAD_CGLTF_INCLUDE_DIR}")
  target_link_libraries(test_gltf_export PRIVATE mitcad_ffi mitcad_geometry mitcad_geometry_io Qt6::Gui TKMesh
    TKFillet TKPrim TKTopAlgo TKGeomBase TKG3d TKBRep mitcad_warnings)
  if(APPLE)
    target_link_libraries(test_gltf_export PRIVATE mitcad_mac_work)
  endif()
  add_test(NAME cli.export_gltf
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DTEST=$<TARGET_FILE:test_gltf_export>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests" "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/export-gltf"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/gltf-export-test.cmake")
  set_tests_properties(cli.export_gltf PROPERTIES TIMEOUT 300)
endif()

# Cell Grid (mitcad#2238): hexagons and circles cut from a plate with
# analytic volumes, whole, snapped and trimmed edge cells, through all, a
# circle keep-out, and a fillet on a cell's edge through a change of the
# plate's size.
if(BUILD_TESTING)
  add_test(NAME cli.cell_grid COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/cell_grid.json")
endif()

# Every script's design exported as a script and replayed (mitcad#2245).
if(BUILD_TESTING)
  include("${CMAKE_CURRENT_SOURCE_DIR}/script-round-trip-tests.cmake")
endif()

# GitHub #553's design (mitcad#2193), found by its content id under
# MITCAD_MITCAD_REPORTED and opened in a copy: a lip's edge rounded with 2,
# Tangent Chain off, where it runs smoothly into a pocket wall's rounding of
# radius 2: the rounding ends against that rounding, one valid solid without
# a note (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.fillet_face_end_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-face-end-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-face-end-reported-test.cmake")
  set_tests_properties(cli.fillet_face_end_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet face end reported: skipped" TIMEOUT 600)
endif()

# The reported wave-shaped model and the other program's result of it
# (mitcad#2228), found by their content ids in wavy-fillet/ under
# MITCAD_MITCAD_REPORTED: each top edge along the waves rounded at 2.5, 3,
# 5, 8.5 and 8.7 mm, one valid solid each; the .f3d's fillet (8.7 mm)
# built, parametric with its bodies as the file's within 0.1 % with as
# many faces, or within the difference the test names (skipped without
# the files).
if(BUILD_TESTING)
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.wavy_fillet_reported
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/wavy-fillet-reported-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>"
              --work "${CMAKE_CURRENT_BINARY_DIR}/wavy-fillet-reported")
    set_tests_properties(cli.wavy_fillet_reported PROPERTIES TIMEOUT 1800
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1"
      SKIP_REGULAR_EXPRESSION "wavy fillet reported: skipped")
  endif()
endif()

# GitHub #592's design (mitcad#2291), found by its content id under
# MITCAD_MITCAD_REPORTED and opened in a copy: the top's edge rounded with 3
# past a cut-out's rim rounded with 2, the cut-out's axis in the top's
# plane: the roundings join at the rim's own ball (the port's patch 2291),
# one valid solid without a warning and no face under 0.01 mm2; with 2 for
# both as clean as before (skipped without the design; the synthetic case
# is geometry.port.2291).
if(BUILD_TESTING)
  add_test(NAME cli.fillet_tight_rim_corner_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-tight-rim-corner-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-tight-rim-corner-reported-test.cmake")
  set_tests_properties(cli.fillet_tight_rim_corner_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet tight rim corner reported: skipped" TIMEOUT 600)
endif()

# GitHub #582's design (mitcad#2272), found by its content id under
# MITCAD_MITCAD_REPORTED and opened in a copy: a nozzle's foot that crosses
# the line where a dome runs on tangentially into a tube, rounded with 1.5
# on the original nozzle (picked by either edge of its chain), on a pattern
# copy, and in the rounding the pattern repeats together with the flange's
# chamfer, every copy rounded and chamfered (mitcad#2372): one valid solid
# at the exact size without a note each (skipped without the design; the synthetic
# case is geometry.fillet_nozzle_dome_junction).
if(BUILD_TESTING)
  add_test(NAME cli.fillet_nozzle_dome_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-nozzle-dome-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-nozzle-dome-reported-test.cmake")
  set_tests_properties(cli.fillet_nozzle_dome_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet nozzle dome reported: skipped" TIMEOUT 900)
endif()

# A test's PASS_REGULAR_EXPRESSION is a list: a ';' in its text splits it
# into alternatives of which any one passes, so that the test no longer
# checks what it says (mitcad#1913). Write '.' for a ';' of the output.
if(BUILD_TESTING)
  get_property(cli_tests DIRECTORY PROPERTY TESTS)
  foreach(test IN LISTS cli_tests)
    get_test_property(${test} PASS_REGULAR_EXPRESSION pass)
    list(LENGTH pass alternatives)
    if(alternatives GREATER 1)
      message(SEND_ERROR "${test}: its PASS_REGULAR_EXPRESSION holds a ';', which makes it ${alternatives} alternatives of which any one passes: write '.' for it")
    endif()
  endforeach()
endif()

# A base view of a sheet-metal body's Flat Pattern (mitcad#2280): its blank,
# its bend lines in their two line types with their notes, dimensions on it,
# the flange and the pattern's side followed, the view kept while the
# pattern cannot be made.
if(BUILD_TESTING)
  add_test(NAME cli.drawing_flat_pattern_view
           COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/drawing_flat_pattern_view.json")
endif()

# URDF export for ROS 2 (mitcad#2100): a small mechanism exported to a
# package folder, its files and structure checked (check_urdf too where it
# is installed).
if(BUILD_TESTING)
  add_test(NAME cli.export_urdf
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPT=${CMAKE_CURRENT_SOURCE_DIR}/tests/urdf_export.json"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/urdf-export"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/urdf-export-test.cmake")
endif()

# Split Body with a face of another component's body as the Splitting Tool
# (mitcad#2318, follow-up of mitcad#2299), on GitHub #588's design found by
# its content id under MITCAD_MITCAD_REPORTED, with the real kernel: the
# body the reporter split, cut by the other component's face picked through
# its occurrence (tool_links, as the panel writes it), gives two solids of
# its volume at the face's height, and the other component stays as it was
# (skipped without the design).
if(BUILD_TESTING)
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.split_body_other_component_reported
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/split-body-other-component-reported-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>"
              --work "${CMAKE_CURRENT_BINARY_DIR}/split-body-other-component-reported")
    set_tests_properties(cli.split_body_other_component_reported PROPERTIES TIMEOUT 1200
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1"
      SKIP_REGULAR_EXPRESSION "split body other component reported: skipped")
  endif()
endif()

# GitHub #608's design (mitcad#2330, fillet-full-round-order-reported-test.cmake,
# skipped without it): the rounding wider than the riser (Fillet2) and a
# rounding of a corner of the bottom it touches (Fillet1, at 11 and as a
# Full Round) in both orders: each ok without a note, no sliver face, no lip
# of Fillet2's rounding past Fillet1's, the same volume in both orders.
if(BUILD_TESTING)
  add_test(NAME cli.fillet_full_round_order_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-full-round-order-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-full-round-order-reported-test.cmake")
  set_tests_properties(cli.fillet_full_round_order_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet full round order reported: skipped" TIMEOUT 900)
endif()

# Boundary Surface's Network whose curves run on past the outer crossing
# curves (mitcad#2333, GitHub #609): the reported network rebuilt as a
# design, found by its content id under MITCAD_MITCAD_REPORTED, builds one
# face through all five curves between the outer crossings (skipped
# without the design).
if(BUILD_TESTING)
  add_test(NAME cli.boundary_network_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/boundary-network-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/boundary-network-reported-test.cmake")
  set_tests_properties(cli.boundary_network_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "boundary network reported: skipped" TIMEOUT 600)
endif()

# Lofts along two rails (mitcad#2373, follow-up of mitcad#2320,
# loft-rails-reported-test.py): the forum's four-section bottle built by
# tests/loft_rails_four_sections.json, and GitHub #607's and #610's designs
# found by their content ids under MITCAD_MITCAD_REPORTED (skipped without
# them): each loft builds, both rails lie on it within 1e-4 mm, and the
# mirrored cases are their own mirror images in x within 1e-6 mm.
if(BUILD_TESTING)
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.loft_rails_four_sections
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/loft-rails-reported-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>" --case forum
              --work "${CMAKE_CURRENT_BINARY_DIR}/loft-rails-four-sections")
    set_tests_properties(cli.loft_rails_four_sections PROPERTIES TIMEOUT 600
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1")
    add_test(NAME cli.loft_rails_reported
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/loft-rails-reported-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>" --case reported
              --work "${CMAKE_CURRENT_BINARY_DIR}/loft-rails-reported")
    set_tests_properties(cli.loft_rails_reported PROPERTIES TIMEOUT 1200
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1"
      SKIP_REGULAR_EXPRESSION "loft rails reported: skipped")
  endif()
endif()

# Replace Face of a cylinder's round top onto a slanted planar face
# (mitcad#2382, GitHub #643): the reported design, found by its content id
# under MITCAD_MITCAD_REPORTED, two cylinder tops replaced by a slanted
# surface strip's plane, each an ellipse on it (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.replace_face_tilted_round_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/replace-face-tilted-round-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/replace-face-tilted-round-reported-test.cmake")
  set_tests_properties(cli.replace_face_tilted_round_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "replace face tilted round reported: skipped" TIMEOUT 600)
endif()

# Shells with Sharp corners on reporters' designs (GitHub #642 and #645,
# mitcad#2383, mitcad#2415), found by their content ids under
# MITCAD_MITCAD_REPORTED and opened in a copy: the revolved cone with five
# notches through its wall shells 2 mm inside, Sharp (OCCT port patch
# 2383); the disk with an arm tangent to it, its seam on the tangent line,
# 2, 1 and 5 mm, and #645's loft cut to a slot, closed, 2 mm (patch 2415),
# with its top open, 2 and 1 mm, Sharp and Rounded (patch 2462); the cone
# shelled again with the step's top at 3 mm, a wall of its own
# (mitcad#2443); skipped without the designs.
if(BUILD_TESTING)
  add_test(NAME cli.shell_sharp_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/shell-sharp-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/shell-sharp-reported-test.cmake")
  set_tests_properties(cli.shell_sharp_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "shell sharp reported: skipped" TIMEOUT 900)
endif()

# The rules' corner relief at flange corners (mitcad#2342): Flanges on two
# adjacent edges and a flange on several edges, Rectangular, Round and Tear,
# a width inside the bends' crossing zone, the version flag, a flange's own
# relief and a Corner Relief feature that overrides the rules'.
if(BUILD_TESTING)
  add_test(NAME cli.sheet_flange_corner_relief
    COMMAND mitcad-cli run "${scripts}/sheet_flange_corner_relief.json")
  add_test(NAME cli.sheet_flange_edges_corner_relief
    COMMAND mitcad-cli run "${scripts}/sheet_flange_edges_corner_relief.json")
endif()

# The three Corner Relief types at the corners of a flange on four edges,
# with the reported rules (mitcad#2411): Rectangular, Round and Tear give
# three different corners, measured from the tangent lines' inner corner.
if(BUILD_TESTING)
  add_test(NAME cli.sheet_flange_corner_relief_types
    COMMAND mitcad-cli run "${scripts}/sheet_flange_corner_relief_types.json")
endif()

# The same on the reported design of GitHub #614 under MITCAD_MITCAD_REPORTED
# (mitcad#2424): as saved, then Rectangular, Round and Tear 1 mm, three
# different corners and a valid flat pattern each; skipped without the file.
if(BUILD_TESTING)
  add_test(NAME cli.sheet_flange_corner_relief_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/sheet-flange-corner-relief-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/sheet-flange-corner-relief-reported-test.cmake")
  set_tests_properties(cli.sheet_flange_corner_relief_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "sheet flange corner relief reported: skipped" TIMEOUT 600)
endif()

# A sketch on a drafted face after the Draft's fixed plane changes
# (mitcad#2408, follow-up of mitcad#2351, GitHub #622): the reported design,
# found by its content id under MITCAD_MITCAD_REPORTED, recomputes with the
# Draft about the offset plane, its sketch on the drafted face; the sketch
# follows the face through plane edits and is redefined onto it (skipped
# without the design).
if(BUILD_TESTING)
  add_test(NAME cli.draft_neutral_plane_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/draft-neutral-plane-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/draft-neutral-plane-reported-test.cmake")
  set_tests_properties(cli.draft_neutral_plane_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "draft neutral plane reported: skipped" TIMEOUT 600)
endif()

# Convert to Sheet Metal on the reported rectangle-to-round transition
# (mitcad#2435): its loft's sides refused as bending two ways, its neck's
# split cylinder made a sheet with an exact blank (skipped without
# `github-684/` under MITCAD_MITCAD_REPORTED; the synthetic cases are
# cli.sheet_convert_curved).
if(BUILD_TESTING)
  add_test(NAME cli.sheet_convert_curved_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/sheet-convert-curved-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/sheet-convert-curved-reported-test.cmake")
  set_tests_properties(cli.sheet_convert_curved_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "sheet convert curved reported: skipped" TIMEOUT 600)
endif()

# GitHub #635's design (mitcad#2365), found by its content id under
# MITCAD_MITCAD_REPORTED and opened in a copy: the block's end edges rounded
# where a counterbore's rim comes 1 mm from each end face, each counterbore's
# wall cutting its rounding alike at both ends (OCCT port patch 2365): one
# valid solid without a note, the volumes and the faces beside the
# counterbores the mirror image's (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.fillet_counterbore_seam_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-counterbore-seam-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-counterbore-seam-reported-test.cmake")
  set_tests_properties(cli.fillet_counterbore_seam_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet counterbore seam reported: skipped" TIMEOUT 900)
endif()

# Turn Roughing along a concave R15 shoulder fillet (mitcad#2420, GitHub
# #668): the reported shaft, found by its content id under
# MITCAD_MITCAD_REPORTED, opened through mitcad-cli mcp; its turning profile,
# the roughing's arc, the program's G2 and the simulated stock against the
# CAD fillet (skipped without the design).
if(BUILD_TESTING)
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.turn_fillet_reported
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/turn-fillet-reported-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>"
              --work "${CMAKE_CURRENT_BINARY_DIR}/turn-fillet-reported")
    set_tests_properties(cli.turn_fillet_reported PROPERTIES TIMEOUT 900
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1"
      SKIP_REGULAR_EXPRESSION "turn fillet reported: skipped")
  endif()
endif()

# GitHub #577's design (mitcad#2380), found by its content id under
# MITCAD_MITCAD_REPORTED and opened in a copy: the benchmark's last step, a
# circular pattern of the nozzle's features with its chamfers and roundings,
# one valid solid without a note, every copy chamfered and rounded (skipped
# without the design; GitHub #582's design is cli.fillet_nozzle_dome_reported).
if(BUILD_TESTING)
  add_test(NAME cli.fillet_dome_pattern_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-dome-pattern-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-dome-pattern-reported-test.cmake")
  set_tests_properties(cli.fillet_dome_pattern_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet dome pattern reported: skipped" TIMEOUT 900)
  # A circular pattern of a boss and a hole through all in it (mitcad#2380):
  # each copy of the hole reaches through the boss's copy before it, also
  # where the bodies before the pattern end short of the copy.
  add_test(NAME cli.pattern_through_all_copied_boss
    COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/pattern_through_all_copied_boss.json")
endif()

# GitHub #620's design (mitcad#2352), found by its content id under
# MITCAD_MITCAD_REPORTED and opened in a copy: three tangent-chained edges
# round a pocket's rim rounded with 1.5 (the design's suppressed Fillet14),
# which 0.71.0 refused at a corner past a narrow face: one closed solid
# without a note, the rounding at 1.5, the volume pinned; Fillet15 after it
# as well (skipped without the design; the synthetic ledge case is
# geometry.fillet_ledge_rising_face).
if(BUILD_TESTING)
  add_test(NAME cli.fillet_rim_corner_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/fillet-rim-corner-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/fillet-rim-corner-reported-test.cmake")
  set_tests_properties(cli.fillet_rim_corner_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "fillet rim corner reported: skipped" TIMEOUT 900)
endif()

# A mirror of a boss extruded to the next face (mitcad#2430): Adjust builds
# the image again, ending on the lower face on its side; Identical reflects
# the finished boss, which reaches nothing there.
if(BUILD_TESTING)
  add_test(NAME cli.mirror_to_next COMMAND mitcad-cli run "${scripts}/mirror_to_next.json")
endif()

# GitHub #640's design (mitcad#2430), found by its content id under
# MITCAD_MITCAD_REPORTED and unpacked into a work folder: its mirror, saved
# without a compute option, keeps its result; edited to Adjust, the mirrored
# extrusions are built again at the image (skipped without the design).
if(BUILD_TESTING)
  add_test(NAME cli.mirror_to_next_reported
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
            "-DSCRIPTS=${CMAKE_CURRENT_SOURCE_DIR}/tests"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/mirror-to-next-reported"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/mirror-to-next-reported-test.cmake")
  set_tests_properties(cli.mirror_to_next_reported PROPERTIES
    SKIP_REGULAR_EXPRESSION "mirror to next reported: skipped" TIMEOUT 600)
endif()

# A mirror of a rib and of a web To Next (mitcad#2431): Adjust builds each
# again at the image, down to the lower face on its side, its thickness on
# the image's side; Identical reflects the finished rib, which reaches
# nothing there.
if(BUILD_TESTING)
  add_test(NAME cli.mirror_rib_to_next COMMAND mitcad-cli run "${scripts}/mirror_rib_to_next.json")
endif()

# A pattern of a mirror of a boss extruded to the next face (mitcad#2432):
# with Adjust the mirrored boss is built again at each copy and ends on the
# lower face there; a mirror with Identical gives the pattern its reflected
# finished tool, whose copy reaches nothing.
if(BUILD_TESTING)
  add_test(NAME cli.mirror_pattern_to_next
    COMMAND mitcad-cli run "${scripts}/mirror_pattern_to_next.json")
endif()

# A lip on the top edge of a tray's wall (mitcad#2445): folded in, its ends
# keep the Corner Gap short of the tray's end walls; folded out, on a long
# and a short wall, as before; the flat patterns valid; a lip that crosses
# the tray refused naming itself and the wall it hits.
if(BUILD_TESTING)
  add_test(NAME cli.sheet_flange_lip
    COMMAND mitcad-cli run "${scripts}/sheet_flange_lip.json")
endif()

# Shell with faces of their own wall thickness (mitcad#2443, GitHub #642's
# second request): a block's bottom at its own thickness Inside, Outside,
# Both, closed and Rounded inside, a cylinder's side, the thicknesses
# edited; the walls measured and the volumes exact; Rounded outside refused.
if(BUILD_TESTING)
  add_test(NAME cli.shell_own_thickness COMMAND mitcad-cli run "${CMAKE_CURRENT_SOURCE_DIR}/tests/shell_own_thickness.json")
endif()

# A reported .f3z assembly package (mitcad#2449), found by its content id
# under MITCAD_F3D_REPORTED (else the f3d-reported folder beside
# MITCAD_F3D_CORPUS) with its STEP export: the copies of its documents show
# and hide their sketches as the documents do, its bodies are where the
# STEP file has them, and no more items fall back than the fixed state
# (skipped without the files). Imported with four threads (its documents
# side by side) and again with one: both give the same design.
if(BUILD_TESTING)
  find_package(Python3 COMPONENTS Interpreter QUIET)
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.f3d_reported_package
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/f3d-reported-package-test.py"
              --cli "$<TARGET_FILE:mitcad-cli>"
              --work "${CMAKE_CURRENT_BINARY_DIR}/f3d-reported-package" --compare-serial)
    set_tests_properties(cli.f3d_reported_package PROPERTIES TIMEOUT 7200
      ENVIRONMENT "PYTHONDONTWRITEBYTECODE=1" SKIP_REGULAR_EXPRESSION "f3d reported package: skipped")
  endif()
endif()

# Every mitcad-cli test (mitcad-cli loads Qt's DLLs, mitcad-cli render): on
# Windows they find them through PATH, as app.unit does. Last in the file, so
# that tests added below the old place get it too (0.66.0-rc1: the knife edge
# tests failed with 0xc0000135, a DLL not found).
if(BUILD_TESTING AND WIN32)
  get_property(mitcad_cli_tests DIRECTORY PROPERTY TESTS)
  set_property(TEST ${mitcad_cli_tests} APPEND PROPERTY ENVIRONMENT_MODIFICATION
    "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()
