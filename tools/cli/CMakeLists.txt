# SPDX-License-Identifier: MIT
# mitcad-cli: the model and the kernel without the user interface.
add_executable(mitcad-cli main.cpp library.cpp library.hpp projects.cpp projects.hpp)
# Paths and arguments as UTF-8 on Windows too (cmake/Utf8CodePage.cmake).
mitcad_utf8_code_page(mitcad-cli)
target_link_libraries(mitcad-cli PRIVATE mitcad_ffi mitcad_geometry mitcad_geometry_io mitcad_warnings)

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

# The imports, which the import corpus tests run, apart from the other
# commands (tools/check-all.sh runs the corpus again when they change).
target_sources(mitcad-cli PRIVATE import.cpp import.hpp)
# Drawings with views, dimensions and a hole table (mitcad#471).
target_sources(mitcad-cli PRIVATE drawing.cpp drawing.hpp)
# Display meshings compared face by face (mitcad#644): the view's mesh
# options from app/view/ViewTessellation.hpp.
target_sources(mitcad-cli PRIVATE mesh_compare.cpp mesh_compare.hpp)

if(BUILD_TESTING)
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

  # Face operations (F2): the T0 models' volumes, names through edits, failures.
  foreach(script f2_fillet_chamfer f2_shell_draft f2_face_ops f2_names_follow_parameters f2_failures)
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
  foreach(script f3_sweep f3_loft f3_pipe_coil f3_rib_web f3_names_follow_parameters f3_failures)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json")
  endforeach()
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

  # Components and occurrences (F6): placements, subassemblies, timeline
  # moves, components made by features and copies, with world-space
  # volumes, boxes and centres of mass; then external components: the
  # assembly saved, inserted linked and as a copy, edited, and the link
  # following the edit when the design opens again.
  add_test(NAME cli.f6_components COMMAND mitcad-cli run "${scripts}/f6_components.json")
  # Derives: linked copies of other components' bodies (mitcad#324).
  add_test(NAME cli.f6_derive COMMAND mitcad-cli run "${scripts}/f6_derive.json")
  # The BUILD tab (mitcad#375): profiles placed on sides and ends, bound,
  # mitred and connected, from the synthetic sources of build_parts.mitcad.
  foreach(script build_frame build_connect)
    add_test(NAME cli.${script} COMMAND mitcad-cli run "${scripts}/${script}.json"
                                          --open "${scripts}/build_parts.mitcad")
  endforeach()
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
    "Warning: plate is updated from plate\\.mitcad.*Placed bodies: 8.*plate:1/Body1 \\(F1\\.b0\\): volume 8000\\.000 mm3")

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

  # Saving the result of a script, then reading it back.
  set(saved "${CMAKE_CURRENT_BINARY_DIR}/join.mitcad")
  add_test(NAME cli.save COMMAND mitcad-cli run "${scripts}/join.json" --save "${saved}")
  set_tests_properties(cli.save PROPERTIES FIXTURES_SETUP cli_saved)
  add_test(NAME cli.info_saved COMMAND mitcad-cli info "${saved}" --json)
  set_tests_properties(cli.info_saved PROPERTIES FIXTURES_REQUIRED cli_saved
    PASS_REGULAR_EXPRESSION "\"name\": \"Body1\"")

  add_test(NAME cli.failed_expectation COMMAND mitcad-cli run "${scripts}/failing.json")
  set_tests_properties(cli.failed_expectation PROPERTIES WILL_FAIL TRUE)

  # Construction geometry and analysis (F5): datums checked through queries,
  # physical properties, measurement and comparison with a STEP file.
  foreach(script construction analysis holed_block face_separation face_geometry)
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
  set_tests_properties(cli.measure PROPERTIES PASS_REGULAR_EXPRESSION
    "area 1386\\.902.*Distance: 20\\.000000 mm.*Angle: 180\\.000000 deg")
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
  add_test(NAME cli.import COMMAND mitcad-cli import "${exchange}/out.step"
                                   --save "${exchange}/imported.mitcad")
  set_tests_properties(cli.import PROPERTIES FIXTURES_REQUIRED cli_exported PASS_REGULAR_EXPRESSION
    "Bracket \\(F1\\.b0\\): volume 23946\\.350 mm3.*Bracket \\(2\\) \\(F1\\.b1\\): volume 47892\\.699 mm3")
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
    "Fillet1 +FilletFeature +parametric -> F3 \\(not verified\\): its result deviates from the file's; kept parametric at the user's choice: volume change -8\\.5841.*F3 Fillet1 \\(fillet\\): warning: Deviation from the file accepted at import.*Body1 \\(F2\\.b0\\): volume 991\\.416 mm3")
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
    "Imported 2 bodies of part\\.ipt \\(2 solids, 2 valid; 0 not built\\).*part number: MITCAD-TEST-1; material: Aluminum \\(Mitcad's aluminum\\); units: in.*Body1 \\(F1\\.b0\\): valid solid, volume 1000\\.000 mm3.*Body2 \\(F2\\.b0\\): valid solid, volume 6283\\.185 mm3")
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
  add_test(NAME ipt.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/ipt-corpus" -P "${CMAKE_CURRENT_SOURCE_DIR}/ipt-corpus.cmake")
  set_tests_properties(ipt.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "ipt corpus: skipped")
  # The .iam import (mitcad#60, stage 4): a test project of assemblies saved
  # on another machine (mitcad_ipt::testassembly): parts found relative to
  # the assembly and by name, one missing, a sub-assembly, hidden, grounded
  # and suppressed occurrences; every placement checked against the one
  # the file displays and every placed part against its stored range box. Real assemblies under
  # MITCAD_IPT_CORPUS: iam-corpus.cmake.
  add_test(NAME cli.iam_files COMMAND test_exchange --write-iam "${exchange}/iam")
  set_tests_properties(cli.iam_files PROPERTIES FIXTURES_SETUP cli_iam)
  add_test(NAME cli.import_iam
    COMMAND mitcad-cli import-iam "${exchange}/iam/top.iam" --save "${exchange}/iam/top.mitcad")
  set_tests_properties(cli.import_iam PROPERTIES FIXTURES_REQUIRED cli_iam FIXTURES_SETUP cli_iam_saved
    PASS_REGULAR_EXPRESSION
    "Imported assembly top\\.iam: 7 occurrences placed \\(1 suppressed, 1 hidden, 1 of missing files, 0 not placed\\); 4 part files \\(3 imported, 1 missing, 0 failed; 3 identities confirmed, 0 differ\\), 1 sub-assemblies; units: mm.*files found: 1 by name, 1 missing, 3 relative to the assembly.*check: 7 of 7 placements agree with the displayed ones.*check: 5 of 5 parts. bodies lie within their stored range boxes")
  add_test(NAME cli.info_iam COMMAND mitcad-cli info "${exchange}/iam/top.mitcad")
  set_tests_properties(cli.info_iam PROPERTIES FIXTURES_REQUIRED cli_iam_saved
    PASS_REGULAR_EXPRESSION "F1 Base1 \\(base\\): ok.*Bodies: 5")
  # With --history, each part's design replayed: the cube's sketch and
  # extrusion.
  add_test(NAME cli.import_iam_history COMMAND mitcad-cli import-iam "${exchange}/iam/top.iam" --history)
  set_tests_properties(cli.import_iam_history PROPERTIES FIXTURES_REQUIRED cli_iam PASS_REGULAR_EXPRESSION
    "part [^\n]*cube\\.ipt [^\n]*imported: 1 bodies \\(1 valid, 0 not built\\); features 2 parametric")
  # The parts' outcomes with --history (mitcad#507): the three part files
  # counted before the first is imported and traced (part n of 3), the cube
  # replayed, the pair (twice) as its stored bodies, having no design; and
  # with the cube's replay made to fail, the cube as its stored bodies too.
  add_test(NAME cli.import_iam_history_parts COMMAND mitcad-cli import-iam "${exchange}/iam/top.iam" --history)
  set_tests_properties(cli.import_iam_history_parts PROPERTIES FIXTURES_REQUIRED cli_iam
    ENVIRONMENT "MITCAD_IMPORT_TRACE=1" PASS_REGULAR_EXPRESSION
    "part [123] of 3: cube\\.ipt: replayed in [0-9.]+ s.*parts with their design history: 1 replayed, 2 as their stored bodies.*cube\\.ipt [^\n]*features 2 parametric; replayed: 2 items, [0-9]+ verified")
  add_test(NAME cli.import_iam_history_fallback COMMAND mitcad-cli import-iam "${exchange}/iam/top.iam" --history)
  set_tests_properties(cli.import_iam_history_fallback PROPERTIES FIXTURES_REQUIRED cli_iam
    ENVIRONMENT "MITCAD_IMPORT_TRACE=1;MITCAD_TEST_IAM_REPLAY_FAIL=cube.ipt" PASS_REGULAR_EXPRESSION
    "part [123] of 3: cube\\.ipt: bodies in [0-9.]+ s.*parts with their design history: 0 replayed, 3 as their stored bodies.*cube\\.ipt [^\n]*imported: 1 bodies \\(1 valid, 0 not built\\); bodies only: the replay failed: MITCAD_TEST_IAM_REPLAY_FAIL names it")
  add_test(NAME cli.import_ipt_iam COMMAND mitcad-cli import-ipt "${exchange}/iam/sub/sub.iam")
  set_tests_properties(cli.import_ipt_iam PROPERTIES FIXTURES_REQUIRED cli_iam PASS_REGULAR_EXPRESSION
    "Imported assembly sub\\.iam: 2 occurrences placed.*1 part files \\(1 imported, 0 missing.*check: 2 of 2 placements agree.*check: 2 of 2 parts. bodies lie within their stored range boxes")
  add_test(NAME iam.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/iam-corpus" -P "${CMAKE_CURRENT_SOURCE_DIR}/iam-corpus.cmake")
  set_tests_properties(iam.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "iam corpus: skipped")
  # Reorient Design on the test files' bench (c71542a1) and drawer
  # (1dc60c2a) assemblies, found by content id under MITCAD_IPT_CORPUS
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
    "Built 6 elements as features \\(2 floor, 4 wall\\), checked against the geometry the file keeps of them \\(else their stored boxes\\); of those kinds none came in as meshes or boxes.*4 level planes, 2 grid axes, 0 grid arcs, 2 appearances.*link linked\\.rvt: linked\\.rvt, 4 elements")
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
      "Imported test-${version}\\.dwg \\(dwg ${version} \\(AC[0-9]+\\)\\): 6 bodies \\(3 solids, 3 meshes; 6 valid\\), 2 components.*Component Segment: 1 bodies, 2 occurrences \\(2 placements\\).*Component Brace: 1 bodies, 8 occurrences \\(10 placements\\).*Left out: 1 LINE.*Checks: 11 of 11 pass")
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
    "Imported empty-solid\\.dwg \\(dwg R2018 \\(AC1032\\)\\): 6 bodies \\(3 solids, 3 meshes; 6 valid\\), 2 components.*Not built: 3DSOLID 64: its B-rep record holds no body with faces.*Check every body built: FAIL")
  add_test(NAME dwg.corpus
    COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>" "-DRUNNER=$<TARGET_FILE:mitcad_run_parallel>"
            "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/dwg-corpus"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/dwg-corpus.cmake")
  set_tests_properties(dwg.corpus PROPERTIES SKIP_REGULAR_EXPRESSION "dwg corpus: skipped")

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
  # content ids under MITCAD_F3D_CORPUS and MITCAD_F3D_BACKUP: the items
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
  set_tests_properties(cli.perf_holes_mesh PROPERTIES FIXTURES_REQUIRED cli_perf_holes
    TIMEOUT ${perf_holes_mesh_limit} RUN_SERIAL TRUE
    PASS_REGULAR_EXPRESSION "1 bodies, 1257 faces \\(42 planes\\), without a triangulation a 0 b 0;.*planes redone a 0 b 0 \\(a checked, b checked\\)")
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
    PASS_REGULAR_EXPRESSION "restored from store: 0, evaluated: 12; stored: 8 .*${volume}")
  add_test(NAME cli.store_hit COMMAND mitcad-cli info "${perf}/perf_model.mitcad"
                                      --result-store "${store}" --persist-min-ms 0)
  set_tests_properties(cli.store_hit PROPERTIES FIXTURES_REQUIRED cli_store
    PASS_REGULAR_EXPRESSION "restored from store: 8, evaluated: 4; stored: 0 .*F12 Fillet2 \\(fillet\\): ok.*${volume}")
  add_test(NAME cli.store_edit_file COMMAND mitcad-cli run "${scripts}/store_edit.json"
                                            --open "${perf}/perf_model.mitcad" --save "${perf}/perf_edit.mitcad")
  set_tests_properties(cli.store_edit_file PROPERTIES FIXTURES_REQUIRED cli_perf FIXTURES_SETUP cli_store_edited)
  add_test(NAME cli.store_edit COMMAND mitcad-cli info "${perf}/perf_edit.mitcad"
                                       --result-store "${store}" --persist-min-ms 0)
  set_tests_properties(cli.store_edit PROPERTIES FIXTURES_REQUIRED "cli_store;cli_store_edited"
    PASS_REGULAR_EXPRESSION "restored from store: 7, evaluated: 5; stored: 1 .*F12 Fillet2 \\(fillet\\): ok")
  # The caches' diagnostics (the cache query): where the results came from,
  # the process's memory and the store's files.
  add_test(NAME cli.store_diagnostics COMMAND mitcad-cli info "${perf}/perf_model.mitcad"
                                              --result-store "${store}" --diagnostics)
  set_tests_properties(cli.store_diagnostics PROPERTIES FIXTURES_REQUIRED cli_store
    PASS_REGULAR_EXPRESSION
    "Cache: {\"last_recompute\":{\"cached\":0,.*\"restored\":8,\"restored_packed\":[0-9]+},\"memory\":{.*\"process\":{\"heap\":[0-9]+,\"peak_resident\":[0-9]+,\"resident\":[1-9][0-9]*},\"store\":{.*\"disk\":{.*\"by_type\":\\[{.*\"type\":\"(rectangular|circular)_pattern\"")

  # Small UI gaps (P9): a full round and a chamfer as wide as a rib succeed
  # with a warning.
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
  endif()
  # The fastener generator's dimension tables (mitcad#900): sizes, pitches,
  # length series, each standard's rows and the earlier versions' rows;
  # needs only Python.
  if(Python3_Interpreter_FOUND)
    add_test(NAME cli.fastener-tables
      COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/fastener-tables-test.py"
              --libraries "${PROJECT_SOURCE_DIR}/tools/libraries")
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
  # A pattern of a straight sheet cut with many copies, cut at once (mitcad#288).
  add_test(NAME cli.sheet_pattern_holes COMMAND mitcad-cli run "${scripts}/sheet_pattern_holes.json")
  # Contour Flange, Convert to Sheet Metal and Perforate (mitcad#461).
  foreach(sheet_case sheet_contour sheet_perforate sheet_convert)
    set(sheet_file "${CMAKE_CURRENT_BINARY_DIR}/${sheet_case}.mitcad")
    add_test(NAME cli.${sheet_case}
      COMMAND mitcad-cli run "${scripts}/${sheet_case}.json" --save "${sheet_file}")
    set_tests_properties(cli.${sheet_case} PROPERTIES FIXTURES_SETUP cli_${sheet_case})
    add_test(NAME cli.${sheet_case}_reopen
      COMMAND mitcad-cli run "${scripts}/${sheet_case}_reopen.json" --open "${sheet_file}")
    set_tests_properties(cli.${sheet_case}_reopen PROPERTIES FIXTURES_REQUIRED cli_${sheet_case})
  endforeach()
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

# Wrap (mitcad#473): Extrude's wrap onto a rod as an emboss, raised and
# engraved, and profiles developed onto a cone by Emboss and Extrude.
if(BUILD_TESTING)
  add_test(NAME cli.wrap COMMAND mitcad-cli run "${scripts}/wrap.json")
endif()

# mitcad-cli loads Qt's DLLs (mitcad-cli render): on Windows its tests find
# them through PATH, as app.unit does.
if(BUILD_TESTING AND WIN32)
  get_property(mitcad_cli_tests DIRECTORY PROPERTY TESTS)
  set_property(TEST ${mitcad_cli_tests} APPEND PROPERTY ENVIRONMENT_MODIFICATION
    "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
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
  if(WIN32)
    set_property(TEST cli.drawing_design cli.export_drawing APPEND PROPERTY ENVIRONMENT_MODIFICATION
      "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
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
    "1 bodies, 7 faces \\(6 planes\\), without a triangulation a 0 b 0;.*planes redone a 0 b 0 \\(a default, b checked\\)")
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
    "2 bodies, 14 faces \\([0-9]+ planes\\), without a triangulation a 0 b 0")
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
