# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Three tangent-chained edges round a pocket's rim rounded with 1.5, on
# GitHub #620's design (mitcad#2352; never committed): the archive in
# `github-620/` under MITCAD_MITCAD_REPORTED, found by its content id (the
# first 8 hex digits of its SHA-256), the design in it by its own, unpacked
# into WORK and opened by mitcad-cli run with fillet_rim_corner_reported.json:
# the design's suppressed Fillet14 turned on builds one closed solid without
# a note, its rounding at 1.5, the volume pinned (0.71.0 refused it: "the
# fillet is wider than the face beside its edge and would take that face
# away, but cannot meet the face beyond it, at its corner with the bevel of
# 1 more edge"); Fillet15 after it as well. Skipped when the folder or the
# archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-rim-corner-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-rim-corner-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "ea1e6fd3") # the reporter's archive
set(design_id "ca57e453") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-620")
  message(STATUS "fillet rim corner reported: skipped (MITCAD_MITCAD_REPORTED/github-620 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-620/*.zip")
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
  message(STATUS "fillet rim corner reported: skipped (no archive ${archive_id} in ${reported}/github-620)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/fillet_rim_corner_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "fillet_rim_corner_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "${out}")
message(STATUS "ok   the pocket's rim rounded with 1.5 past the narrow face at its corner, one closed solid without a note")
