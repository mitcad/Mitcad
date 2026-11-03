# SPDX-License-Identifier: MIT
# The crates' tests in ctest (core/CMakeLists.txt, mitcad#101), in script
# mode, with the arguments after "--":
#
#   cmake -DCARGO=<cargo> -DMANIFEST=<core/Cargo.toml> -DLIST=<file> -P cargo-tests.cmake
#         -- build [--release] <package>...
#     Builds the test binaries of the packages in one cargo run (cargo test
#     --no-run) and lists them in LIST, a line per binary: the package, its
#     folder, the target and the binary, separated by tabs. The build runs
#     with a job count the memory allows (cargo_jobs below), and after it
#     the profile's folder in CARGO_TARGET_DIR is swept of what the build
#     did not name (sweep below; MITCAD_CARGO_SWEEP=0 turns that off).
#   cmake -DLIST=<file> [-DTEST_NAME=<ctest>] -P cargo-tests.cmake -- run <package> [<argument>...]
#     Runs the package's test binaries in the package's folder, as cargo
#     test does, with libtest's arguments (a filter, --skip <filter>); on
#     Windows with a temporary folder of the run's own (see below), named
#     after TEST_NAME, the package and the arguments.
#   cmake -DLIST=<file> -P cargo-tests.cmake -- parts <package> <filter>...
#     Checks the parts a package's tests run in: those of each filter, and
#     the rest (--skip of every filter). Together they must list each test
#     of the package's binaries (--list) exactly once, and each filter must
#     match a test (else it is left from a module renamed or removed).
cmake_minimum_required(VERSION 3.24)

# The arguments after "--".
set(args)
set(after FALSE)
math(EXPR last "${CMAKE_ARGC} - 1")
foreach(i RANGE ${last})
  if(after)
    list(APPEND args "${CMAKE_ARGV${i}}")
  elseif("${CMAKE_ARGV${i}}" STREQUAL "--")
    set(after TRUE)
  endif()
endforeach()
list(POP_FRONT args mode)

# package_name(<id> <out>): the name in a cargo package id, in the form
# path+file:///.../vcs#mitcad-vcs@0.2.0, path+file:///.../name#0.2.0 (the
# name is the folder's) or the older "mitcad-vcs 0.2.0 (path+file:///...)".
function(package_name id out)
  if(id MATCHES "#([^#@]+)@[^#]*$")
    set(${out} "${CMAKE_MATCH_1}" PARENT_SCOPE)
  elseif(id MATCHES "/([^/#]+)#[^#@]*$")
    set(${out} "${CMAKE_MATCH_1}" PARENT_SCOPE)
  elseif(id MATCHES "^([^ ]+) ")
    set(${out} "${CMAKE_MATCH_1}" PARENT_SCOPE)
  else()
    message(FATAL_ERROR "cannot read the package id ${id}")
  endif()
endfunction()

# binaries(<package> <out>): the package's lines of LIST, each the folder,
# the target and the binary separated by "|".
function(binaries package out)
  if(NOT EXISTS "${LIST}")
    message(FATAL_ERROR "${LIST} is missing: the test binaries were not built")
  endif()
  file(READ "${LIST}" text)
  string(REPLACE "\n" ";" lines "${text}")
  set(found)
  foreach(line IN LISTS lines)
    if(line STREQUAL "")
      continue()
    endif()
    string(REPLACE "\t" ";" fields "${line}")
    list(POP_FRONT fields name)
    if(name STREQUAL package)
      list(JOIN fields "|" entry)
      list(APPEND found "${entry}")
    endif()
  endforeach()
  if(NOT found)
    message(FATAL_ERROR "no test binaries of ${package} in ${LIST}")
  endif()
  set(${out} "${found}" PARENT_SCOPE)
endfunction()

# list_tests(<binary> <folder> <out> [<argument>...]): the tests the binary
# runs with these arguments (libtest's --list).
function(list_tests binary folder out)
  execute_process(COMMAND "${binary}" --list --format terse ${ARGN} WORKING_DIRECTORY "${folder}"
    OUTPUT_VARIABLE listed RESULT_VARIABLE status)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "${binary} --list failed (${status})")
  endif()
  string(REGEX MATCHALL "[^\n]+: test" tests "${listed}")
  list(TRANSFORM tests REPLACE ": test$" "")
  set(${out} "${tests}" PARENT_SCOPE)
endfunction()

# cargo_jobs(): sets CARGO_BUILD_JOBS for the test build unless it is set
# already (mitcad#796): one job per 2.5 GiB of the memory available now
# (MITCAD_CARGO_JOB_MIB, in MiB, changes the 2.5 GiB), at least one and at
# most the logical cores. A rustc job of the test build (a large crate, or a
# test binary's link with its dependencies' debug information) takes up to
# about 2.5 GiB; with a job per core, 16 jobs ran a Windows VM of 10-19 GB
# out of memory after the crates were split. On Linux the limit of the
# cgroup the build runs in (a systemd scope or slice of check-all or
# mitcad-testrun) counts too: its memory.max less its anonymous memory
# (its page cache is reclaimed when needed), at each level up to the root.
function(cargo_jobs)
  # The native macOS scheduler keeps two compiler jobs within its reserved
  # headroom. Also cap direct CTest runs, without increasing a caller's
  # smaller job count. Other platforms leave this unset.
  set(max_jobs 0)
  if("$ENV{MITCAD_CARGO_MAX_JOBS}" MATCHES "^[1-9][0-9]*$")
    set(max_jobs "$ENV{MITCAD_CARGO_MAX_JOBS}")
  endif()
  if(NOT "$ENV{CARGO_BUILD_JOBS}" STREQUAL "")
    if(max_jobs GREATER 0 AND "$ENV{CARGO_BUILD_JOBS}" MATCHES "^[1-9][0-9]*$"
        AND "$ENV{CARGO_BUILD_JOBS}" GREATER max_jobs)
      set(ENV{CARGO_BUILD_JOBS} ${max_jobs})
    endif()
    message("cargo jobs: $ENV{CARGO_BUILD_JOBS} (CARGO_BUILD_JOBS)")
    return()
  endif()
  set(per_job 2560)
  if("$ENV{MITCAD_CARGO_JOB_MIB}" MATCHES "^[1-9][0-9]*$")
    set(per_job "$ENV{MITCAD_CARGO_JOB_MIB}")
  endif()
  cmake_host_system_information(RESULT available QUERY AVAILABLE_PHYSICAL_MEMORY)
  cmake_host_system_information(RESULT cores QUERY NUMBER_OF_LOGICAL_CORES)
  if(NOT available MATCHES "^[0-9]+$" OR NOT cores MATCHES "^[1-9][0-9]*$")
    if(max_jobs GREATER 0)
      set(ENV{CARGO_BUILD_JOBS} 1)
      message("cargo jobs: 1 (the memory or the cores could not be read; bounded compiler jobs)")
    else()
      message("cargo jobs: cargo's default (the memory or the cores could not be read)")
    endif()
    return()
  endif()
  set(source "available")
  if(CMAKE_HOST_UNIX AND EXISTS "/proc/self/cgroup")
    file(STRINGS "/proc/self/cgroup" groups REGEX "^0::/")
    list(LENGTH groups count)
    if(count EQUAL 1)
      string(REGEX REPLACE "^0::" "" group "${groups}")
      string(REGEX REPLACE "/+$" "" dir "/sys/fs/cgroup${group}")
      while(dir MATCHES "^/sys/fs/cgroup/.")
        if(EXISTS "${dir}/memory.max" AND EXISTS "${dir}/memory.stat")
          file(READ "${dir}/memory.max" max)
          string(STRIP "${max}" max)
          file(STRINGS "${dir}/memory.stat" anon REGEX "^anon [0-9]+$")
          string(REGEX REPLACE "^anon " "" anon "${anon}")
          if(max MATCHES "^[0-9]+$" AND anon MATCHES "^[0-9]+$")
            set(left 0)
            if(max GREATER anon)
              math(EXPR left "(${max} - ${anon}) / 1048576")
            endif()
            if(left LESS available)
              set(available ${left})
              set(source "left in ${dir}")
            endif()
          endif()
        endif()
        get_filename_component(dir "${dir}" DIRECTORY)
      endwhile()
    endif()
  endif()
  math(EXPR jobs "${available} / ${per_job}")
  if(jobs GREATER cores)
    set(jobs ${cores})
  endif()
  if(max_jobs GREATER 0 AND jobs GREATER max_jobs)
    set(jobs ${max_jobs})
  endif()
  if(jobs LESS 1)
    set(jobs 1)
  endif()
  set(ENV{CARGO_BUILD_JOBS} ${jobs})
  message("cargo jobs: ${jobs} (${available} MiB ${source}, ${per_job} MiB a job, ${cores} cores)")
endfunction()

# The hash cargo puts in the names of a unit's files and folders,
# <name>-<hash>: 16 lowercase hexadecimal digits.
string(REPEAT "[0-9a-f]" 16 hex16)

# under(<path> <folder> <out>): whether the path is the folder or inside it
# (without regard to case on Windows).
function(under path folder out)
  set(path "${path}/")
  set(folder "${folder}/")
  if(CMAKE_HOST_WIN32)
    string(TOLOWER "${path}" path)
    string(TOLOWER "${folder}" folder)
  endif()
  string(FIND "${path}" "${folder}" at)
  if(at EQUAL 0)
    set(${out} TRUE PARENT_SCOPE)
  else()
    set(${out} FALSE PARENT_SCOPE)
  endif()
endfunction()

# entry_size(<path> <out>): the size in bytes of a file, or of the files in
# a folder (links not followed).
function(entry_size path out)
  set(total 0)
  if(IS_DIRECTORY "${path}")
    file(GLOB_RECURSE files LIST_DIRECTORIES false "${path}/*")
  else()
    set(files "${path}")
  endif()
  foreach(file IN LISTS files)
    if(EXISTS "${file}" AND NOT IS_SYMLINK "${file}")
      file(SIZE "${file}" size)
      math(EXPR total "${total} + ${size}")
    endif()
  endforeach()
  set(${out} ${total} PARENT_SCOPE)
endfunction()

# stale(<kind> <path>): marks a file or folder of the sweep for removal
# (doomed, and its count and size by kind), unless it was written after
# the build started (by another build in the same folder).
function(stale kind path)
  file(TIMESTAMP "${path}" modified "%s" UTC)
  if(NOT modified MATCHES "^[0-9]+$" OR NOT modified LESS started)
    return()
  endif()
  entry_size("${path}" size)
  math(EXPR count "${count_${kind}} + 1")
  math(EXPR bytes "${bytes_${kind}} + ${size}")
  set(count_${kind} ${count} PARENT_SCOPE)
  set(bytes_${kind} ${bytes} PARENT_SCOPE)
  list(APPEND doomed "${path}")
  set(doomed "${doomed}" PARENT_SCOPE)
endfunction()

# json(<out> <mode> <json> <member|index>...): string(JSON) that keeps its
# first error in json_error instead of failing the script.
function(json out mode text)
  string(JSON value ERROR_VARIABLE error ${mode} "${text}" ${ARGN})
  if(error AND NOT json_error)
    set(json_error "${error}" PARENT_SCOPE)
  endif()
  set(${out} "${value}" PARENT_SCOPE)
endfunction()

# sweep(<profile>): after a successful build, removes from the profile's
# folder in CARGO_TARGET_DIR (debug or release) what the build did not name
# in its messages (mitcad#796), so that the folder stays about the size of
# one build: cargo keeps every unit it ever built there, and each change of
# a dependency's version, of the features, flags, environment, toolchain or
# the checkout's path builds new units beside the old ones. The build's
# units are its messages' compiler-artifact (built or fresh) and
# build-script-executed entries; the hashes in the paths they name are the
# units' (cargo names a unit's files and folders <name>-<hash> with the
# same hash). Removed, when they are older than the build's start:
# - deps/: the files and folders <name>-<hash>[.<extension>] with another
#   hash (.rlib, .rmeta, .d, .pdb, .exe, .dll, .lib, .exp, .dwo, .dSYM ...);
#   names without a hash and those of programs (bin targets, whose files
#   the messages name only by their copies outside deps/) stay;
# - .fingerprint/ and build/: the folders <name>-<hash> with another hash,
#   except fingerprints of programs and examples (same reason);
# - incremental/: the sessions of crates the build has no unit of, and of
#   a crate with more sessions than units the oldest ones (a session
#   removed only makes the crate's next change compile in full).
# Nothing outside these four folders of the profile's folder is touched,
# and nothing at all when the messages cannot be read or name no unit in
# that folder. Since what is removed was not part of the build, the next
# build of the same sources compiles nothing again.
function(sweep profile)
  if("$ENV{MITCAD_CARGO_SWEEP}" STREQUAL "0")
    message("cargo sweep: off (MITCAD_CARGO_SWEEP=0)")
    return()
  endif()
  if("$ENV{CARGO_TARGET_DIR}" STREQUAL "")
    message("cargo sweep: skipped (CARGO_TARGET_DIR is not set)")
    return()
  endif()
  file(TO_CMAKE_PATH "$ENV{CARGO_TARGET_DIR}" target)
  cmake_path(ABSOLUTE_PATH target NORMALIZE)
  string(REGEX REPLACE "/+$" "" target "${target}")
  set(dir "${target}/${profile}")
  if(NOT IS_DIRECTORY "${dir}/deps")
    message("cargo sweep: skipped (no ${dir}/deps)")
    return()
  endif()

  # The build's units: their hashes (kept_<hash>), the units per crate
  # (units_<crate>) and the programs' crates (program_<crate>).
  string(REGEX MATCHALL "[^\n]*\"reason\":\"(compiler-artifact|build-script-executed)\"[^\n]*"
    lines "${messages}")
  set(units 0)
  set(inside 0)
  set(json_error)
  foreach(line IN LISTS lines)
    set(paths)
    json(reason GET "${line}" reason)
    if(reason STREQUAL "build-script-executed")
      json(out GET "${line}" out_dir)
      list(APPEND paths "${out}")
    elseif(reason STREQUAL "compiler-artifact")
      math(EXPR units "${units} + 1")
      json(name GET "${line}" target name)
      string(REPLACE "-" "_" crate "${name}")
      if(NOT crate MATCHES "^[A-Za-z0-9_]+$")
        set(json_error "the target name ${name}")
        break()
      endif()
      if(NOT DEFINED units_${crate})
        set(units_${crate} 0)
      endif()
      math(EXPR units_${crate} "${units_${crate}} + 1")
      json(kinds GET "${line}" target kind)
      if(kinds MATCHES "\"(bin|example)\"")
        set(program_${crate} TRUE)
      endif()
      json(count LENGTH "${line}" filenames)
      if(count MATCHES "^[1-9][0-9]*$")
        math(EXPR last "${count} - 1")
        foreach(i RANGE ${last})
          json(file GET "${line}" filenames ${i})
          list(APPEND paths "${file}")
        endforeach()
      endif()
      json(executable GET "${line}" executable)
      if(NOT executable STREQUAL "")
        list(APPEND paths "${executable}")
      endif()
    endif()
    if(json_error)
      break()
    endif()
    foreach(path IN LISTS paths)
      file(TO_CMAKE_PATH "${path}" path)
      under("${path}" "${dir}" is_inside)
      if(is_inside)
        math(EXPR inside "${inside} + 1")
      endif()
      string(REGEX MATCHALL "-${hex16}" hashes "${path}")
      foreach(hash IN LISTS hashes)
        string(SUBSTRING "${hash}" 1 16 hash)
        set(kept_${hash} TRUE)
      endforeach()
    endforeach()
  endforeach()
  if(json_error)
    message("cargo sweep: skipped (a message of cargo could not be read: ${json_error})")
    return()
  endif()
  if(units EQUAL 0 OR inside EQUAL 0)
    message("cargo sweep: skipped (the build named no unit in ${dir})")
    return()
  endif()

  set(started "${build_started}")
  set(doomed)
  foreach(kind deps fingerprint build incremental)
    set(count_${kind} 0)
    set(bytes_${kind} 0)
  endforeach()

  # (The names are matched first and the match used in a second if():
  # an if() expands its variables before it evaluates MATCHES.)
  file(GLOB entries LIST_DIRECTORIES true "${dir}/deps/*")
  foreach(entry IN LISTS entries)
    get_filename_component(name "${entry}" NAME)
    if(IS_SYMLINK "${entry}" OR NOT name MATCHES "^([A-Za-z0-9_]+)-(${hex16})(\\..*)?$")
      continue()
    endif()
    if(NOT DEFINED kept_${CMAKE_MATCH_2} AND NOT DEFINED program_${CMAKE_MATCH_1})
      stale(deps "${entry}")
    endif()
  endforeach()

  foreach(kind fingerprint build)
    if(kind STREQUAL "fingerprint")
      file(GLOB entries LIST_DIRECTORIES true "${dir}/.fingerprint/*")
    else()
      file(GLOB entries LIST_DIRECTORIES true "${dir}/build/*")
    endif()
    foreach(entry IN LISTS entries)
      get_filename_component(name "${entry}" NAME)
      if(NOT IS_DIRECTORY "${entry}" OR IS_SYMLINK "${entry}" OR NOT name MATCHES "-(${hex16})$")
        continue()
      endif()
      if(DEFINED kept_${CMAKE_MATCH_1})
        continue()
      endif()
      if(kind STREQUAL "fingerprint")
        file(GLOB programs "${entry}/bin-*" "${entry}/example-*")
        if(programs)
          continue()
        endif()
      endif()
      stale(${kind} "${entry}")
    endforeach()
  endforeach()

  # incremental/<crate>-<id>: the id is rustc's, not cargo's hash.
  file(GLOB entries LIST_DIRECTORIES true "${dir}/incremental/*")
  set(crates)
  foreach(entry IN LISTS entries)
    get_filename_component(name "${entry}" NAME)
    if(NOT IS_DIRECTORY "${entry}" OR IS_SYMLINK "${entry}" OR NOT name MATCHES "^([A-Za-z0-9_]+)-[0-9a-z]+$")
      continue()
    endif()
    set(crate "${CMAKE_MATCH_1}")
    if(NOT DEFINED units_${crate})
      stale(incremental "${entry}")
      continue()
    endif()
    file(TIMESTAMP "${entry}" modified "%s" UTC)
    list(APPEND sessions_${crate} "${modified}|${entry}")
    list(APPEND crates ${crate})
  endforeach()
  if(crates)
    list(REMOVE_DUPLICATES crates)
  endif()
  foreach(crate IN LISTS crates)
    list(LENGTH sessions_${crate} count)
    if(count GREATER "${units_${crate}}")
      list(SORT sessions_${crate})
      math(EXPR extra "${count} - ${units_${crate}}")
      list(SUBLIST sessions_${crate} 0 ${extra} old)
      foreach(session IN LISTS old)
        string(REGEX REPLACE "^[0-9]*\\|" "" session "${session}")
        stale(incremental "${session}")
      endforeach()
    endif()
  endforeach()

  # Removed in batches by cmake -E rm, which reports what it cannot remove
  # (a file in use) without failing the build.
  set(batch)
  set(length 0)
  foreach(path IN LISTS doomed)
    list(APPEND batch "${path}")
    string(LENGTH "${path}" size)
    math(EXPR length "${length} + ${size} + 3")
    if(length GREATER 16000)
      execute_process(COMMAND "${CMAKE_COMMAND}" -E rm -rf -- ${batch})
      set(batch)
      set(length 0)
    endif()
  endforeach()
  if(batch)
    execute_process(COMMAND "${CMAKE_COMMAND}" -E rm -rf -- ${batch})
  endif()
  set(left 0)
  foreach(path IN LISTS doomed)
    if(EXISTS "${path}")
      math(EXPR left "${left} + 1")
    endif()
  endforeach()

  set(count 0)
  set(bytes 0)
  set(parts)
  foreach(kind deps fingerprint build incremental)
    math(EXPR count "${count} + ${count_${kind}}")
    math(EXPR bytes "${bytes} + ${bytes_${kind}}")
    math(EXPR mib "(${bytes_${kind}} + 524288) / 1048576")
    list(APPEND parts "${kind} ${count_${kind}} (${mib} MiB)")
  endforeach()
  list(JOIN parts ", " parts)
  math(EXPR mib "(${bytes} + 524288) / 1048576")
  message("cargo sweep of ${dir}: ${count} stale entries, ${mib} MiB, removed (${parts}); "
          "${units} units of the build kept")
  if(left GREATER 0)
    message("cargo sweep: ${left} entries could not be removed")
  endif()
endfunction()

if(mode STREQUAL "build")
  set(profile)
  set(profile_folder debug)
  set(packages)
  foreach(arg IN LISTS args)
    if(arg STREQUAL "--release")
      set(profile --release)
      set(profile_folder release)
    else()
      list(APPEND packages -p "${arg}")
    endif()
  endforeach()
  cargo_jobs()
  string(TIMESTAMP build_started "%s" UTC)
  # Cargo's messages are JSON on standard output, its progress and the
  # compilers' diagnostics go to standard error as usual.
  execute_process(COMMAND "${CARGO}" test --no-run --manifest-path "${MANIFEST}" ${profile} ${packages}
                          --message-format json-render-diagnostics
    OUTPUT_VARIABLE messages RESULT_VARIABLE status)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "cargo test --no-run failed (${status})")
  endif()
  # The test binaries: the artifacts with an executable built as tests (not
  # the programs that integration tests run).
  string(REGEX MATCHALL "[^\n]*\"executable\":\"[^\n]*" artifacts "${messages}")
  set(lines)
  foreach(artifact IN LISTS artifacts)
    string(JSON test GET "${artifact}" profile test)
    if(NOT test)
      continue()
    endif()
    string(JSON id GET "${artifact}" package_id)
    string(JSON manifest GET "${artifact}" manifest_path)
    string(JSON kind GET "${artifact}" target kind 0)
    string(JSON target GET "${artifact}" target name)
    string(JSON executable GET "${artifact}" executable)
    package_name("${id}" name)
    file(TO_CMAKE_PATH "${manifest}" manifest)
    get_filename_component(folder "${manifest}" DIRECTORY)
    file(TO_CMAKE_PATH "${executable}" executable)
    list(APPEND lines "${name}\t${folder}\t${kind} ${target}\t${executable}")
  endforeach()
  list(SORT lines)
  list(LENGTH lines count)
  list(JOIN lines "\n" text)
  file(WRITE "${LIST}" "${text}\n")
  message("${count} test binaries listed in ${LIST}")
  sweep(${profile_folder})

elseif(mode STREQUAL "run")
  list(POP_FRONT args package)
  binaries("${package}" entries)
  # On Windows the tests' temporary folders (std::env::temp_dir and the git
  # they run) go to a folder of this run outside the system's temporary
  # folder: Storage Sense, which runs when the disk is low, deletes empty
  # folders there, also those of a git repository a test has just made
  # (refs, objects), and git then no longer sees the repository. Not in the
  # build tree: a build folder in a git checkout would put the tests'
  # folders in that repository. Each run (ctest, package and filter in this
  # build) has its own folder, as parts of a package run at the same time.
  # The ctest's name counts too (mitcad#1017): core.model.assembly and
  # core.model.assembly.workers run the same binary with the same arguments
  # (only their environment differs), and with one folder for both the run
  # that started or ended removed the folder under the other's tests.
  set(temporary)
  if(CMAKE_HOST_WIN32 AND DEFINED ENV{LOCALAPPDATA})
    file(TO_CMAKE_PATH "$ENV{LOCALAPPDATA}" local)
    string(SHA1 run_id "${LIST};${TEST_NAME};${package};${args}")
    string(SUBSTRING "${run_id}" 0 8 run_id)
    set(temporary "${local}/mitcad-tests/${run_id}")
    file(REMOVE_RECURSE "${temporary}")
    file(MAKE_DIRECTORY "${temporary}")
    file(TO_NATIVE_PATH "${temporary}" native)
    set(ENV{TMP} "${native}")
    set(ENV{TEMP} "${native}")
  endif()
  set(failed)
  foreach(entry IN LISTS entries)
    string(REPLACE "|" ";" fields "${entry}")
    list(GET fields 0 folder)
    list(GET fields 1 target)
    list(GET fields 2 binary)
    message("${target}")
    execute_process(COMMAND "${binary}" --quiet ${args} WORKING_DIRECTORY "${folder}" RESULT_VARIABLE status)
    if(NOT status EQUAL 0)
      list(APPEND failed "${target} (${status})")
    endif()
  endforeach()
  if(failed)
    list(JOIN failed ", " failed)
    message(FATAL_ERROR "failed: ${failed}")
  endif()
  if(temporary)
    file(REMOVE_RECURSE "${temporary}")
  endif()

elseif(mode STREQUAL "parts")
  list(POP_FRONT args package)
  set(filters "${args}")
  if(NOT filters)
    message(FATAL_ERROR "parts: no filters")
  endif()
  list(LENGTH filters parts)
  math(EXPR last_part "${parts} - 1")
  set(skips)
  foreach(i RANGE ${last_part})
    list(GET filters ${i} filter)
    list(APPEND skips --skip "${filter}")
    set(count_${i} 0)
  endforeach()
  set(count_rest 0)
  set(total 0)
  set(problems)
  binaries("${package}" entries)
  foreach(entry IN LISTS entries)
    string(REPLACE "|" ";" fields "${entry}")
    list(GET fields 0 folder)
    list(GET fields 1 target)
    list(GET fields 2 binary)
    list_tests("${binary}" "${folder}" all)
    set(listed)
    foreach(i RANGE ${last_part})
      list(GET filters ${i} filter)
      list_tests("${binary}" "${folder}" part "${filter}")
      list(LENGTH part n)
      math(EXPR count_${i} "${count_${i}} + ${n}")
      list(APPEND listed ${part})
    endforeach()
    list_tests("${binary}" "${folder}" rest ${skips})
    list(LENGTH rest n)
    math(EXPR count_rest "${count_rest} + ${n}")
    list(APPEND listed ${rest})
    list(LENGTH all n)
    math(EXPR total "${total} + ${n}")
    # Each test exactly once among the parts: the same lists when sorted,
    # else which tests are in no part or in several.
    list(SORT all)
    list(SORT listed)
    if(NOT all STREQUAL listed)
      foreach(test IN LISTS all)
        set(times 0)
        foreach(other IN LISTS listed)
          if(other STREQUAL test)
            math(EXPR times "${times} + 1")
          endif()
        endforeach()
        if(NOT times EQUAL 1)
          list(APPEND problems "${target}: ${test} is in ${times} parts")
        endif()
      endforeach()
    endif()
  endforeach()
  set(summary)
  foreach(i RANGE ${last_part})
    list(GET filters ${i} filter)
    list(APPEND summary "${filter} ${count_${i}}")
    if(count_${i} EQUAL 0)
      list(APPEND problems "no test matches ${filter}")
    endif()
  endforeach()
  list(APPEND summary "the rest ${count_rest}")
  list(JOIN summary ", " summary)
  message("${package}: ${total} tests; ${summary}")
  if(problems)
    list(JOIN problems "\n  " problems)
    message(FATAL_ERROR "${package}'s parts:\n  ${problems}")
  endif()

else()
  message(FATAL_ERROR "usage: cmake [-D...] -P cargo-tests.cmake -- build|run|parts ...")
endif()
