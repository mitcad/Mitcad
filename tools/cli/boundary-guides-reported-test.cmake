# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Boundary Surface's Fill a Loop through guide curves (mitcad#2526) on the
# reported design of GitHub #711 (never committed): its archive in
# `github-711/` under MITCAD_MITCAD_REPORTED, found by its content id (the
# first 8 hex digits of its SHA-256), the design in it by its own, unpacked
# into WORK. Opened by mitcad-cli run with boundary_guides_reported.json:
# the bowl's rim of seven arcs filled flat without guides and through the
# four depth curves inside it with them, the guided face rebuilt when a
# guide's sketch changes. Skipped when the folder or the archive is not
# there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P boundary-guides-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "boundary-guides-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "b9f39c91") # the reporter's archive
set(design_id "f603fb13") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "boundary guides reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-711/*.zip")
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
  message(STATUS "boundary guides reported: skipped (no archive ${archive_id} in ${reported}/github-711)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/boundary_guides_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "boundary_guides_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported rim filled through its four guide curves, rebuilt when a guide changes")
