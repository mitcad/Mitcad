# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Extend of a planar surface body's slanted edge (mitcad#2056, GitHub #462)
# on a reported design (never committed): its archive in `github-462/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits
# of its SHA-256), the design in it by its own, unpacked into WORK. Opened
# by mitcad-cli run with extend_slanted_plane_reported.json: the slanted
# end edge extended by 26 gives one planar face whose two long edges are
# each one straight edge and whose new end edge is parallel to the picked
# one. Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P extend-slanted-plane-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "extend-slanted-plane-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "03242313") # the reporter's archive
set(design_id "e2008371") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "extend slanted plane reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-462/*.zip")
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
  message(STATUS "extend slanted plane reported: skipped (no archive ${archive_id} in ${reported}/github-462)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/extend_slanted_plane_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "extend_slanted_plane_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported slanted edge extends along the long edges to an end edge parallel to it")
