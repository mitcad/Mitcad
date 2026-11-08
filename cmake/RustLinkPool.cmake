# SPDX-License-Identifier: MIT
# Linux debug builds: the links of the programs that take the Rust static
# library (libmitcad_ffi.a, about 2.1 GB with its split debug information)
# run in a Ninja job pool of their own, a few at a time (mitcad#1441).
#
# Each such link with GNU ld charges about 1.5 GB (the application's about
# 3 GB) to the build's cgroup, and all of them (14 programs) become ready
# at once, the moment the library is built. Under a memory limit with
# MemoryHigh (a systemd scope or slice), more links than fit were not
# killed but throttled: each ld stayed in the kernel's reclaim for over an
# hour and the build never finished. The pool bounds the links' memory to
# MITCAD_RUST_LINK_JOBS of them; the other targets' links and the compiles
# keep the build's parallelism. Windows, macOS and the release builds are
# unchanged.
include_guard(GLOBAL)

set(MITCAD_RUST_LINK_JOBS "" CACHE STRING
  "Links of programs with the Rust library at once in Linux debug builds (empty: 3; 0: no pool)")

# Every buildsystem target of DIR and its subdirectories.
function(_mitcad_directory_targets dir out)
  get_property(targets DIRECTORY "${dir}" PROPERTY BUILDSYSTEM_TARGETS)
  get_property(subdirectories DIRECTORY "${dir}" PROPERTY SUBDIRECTORIES)
  foreach(subdirectory IN LISTS subdirectories)
    _mitcad_directory_targets("${subdirectory}" sub_targets)
    list(APPEND targets ${sub_targets})
  endforeach()
  set(${out} ${targets} PARENT_SCOPE)
endfunction()

# TRUE in OUT when TARGET links the Rust static library LIBRARY, directly or
# through the targets it links (their link interfaces).
function(_mitcad_links_library target library out)
  set(seen "")
  set(queue "${target}")
  set(found FALSE)
  while(queue)
    list(POP_FRONT queue item)
    # $<LINK_ONLY:x> names x; other generator expressions are skipped.
    if(item MATCHES "^\\$<LINK_ONLY:([^>]+)>$")
      set(item "${CMAKE_MATCH_1}")
    endif()
    if(item IN_LIST seen OR NOT TARGET "${item}")
      continue()
    endif()
    list(APPEND seen "${item}")
    get_property(aliased TARGET "${item}" PROPERTY ALIASED_TARGET)
    if(aliased)
      set(item "${aliased}")
    endif()
    if(item STREQUAL library)
      set(found TRUE)
      break()
    endif()
    get_property(imported TARGET "${item}" PROPERTY IMPORTED)
    get_property(type TARGET "${item}" PROPERTY TYPE)
    if(NOT imported AND NOT type STREQUAL "INTERFACE_LIBRARY")
      get_property(links TARGET "${item}" PROPERTY LINK_LIBRARIES)
      list(APPEND queue ${links})
    endif()
    get_property(interface_links TARGET "${item}" PROPERTY INTERFACE_LINK_LIBRARIES)
    list(APPEND queue ${interface_links})
  endwhile()
  set(${out} ${found} PARENT_SCOPE)
endfunction()

# Call after every add_subdirectory: puts the link of each executable and
# shared library that links the Rust library into the pool.
function(mitcad_pool_rust_links)
  if(NOT CMAKE_SYSTEM_NAME STREQUAL "Linux" OR NOT CMAKE_GENERATOR MATCHES "Ninja"
     OR NOT CMAKE_BUILD_TYPE STREQUAL "Debug" OR NOT TARGET mitcad_ffi-static)
    return()
  endif()
  set(jobs "${MITCAD_RUST_LINK_JOBS}")
  if(jobs STREQUAL "")
    set(jobs 3)
  endif()
  if(NOT jobs MATCHES "^[0-9]+$")
    message(FATAL_ERROR "MITCAD_RUST_LINK_JOBS must be a number, not '${jobs}'")
  endif()
  if(jobs EQUAL 0)
    return()
  endif()
  set_property(GLOBAL APPEND PROPERTY JOB_POOLS mitcad_rust_link=${jobs})
  _mitcad_directory_targets("${CMAKE_SOURCE_DIR}" targets)
  set(pooled 0)
  foreach(target IN LISTS targets)
    get_property(type TARGET "${target}" PROPERTY TYPE)
    if(NOT type MATCHES "^(EXECUTABLE|SHARED_LIBRARY|MODULE_LIBRARY)$")
      continue()
    endif()
    _mitcad_links_library("${target}" mitcad_ffi-static links)
    if(links)
      set_property(TARGET "${target}" PROPERTY JOB_POOL_LINK mitcad_rust_link)
      math(EXPR pooled "${pooled} + 1")
    endif()
  endforeach()
  message(STATUS "Links with the Rust library: ${pooled} programs, ${jobs} at once (MITCAD_RUST_LINK_JOBS)")
endfunction()
