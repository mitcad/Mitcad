# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Mirror and patterns of a scoped Extrude Cut (mitcad#2126, GitHub #495) on
# a reported design (never committed): its archive in `github-495/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits
# of its SHA-256), the design in it by its own (the one with two
# components), unpacked into WORK. Opened by mitcad-cli run with
# scoped_cut_mirror_reported.json: a pocket cut with an automatic target
# scope in one component, beside another component's body it does not
# reach, is mirrored and patterned in its own component, and the other
# body stays as it was. Before the fix both failed because the other
# component has a body. Skipped when the folder or the archive is not
# there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P scoped-cut-mirror-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "scoped-cut-mirror-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "2d8219e2") # the reporter's archive
set(design_id "9a31c2f4") # the design in it with two components

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "scoped cut mirror reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-495/*.zip")
set(archive "")
foreach(candidate IN LISTS archives)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL archive_id)
    set(archive "${candidate}")
    break()
  endif()
endforeach()
if(archive STREQUAL "")
  message(STATUS "scoped cut mirror reported: skipped (no archive ${archive_id} in ${reported}/github-495)")
  return()
endif()

# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/unpacked")
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/unpacked/*.mitcad")
set(design "")
foreach(candidate IN LISTS designs)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL design_id)
    set(design "${candidate}")
    break()
  endif()
endforeach()
if(design STREQUAL "")
  message(FATAL_ERROR "the archive ${archive_id} holds no design ${design_id}")
endif()

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/scoped_cut_mirror_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "scoped_cut_mirror_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the scoped pocket is mirrored and patterned in its own component; the other component's body is unchanged")
