# SPDX-License-Identifier: MIT
# The bridge, and the release tool mitcad-release of the automatic updates
# (mitcad#9, docs/updates.md).
corrosion_import_crate(MANIFEST_PATH Cargo.toml CRATES mitcad-ffi mitcad-update)
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
corrosion_add_cxxbridge(mitcad_bridge CRATE mitcad_ffi FILES
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
  memory.rs)
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
target_include_directories(mitcad_bridge PUBLIC cpp)
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
  set_tests_properties(f3d.models_loft PROPERTIES SKIP_RETURN_CODE 77)

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
  set(cargo_build_environment "CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR}/cargo-test" ${cargo_test_environment})

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

  # The .f3d import on synthetic dumps with the mock kernel (T1).
  mitcad_cargo_test(core.import mitcad-import)

  mitcad_cargo_test(core.solver mitcad-solver)

  # The .f3d reader; its corpus test reads MITCAD_F3D_CORPUS (default
  # ~/f3d-corpus) and skips when the corpus is missing.
  mitcad_cargo_test(core.f3d mitcad-f3d RELEASE)

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
    set(selected)
    foreach(package IN LISTS packages)
      list(APPEND selected -p ${package})
    endforeach()
    add_test(NAME core.cargo_build${suffix}
      COMMAND "${CMAKE_COMMAND}" "-DCARGO=${Rust_CARGO_CACHED}" "-DMANIFEST=${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
              "-DLIST=${CMAKE_CURRENT_BINARY_DIR}/cargo-test/tests-${profile}.tsv"
              -P "${CMAKE_CURRENT_SOURCE_DIR}/cargo-tests.cmake" -- build ${release} ${packages})
    # A build from scratch (a new build folder, a new toolchain) takes long
    # in a VM.
    set_tests_properties(core.cargo_build${suffix} PROPERTIES FIXTURES_SETUP cargo_tests_${profile}
      ENVIRONMENT "${cargo_build_environment}" TIMEOUT 3600)
    add_test(NAME core.doctests${suffix}
      COMMAND "${Rust_CARGO_CACHED}" test --doc --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
              --quiet ${release} ${selected})
    set_tests_properties(core.doctests${suffix} PROPERTIES FIXTURES_REQUIRED cargo_tests_${profile}
      ENVIRONMENT "${cargo_build_environment}")
  endforeach()
endif()
