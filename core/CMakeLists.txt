# SPDX-License-Identifier: MIT
# The bridge, the release tool mitcad-release of the automatic updates
# (mitcad#9, docs/updates.md) and the test runner mitcad-testrun
# (tools/testrun, mitcad#790; with the build's programs: build\dev\ on
# Windows, build/dev/core/ on Linux and macOS).
corrosion_import_crate(MANIFEST_PATH Cargo.toml CRATES mitcad-ffi mitcad-update mitcad-testrun)
if(MITCAD_RUST_CACHE_ENVIRONMENT)
  corrosion_set_env_vars(mitcad_ffi ${MITCAD_RUST_CACHE_ENVIRONMENT})
  corrosion_set_env_vars(mitcad-release ${MITCAD_RUST_CACHE_ENVIRONMENT})
  corrosion_set_env_vars(mitcad-testrun ${MITCAD_RUST_CACHE_ENVIRONMENT})
endif()
# Live updates (mitcad#89): TLS in the bridge (rustls with ring) takes
# random numbers from the system (bcrypt on Windows) and reads the system's
# root certificates (the Security framework on macOS). Corrosion links what
# Rust's standard library needs, not what crates add.
if(WIN32)
  target_link_libraries(mitcad_ffi INTERFACE bcrypt)
elseif(APPLE)
  target_link_libraries(mitcad_ffi INTERFACE "-framework Security" "-framework CoreFoundation")
endif()

# C++ side of the bridges: the generated mitcad_bridge/*.h and the adapters
# in cpp/bridge/, one per kernel family. Rust calls the geometry library
# through them, so the target links mitcad_geometry.
# The bridges are the rlib mitcad-ffi-bridges in ffi/, linked into the
# static library mitcad_ffi (ffi-staticlib/, mitcad#792). Corrosion reads
# the FILES from the src/ folder of the manifest that the CRATE target
# names, and imports only the static library, so this target names the
# rlib's manifest; the links below are the ones Corrosion makes for an
# imported CRATE.
add_library(mitcad_ffi_bridges_sources INTERFACE)
set_target_properties(mitcad_ffi_bridges_sources PROPERTIES
  INTERFACE_COR_PACKAGE_MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/ffi/Cargo.toml")
corrosion_add_cxxbridge(mitcad_bridge CRATE mitcad_ffi_bridges_sources FILES
  lib.rs
  kernel/boolean.rs
  kernel/dressup.rs
  kernel/exchange.rs
  kernel/extrude.rs
  kernel/profile.rs
  kernel/query.rs
  kernel/shape.rs
  kernel/transform.rs
  brep_import.rs
  kernel/analysis.rs
  kernel/datum.rs
  # Face operations (F2).
  kernel/faceops.rs
  # Profile features (F1).
  kernel/profile_feature.rs
  # Sketch references: face planes and projected curves (S1).
  kernel/sketch_ref.rs
  # Sweeps, lofts, pipes, coils, ribs and webs (F3).
  kernel/sweeps.rs
  # Sketch text (P3).
  kernel/text.rs
  # The result store (P7d).
  kernel/persist.rs
  # Cancellation inside an operation (P7e).
  kernel/cancel.rs
  # Automatic updates (mitcad#9).
  update.rs
  # Triangle meshes for 3D printing (3MF export, mitcad#13).
  kernel/mesh.rs
  # The process's memory against its limits (mitcad#80).
  memory.rs
  # Shaded meshes of faces (USDZ export, mitcad#123).
  kernel/face_mesh.rs
  # Sheet material construction (mitcad#206).
  kernel/sheet.rs
  # The faces a run of features made, copied by a pattern.
  kernel/face_copy.rs
  # The tangent chains of a body's edges and faces, for the hover (mitcad#290).
  kernel/tangent_chain.rs
  # Contacts of a new Extrude's tool (mitcad#289).
  kernel/contact.rs
  # BUILD: the face names of derived parts (mitcad#375).
  kernel/build.rs
  # Contact sets: bodies that stop each other while joints move (mitcad#454).
  kernel/motion_contact.rs
  # Emboss: profiles wrapped onto cylinders (mitcad#458).
  kernel/emboss.rs
  # Thin extrusions along chains of sketch curves (mitcad#463).
  kernel/extrude_curves.rs
  # Boundary Fill: the cells tool bodies enclose (mitcad#457).
  kernel/boundary_fill.rs
  # Patch, Stitch and Thicken: surface bodies filled, sewn and made solid (mitcad#456).
  kernel/surfaces.rs
  # Surface modelling: surface bodies and their features (mitcad#455).
  kernel/surface_sweeps.rs
  # Surface modelling: Offset Surface, Ruled Surface and Extend (mitcad#455).
  kernel/surface_offsets.rs
  # Surface modelling: Trim, Untrim, Unstitch and Reverse Normal (mitcad#455).
  kernel/surface_edits.rs
  # Plastic part features: the bands of lips and grooves (mitcad#459).
  kernel/plastic.rs
  # Drawings: views with hidden lines removed and round holes (mitcad#471).
  kernel/drawing.rs
  # Gears: toothed profiles swept along their axis or ruled to an apex (mitcad#472).
  kernel/gear.rs
  # Surface modelling: Blend, Boundary, Match and Fair Surface (mitcad#455).
  kernel/surface_fills.rs
  # Faces moved by a rigid motion (mitcad#343).
  kernel/move_faces.rs
  # A planar face of a body extruded as a profile (mitcad#535).
  kernel/face_extrude.rs
  # A planar face of a body revolved as a profile (mitcad#539).
  kernel/face_revolve.rs
  # Display meshes kept in the result store (mitcad#858).
  kernel/display_mesh.rs
  # C++ exceptions under bridge calls as errors, tested (mitcad#937).
  kernel/exception_probe.rs
  # A Fillet's or Chamfer's picked edges as polylines (mitcad#957).
  kernel/input_edges.rs
  # Dogbones: the relief cylinders at a body's inside corners (mitcad#1011).
  kernel/dogbone.rs)
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/boolean.cpp
  cpp/bridge/boolean.hpp
  cpp/bridge/dressup.cpp
  cpp/bridge/dressup.hpp
  cpp/bridge/exchange.cpp
  cpp/bridge/exchange.hpp
  cpp/bridge/extrude.cpp
  cpp/bridge/extrude.hpp
  cpp/bridge/profile.cpp
  cpp/bridge/profile.hpp
  cpp/bridge/query.cpp
  cpp/bridge/query.hpp
  cpp/bridge/shape.hpp
  cpp/bridge/analysis.cpp
  cpp/bridge/analysis.hpp
  cpp/bridge/datum.cpp
  cpp/bridge/datum.hpp
  # Face operations (F2).
  cpp/bridge/faceops.cpp
  cpp/bridge/faceops.hpp)
# Transforms, patterns, mirrors, combine and primitives (F4).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/transform.cpp
  cpp/bridge/transform.hpp)
# Profile features (F1).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/profile_feature.cpp
  cpp/bridge/profile_feature.hpp)
# Sketch planes and projected geometry (S1).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/sketch_ref.cpp
  cpp/bridge/sketch_ref.hpp)
# Sweeps, lofts, pipes, coils, ribs and webs (F3).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/sweeps.cpp
  cpp/bridge/sweeps.hpp)
# Sketch text (P3).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/text.cpp
  cpp/bridge/text.hpp)
# The result store (P7d).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/persist.cpp
  cpp/bridge/persist.hpp)
# Cancellation inside an operation (P7e).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/cancel.cpp
  cpp/bridge/cancel.hpp)
# Triangle meshes for 3D printing (3MF export, mitcad#13).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/mesh.cpp
  cpp/bridge/mesh.hpp)
# The process's memory against its limits (mitcad#80).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/memory.cpp
  cpp/bridge/memory.hpp)
# Shaded meshes of faces (USDZ export, mitcad#123).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/face_mesh.cpp
  cpp/bridge/face_mesh.hpp)
# Sheet material construction (mitcad#206).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/sheet.cpp
  cpp/bridge/sheet.hpp)
# The faces a run of features made, copied by a pattern.
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/face_copy.cpp
  cpp/bridge/face_copy.hpp)
# The tangent chains of a body's edges and faces, for the hover (mitcad#290).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/tangent_chain.cpp
  cpp/bridge/tangent_chain.hpp)
# Contacts of a new Extrude's tool (mitcad#289).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/contact.cpp
  cpp/bridge/contact.hpp)
# BUILD: the face names of derived parts (mitcad#375).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/build.cpp
  cpp/bridge/build.hpp)
# Contact sets: bodies that stop each other while joints move (mitcad#454).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/motion_contact.cpp
  cpp/bridge/motion_contact.hpp)
# Emboss: profiles wrapped onto cylinders (mitcad#458).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/emboss.cpp
  cpp/bridge/emboss.hpp)
# Thin extrusions along chains of sketch curves (mitcad#463).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/extrude_curves.cpp
  cpp/bridge/extrude_curves.hpp)
# Boundary Fill: the cells tool bodies enclose (mitcad#457).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/boundary_fill.cpp
  cpp/bridge/boundary_fill.hpp)
# Patch, Stitch and Thicken: surface bodies filled, sewn and made solid (mitcad#456).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/surfaces.cpp
  cpp/bridge/surfaces.hpp)
# Surface modelling: surface bodies and their features (mitcad#455).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/surface_sweeps.cpp
  cpp/bridge/surface_sweeps.hpp)
# Surface modelling: Offset Surface, Ruled Surface and Extend (mitcad#455).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/surface_offsets.cpp
  cpp/bridge/surface_offsets.hpp)
# Surface modelling: Trim, Untrim, Unstitch and Reverse Normal (mitcad#455).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/surface_edits.cpp
  cpp/bridge/surface_edits.hpp)
# Plastic part features: the bands of lips and grooves (mitcad#459).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/plastic.cpp
  cpp/bridge/plastic.hpp)
# Drawings: views with hidden lines removed and round holes (mitcad#471).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/drawing.cpp
  cpp/bridge/drawing.hpp)

# Gears: toothed profiles swept along their axis or ruled to an apex (mitcad#472).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/gear.cpp
  cpp/bridge/gear.hpp)

# Surface modelling: Blend, Boundary, Match and Fair Surface (mitcad#455).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/surface_fills.cpp
  cpp/bridge/surface_fills.hpp)

# Faces moved by a rigid motion (mitcad#343).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/move_faces.cpp
  cpp/bridge/move_faces.hpp)

# A planar face of a body extruded as a profile (mitcad#535).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/face_extrude.cpp
  cpp/bridge/face_extrude.hpp)

# A planar face of a body revolved as a profile (mitcad#539).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/face_revolve.cpp
  cpp/bridge/face_revolve.hpp)
# Display meshes kept in the result store (mitcad#858).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/display_mesh.cpp
  cpp/bridge/display_mesh.hpp)
# C++ exceptions under bridge calls as errors: the trycatch every bridge
# includes, and the test's probe (mitcad#937).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/trycatch.hpp
  cpp/bridge/exception_probe.cpp
  cpp/bridge/exception_probe.hpp)
# A Fillet's or Chamfer's picked edges as polylines, kept with its result
# (mitcad#957).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/input_edges.cpp
  cpp/bridge/input_edges.hpp)
# Dogbones: the relief cylinders at a body's inside corners (mitcad#1011).
target_sources(mitcad_bridge PRIVATE
  cpp/bridge/dogbone.cpp
  cpp/bridge/dogbone.hpp)
target_include_directories(mitcad_bridge PUBLIC cpp)
# What corrosion_add_cxxbridge links for a CRATE that Corrosion imported
# (mitcad#792): the bridge with the Rust library, both ways.
target_link_libraries(mitcad_bridge PRIVATE mitcad_ffi-static)
target_link_libraries(mitcad_ffi-static INTERFACE mitcad_bridge)
target_link_libraries(mitcad_bridge PUBLIC mitcad_geometry)
# The bridge and the Rust library call each other, and a single-pass
# linker takes from each archive only what the archives before it asked
# for: list them three times instead of twice (an optimised Rust build
# pulls in bridge code only on the second round).
set_property(TARGET mitcad_bridge PROPERTY LINK_INTERFACE_MULTIPLICITY 3)
# Analysis and comparison with STEP files (kernel/analysis.rs).
target_link_libraries(mitcad_bridge PRIVATE mitcad_geometry_analysis)
# Import and export of files (kernel/exchange.rs).
target_link_libraries(mitcad_bridge PRIVATE mitcad_geometry_io)

if(BUILD_TESTING)
  add_executable(test_bridge tests/test_bridge.cpp)
  target_link_libraries(test_bridge PRIVATE mitcad_ffi mitcad_geometry mitcad_geometry_analysis
                                            mitcad_warnings)
  add_test(NAME core.bridge COMMAND test_bridge)

  # .f3d body import: Rust reads and converts, C++ builds with OCCT. The
  # corpus run reads MITCAD_F3D_CORPUS (default ~/f3d-corpus) and is skipped
  # when the corpus is missing. It builds every 10th body to stay quick in
  # debug builds; core/f3d/CORPUS_REPORT.md describes the full run.
  # The corpus runs import their files in child processes, several at once
  # (mitcad#70, tests/parallel_runs.hpp); mitcad_run_parallel runs a list of
  # commands that way (the FreeCAD corpus, tools/cli/fcstd-corpus.cmake).
  add_library(mitcad_parallel_runs STATIC tests/parallel_runs.cpp tests/parallel_runs.hpp)
  target_include_directories(mitcad_parallel_runs PUBLIC tests)
  target_link_libraries(mitcad_parallel_runs PRIVATE mitcad_warnings)
  find_package(Threads REQUIRED)
  target_link_libraries(mitcad_parallel_runs PUBLIC Threads::Threads)
  add_executable(mitcad_run_parallel tests/run_parallel.cpp)
  target_link_libraries(mitcad_run_parallel PRIVATE mitcad_parallel_runs mitcad_warnings)
  add_test(NAME core.parallel_runs COMMAND mitcad_run_parallel --self-test)

  add_executable(test_brep_import tests/test_brep_import.cpp)
  target_link_libraries(test_brep_import PRIVATE mitcad_ffi mitcad_geometry mitcad_parallel_runs mitcad_warnings)
  add_test(NAME core.brep_import COMMAND test_brep_import)
  add_test(NAME f3d.corpus_occt COMMAND test_brep_import --corpus --every 10 --min-valid 99)
  set_tests_properties(f3d.corpus_occt PROPERTIES SKIP_RETURN_CODE 77)
  # The display meshes of the newer documents (core/f3d/OGS_FORMAT.md)
  # against the built bodies: each must match a body with as many faces and
  # the same bounding box, and their volumes must agree within 1 %.
  add_test(NAME f3d.corpus_meshes COMMAND test_brep_import --corpus --meshes 1)
  set_tests_properties(f3d.corpus_meshes PROPERTIES SKIP_RETURN_CODE 77)

  # Base features, import and export with OCCT (files go to the build
  # directory). The corpus run imports the .f3d files bodies only, as
  # mitcad-cli import-f3d does; it is skipped when the corpus is missing.
  # Every 10th file keeps it near 20 s in debug builds (all files: about
  # 6 min, building the bodies takes most of it).
  add_executable(test_exchange tests/test_exchange.cpp)
  target_link_libraries(test_exchange PRIVATE mitcad_ffi mitcad_geometry mitcad_geometry_io mitcad_parallel_runs mitcad_warnings)
  add_test(NAME core.exchange COMMAND test_exchange "${CMAKE_CURRENT_BINARY_DIR}/exchange-files")
  add_test(NAME f3d.corpus_import COMMAND test_exchange --corpus --every 10 --min-valid 97)
  set_tests_properties(f3d.corpus_import PROPERTIES SKIP_RETURN_CODE 77)

  # .f3d import with the timeline (T1). The corpus run imports every
  # design under MITCAD_F3D_CORPUS (default ~/f3d-corpus) and prints the
  # coverage; it is skipped when the corpus is missing. Debug builds take
  # every 4th file up to 30 timeline items; core/import/README.md has the
  # full run (release build).
  add_executable(test_f3d_import tests/test_f3d_import.cpp)
  target_link_libraries(test_f3d_import PRIVATE mitcad_ffi mitcad_geometry mitcad_geometry_io mitcad_parallel_runs mitcad_warnings)
  add_test(NAME core.f3d_import COMMAND test_f3d_import "${CMAKE_CURRENT_BINARY_DIR}/f3d-import-files")
  add_test(NAME f3d.corpus_timeline COMMAND test_f3d_import --corpus --every 4 --max-items 30
                                                            --time-limit 60)
  set_tests_properties(f3d.corpus_timeline PROPERTIES SKIP_RETURN_CODE 77)
  # The reference lofts and sweeps (route A of T0b): reference models kept
  # outside the repository under MITCAD_F3D_MODELS (default ~/f3d-models;
  # each an .f3d and its dump), replayed and compared with the volumes of
  # the bodies stored in the files, and imported from the files' own
  # streams (mitcad#34); skipped without them.
  add_test(NAME f3d.models_loft COMMAND test_f3d_import --models)
  # Labelled corpus: the test runner hands it its models' times and plans
  # its children as the corpus tests' (tools/testrun, mitcad#832).
  set_tests_properties(f3d.models_loft PROPERTIES SKIP_RETURN_CODE 77 LABELS corpus)

  # Cargo runs with the toolchain Corrosion builds with, as the build does.
  # Without RUSTUP_TOOLCHAIN this cargo, which is not a rustup proxy, calls
  # rustc through the proxy in each dependency's source directory, and
  # rustup then follows the rust-toolchain.toml that crates such as quote and
  # proc-macro2 ship: it installs rust-src there, concurrently from parallel
  # compiles, which fails the test.
  set(cargo_test_environment "RUSTC=${Rust_COMPILER_CACHED}")
  if(Rust_TOOLCHAIN)
    list(APPEND cargo_test_environment "RUSTUP_TOOLCHAIN=${Rust_TOOLCHAIN}")
  endif()
  # Incremental compilation of the debug test builds (below).
  option(MITCAD_CARGO_TEST_INCREMENTAL "Compile the crates' debug test builds incrementally" ON)
  # cargo's folder of each profile's test builds: cargo-test in the build
  # folder, or the profile's own under MITCAD_CARGO_TARGET_DIR (the
  # top-level CMakeLists.txt). The lists of test binaries stay in cargo-test.
  foreach(profile debug release)
    if(MITCAD_CARGO_TARGET_ROOT)
      set(cargo_target_${profile} "${MITCAD_CARGO_TARGET_ROOT}/test-${profile}")
    else()
      set(cargo_target_${profile} "${CMAKE_CURRENT_BINARY_DIR}/cargo-test")
    endif()
  endforeach()

  # The crates' tests (mitcad#101). A fixture per profile builds the test
  # binaries of every crate in one cargo run, core.cargo_build (debug) and
  # core.cargo_build_release: cargo's lock on its target directory is taken
  # once and the crates compile in parallel, where a cargo test per crate
  # waited for the others' builds. The tests then run the binaries directly
  # (cargo-tests.cmake), at the same time, and core.doctests and
  # core.doctests_release run the documentation tests.
  #
  # mitcad_cargo_test(<name> <package> [RELEASE] [PARTS <part> <filter>...]):
  # the ctest <name> runs the package's tests (RELEASE: optimised). With
  # PARTS, <name>.<part> runs the tests whose names contain <filter>,
  # <name>.rest the others, and <name>.parts checks that the parts together
  # hold each of the package's tests exactly once (a new module goes to the
  # rest until it gets a part of its own).
  function(mitcad_cargo_test name package)
    cmake_parse_arguments(PARSE_ARGV 2 arg "RELEASE" "" "PARTS")
    set(profile debug)
    if(arg_RELEASE)
      set(profile release)
    endif()
    set_property(GLOBAL APPEND PROPERTY MITCAD_CARGO_TESTS_${profile} ${package})
    set(run "${CMAKE_COMMAND}" "-DLIST=${CMAKE_CURRENT_BINARY_DIR}/cargo-test/tests-${profile}.tsv"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/cargo-tests.cmake" --)
    if(NOT arg_PARTS)
      add_test(NAME ${name} COMMAND ${run} run ${package})
      set(tests ${name})
    else()
      set(tests)
      set(filters)
      set(skips)
      while(arg_PARTS)
        list(POP_FRONT arg_PARTS part filter)
        add_test(NAME ${name}.${part} COMMAND ${run} run ${package} "${filter}")
        list(APPEND tests ${name}.${part})
        list(APPEND filters "${filter}")
        list(APPEND skips --skip "${filter}")
      endwhile()
      add_test(NAME ${name}.rest COMMAND ${run} run ${package} ${skips})
      add_test(NAME ${name}.parts COMMAND ${run} parts ${package} ${filters})
      list(APPEND tests ${name}.rest ${name}.parts)
    endif()
    set_tests_properties(${tests} PROPERTIES FIXTURES_REQUIRED cargo_tests_${profile})
  endfunction()

  mitcad_cargo_test(core.model mitcad-model)
  # The general sketch below the model's core (mitcad#780).
  mitcad_cargo_test(core.sketch mitcad-sketch)
  # The Kernel trait and the base data below the model, and the mock
  # kernel's crate (mitcad#777).
  mitcad_cargo_test(core.model_base mitcad-model-base)
  mitcad_cargo_test(core.model_testkit mitcad-model-testkit)
  # The JSON api above the model (mitcad#777).
  mitcad_cargo_test(core.model_api mitcad-model-api)
  # The drawing layout and writers above the model (mitcad#777).
  mitcad_cargo_test(core.model_drawing mitcad-model-drawing)
  # BUILD planning (core/model-build) and the comparison of two designs
  # (core/model-diff) have no tests of their own and no test binary
  # (`test = false`, mitcad#831): their tests are core.model.build-* and
  # core.model.files.
  # The model's tests through the JSON api, a small crate per area
  # (mitcad#777, core/model-tests/README.md).
  mitcad_cargo_test(core.model.api mitcad-model-tests-api)
  mitcad_cargo_test(core.model.appearance mitcad-model-tests-appearance)
  mitcad_cargo_test(core.model.analysis mitcad-model-tests-analysis)
  mitcad_cargo_test(core.model.sketch mitcad-model-tests-sketch)
  mitcad_cargo_test(core.model.sketch-refs mitcad-model-tests-sketch-refs)
  mitcad_cargo_test(core.model.features-profile mitcad-model-tests-features-profile)
  mitcad_cargo_test(core.model.features-solid mitcad-model-tests-features-solid)
  mitcad_cargo_test(core.model.parts mitcad-model-tests-parts)
  mitcad_cargo_test(core.model.sheet mitcad-model-tests-sheet)
  mitcad_cargo_test(core.model.assembly mitcad-model-tests-assembly)
  mitcad_cargo_test(core.model.joints mitcad-model-tests-joints)
  mitcad_cargo_test(core.model.links mitcad-model-tests-links)
  mitcad_cargo_test(core.model.drawing mitcad-model-tests-drawing)
  mitcad_cargo_test(core.model.drawing-views mitcad-model-tests-drawing-views)
  mitcad_cargo_test(core.model.print mitcad-model-tests-print)
  mitcad_cargo_test(core.model.files mitcad-model-tests-files)
  mitcad_cargo_test(core.model.build-frames mitcad-model-tests-build-frames)
  mitcad_cargo_test(core.model.build-timber mitcad-model-tests-build-timber)
  # The imports' tests that need the whole model and its api (mitcad#831).
  mitcad_cargo_test(core.model.imports mitcad-model-tests-imports)
  # The model's tests of designs with several components again with
  # recompute workers (mitcad#842): MITCAD_RECOMPUTE_WORKERS makes every
  # document's recomputes evaluate independent components' features
  # ahead, and the tests must pass as with one thread.
  foreach(area assembly links joints)
    set(workers_test core.model.${area}.workers)
    add_test(NAME ${workers_test}
             COMMAND "${CMAKE_COMMAND}" "-DLIST=${CMAKE_CURRENT_BINARY_DIR}/cargo-test/tests-debug.tsv"
                     -P "${CMAKE_CURRENT_SOURCE_DIR}/cargo-tests.cmake" -- run mitcad-model-tests-${area})
    set_tests_properties(${workers_test} PROPERTIES FIXTURES_REQUIRED cargo_tests_debug
                         ENVIRONMENT "MITCAD_RECOMPUTE_WORKERS=4")
  endforeach()
  # Parameter expressions and the print settings' key map, crates below
  # the model (mitcad#777).
  mitcad_cargo_test(core.expr mitcad-expr)
  mitcad_cargo_test(core.print-keys mitcad-print-keys)
  # Slicers' print settings in real 3MF projects under MITCAD_3MF_CORPUS
  # (mitcad#640; core/model-tests/print/src/print_files_corpus.rs, an ignored
  # test that core.model.print leaves out): each project an index.tsv lists
  # imported as the application imports it, checked against its row,
  # exported in the same dialect and read back; skipped without the corpus.
  add_test(NAME 3mf.corpus
    COMMAND "${CMAKE_COMMAND}" "-DLIST=${CMAKE_CURRENT_BINARY_DIR}/cargo-test/tests-debug.tsv"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/cargo-tests.cmake" -- run mitcad-model-tests-print print_files_corpus:: --ignored
            --nocapture)
  set_tests_properties(3mf.corpus PROPERTIES FIXTURES_REQUIRED cargo_tests_debug
    SKIP_REGULAR_EXPRESSION "3mf corpus: skipped")

  # The .f3d import on synthetic dumps with the mock kernel (T1).
  mitcad_cargo_test(core.import mitcad-import)
  # The crates split from the replay (mitcad#785): the .solidcraft import
  # on the reader's test designs with the mock kernel; the replay's core
  # (its unit tests; the replay's own tests are core.import's) and its
  # families: sheet metal (its unit tests); joints and primitives have none
  # and no test binary (`test = false`, mitcad#831).
  mitcad_cargo_test(core.import-solidcraft mitcad-import-solidcraft)
  mitcad_cargo_test(core.import-replay mitcad-import-replay)
  mitcad_cargo_test(core.import-sheet mitcad-import-sheet)
  # The appearances family (mitcad#791).
  mitcad_cargo_test(core.import-appearances mitcad-import-appearances)
  # The imports' crates split from mitcad-import (mitcad#775): what they
  # share, IFC, .rvt (its corpus tests read MITCAD_RVT_CORPUS and skip
  # without it) and FreeCAD documents, with the mock kernel.
  mitcad_cargo_test(core.import-base mitcad-import-base)
  mitcad_cargo_test(core.import-ifc mitcad-import-ifc)
  mitcad_cargo_test(core.import-rvt mitcad-import-rvt)
  mitcad_cargo_test(core.import-fcstd mitcad-import-fcstd)

  mitcad_cargo_test(core.solver mitcad-solver)

  # The .f3d reader; its corpus test reads MITCAD_F3D_CORPUS (default
  # ~/f3d-corpus) and skips when the corpus is missing.
  mitcad_cargo_test(core.f3d mitcad-f3d RELEASE)
  # The crates split off the .f3d reader (mitcad#776). The dump IR.
  mitcad_cargo_test(core.dump mitcad-dump RELEASE)
  # The ASM part.
  mitcad_cargo_test(core.asm mitcad-asm RELEASE)
  # The names of B-rep entities.
  mitcad_cargo_test(core.f3d-names mitcad-f3d-names RELEASE)
  # The document of the file, and the design segment.
  mitcad_cargo_test(core.f3d-file mitcad-f3d-file RELEASE)
  mitcad_cargo_test(core.f3d-design mitcad-f3d-design RELEASE)
  # The streams, records and sketches of the design segment (mitcad#779).
  mitcad_cargo_test(core.f3d-decode mitcad-f3d-decode RELEASE)

  mitcad_cargo_test(core.dxf mitcad-dxf)

  # Version history (P12b): commits, history and older versions in
  # temporary git repositories, checked with the system's git where it is
  # installed. In parts by module: remote repositories, sync, libraries,
  # the edit locks (mitcad#89; once their module is there), and the rest.
  set(vcs_parts remote tests::remote:: sync tests::sync:: library tests::library::)
  if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/vcs/src/tests/locks.rs")
    list(APPEND vcs_parts locks tests::locks::)
  endif()
  mitcad_cargo_test(core.vcs mitcad-vcs PARTS ${vcs_parts})

  # The zip container shared by the .f3d and .FCStd readers, and the
  # FreeCAD reader on hand-made documents (no OCCT).
  foreach(crate zip freecad)
    mitcad_cargo_test(core.${crate} mitcad-${crate})
  endforeach()

  # Automatic updates (mitcad#9): manifests and downloads signed with keys
  # made in the test (valid, tampered, the wrong key, older versions,
  # pre-releases), and the release tool; no network.
  mitcad_cargo_test(core.update mitcad-update)

  # Licences, bans and sources need no network; advisories are checked by
  # tools/check-dependencies.sh, which fetches the advisory database.
  find_program(CARGO_DENY cargo-deny HINTS "$ENV{HOME}/.cargo/bin" "$ENV{USERPROFILE}/.cargo/bin")
  if(CARGO_DENY)
    add_test(NAME core.dependencies
      COMMAND "${CARGO_DENY}" --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
              check licenses bans sources)
    set_tests_properties(core.dependencies PROPERTIES ENVIRONMENT "${cargo_test_environment}")
  else()
    message(STATUS "cargo-deny not found; dependency policy test skipped")
  endif()

  # The 3MF writer and reader (mitcad#13; no OCCT).
  mitcad_cargo_test(core.3mf mitcad-3mf)

  # The .ipt reader (mitcad#60) on files made in the tests (no OCCT); its
  # corpus test reads MITCAD_IPT_CORPUS and skips without it.
  mitcad_cargo_test(core.ipt mitcad-ipt RELEASE)

  # MCP stdio framing and discovery, with a mock document backend (mitcad#124).
  mitcad_cargo_test(core.mcp mitcad-mcp)

  # The USDZ writer and its checking reader (mitcad#123; no OCCT).
  mitcad_cargo_test(core.usdz mitcad-usdz)

  # The IFC reader (mitcad#273) on files written in the tests (no OCCT);
  # its corpus test reads MITCAD_IFC_CORPUS and skips without it.
  mitcad_cargo_test(core.ifc mitcad-ifc RELEASE)
  # The compound file container (shared by the .ipt and .rvt readers) and
  # the .rvt/.rfa reader (mitcad#274) on files made in the tests; its
  # corpus test reads MITCAD_RVT_CORPUS and skips without it.
  mitcad_cargo_test(core.cfb mitcad-cfb)
  mitcad_cargo_test(core.rvt mitcad-rvt RELEASE)
  # The .solidcraft reader (mitcad#450): formats, upgrades, the container
  # (no OCCT); its corpus test reads MITCAD_SOLIDCRAFT_CORPUS and skips
  # without it.
  mitcad_cargo_test(core.solidcraft mitcad-solidcraft)
  # The .dwg reader (mitcad#453) on drawings written by its tests in every
  # version (no OCCT).
  mitcad_cargo_test(core.dwg mitcad-dwg)
  # The vector helpers on [f64; 3] that the model, the solver and the
  # readers share (mitcad#582; no OCCT).
  mitcad_cargo_test(core.vec3 mitcad-vec3)
  # The test runner's scheduling, profiles and parsers (tools/testrun,
  # mitcad#790); no ctest needed.
  mitcad_cargo_test(tools.testrun mitcad-testrun)

  # The fixtures that build the test binaries, and the documentation tests
  # (cargo with the fixture's packages, so that it builds nothing again).
  foreach(profile debug release)
    get_property(packages GLOBAL PROPERTY MITCAD_CARGO_TESTS_${profile})
    if(NOT packages)
      continue()
    endif()
    set(suffix "")
    set(release "")
    if(profile STREQUAL "release")
      set(suffix _release)
      set(release --release)
    endif()
    # Incremental compilation is part of the hash in the names of the files
    # cargo builds for Mitcad's crates: a CARGO_INCREMENTAL inherited from
    # the shell (0 for one run, unset for the next) built every crate and
    # test binary again beside the earlier ones (mitcad#796). The test
    # builds pin it to the profile's default (on in debug, off in release),
    # off in debug too with MITCAD_CARGO_TEST_INCREMENTAL=OFF, and leave it
    # to the compiler cache's environment, which turns it off.
    set(incremental 0)
    if(profile STREQUAL "debug" AND MITCAD_CARGO_TEST_INCREMENTAL)
      set(incremental 1)
    endif()
    set(cargo_build_environment "CARGO_TARGET_DIR=${cargo_target_${profile}}" ${cargo_test_environment})
    if(NOT MITCAD_RUST_CACHE_ENVIRONMENT MATCHES "CARGO_INCREMENTAL=")
      list(APPEND cargo_build_environment "CARGO_INCREMENTAL=${incremental}")
    endif()
    list(APPEND cargo_build_environment ${MITCAD_RUST_CACHE_ENVIRONMENT})
    set(selected)
    foreach(package IN LISTS packages)
      list(APPEND selected -p ${package})
    endforeach()
    add_test(NAME core.cargo_build${suffix}
      COMMAND "${CMAKE_COMMAND}" "-DCARGO=${Rust_CARGO_CACHED}" "-DMANIFEST=${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
              "-DLIST=${CMAKE_CURRENT_BINARY_DIR}/cargo-test/tests-${profile}.tsv"
              -P "${CMAKE_CURRENT_SOURCE_DIR}/cargo-tests.cmake" -- build ${release} ${packages})
    # A build from scratch (a new build folder, a new toolchain) takes long
    # in a VM. A build runs alone (RUN_SERIAL): a debug build from scratch
    # beside the release profile's crate tests, the geometry tests and the
    # .rvt corpus ran the Linux release check of 0.10.0 out of its memory
    # limit (18.10.2026).
    # On Linux the test runner (mitcad-testrun, mitcad#790) schedules by a
    # memory budget and each test's measured peak, and the build uses only
    # check-all's 8 cargo jobs of the machine's cores, so there the build
    # runs beside the tests that do not need it (C++, geometry, CLI, UI),
    # one cargo build at a time (RESOURCE_LOCK), instead of alone for its
    # ten minutes. Windows likewise: its cargo jobs follow the available
    # memory (cargo-tests.cmake, mitcad#796) and its merge checks and
    # release run the tests through the test runner.
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR WIN32)
      set(cargo_build_alone RESOURCE_LOCK cargo_build)
    else()
      set(cargo_build_alone RUN_SERIAL TRUE)
    endif()
    set_tests_properties(core.cargo_build${suffix} PROPERTIES FIXTURES_SETUP cargo_tests_${profile}
      ENVIRONMENT "${cargo_build_environment}" TIMEOUT 3600 ${cargo_build_alone})
    add_test(NAME core.doctests${suffix}
      COMMAND "${Rust_CARGO_CACHED}" test --doc --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
              --quiet ${release} ${selected})
    set_tests_properties(core.doctests${suffix} PROPERTIES FIXTURES_REQUIRED cargo_tests_${profile}
      ENVIRONMENT "${cargo_build_environment}" RESOURCE_LOCK cargo_build)
  endforeach()
endif()
