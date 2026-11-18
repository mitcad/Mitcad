# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Offset Face on pieces of split faces (mitcad#1947) on the design of
# GitHub #396 (never committed): its archive in `github-396/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits
# of its SHA-256), the design in it by its own, unpacked into WORK. Opened
# by mitcad-cli run with offset_split_corner_reported.json: the two pieces
# that meet at a convex corner move by -1 mm and by -4.5 mm into a valid
# body of the volume the pieces' areas and the corner's length give; a
# piece of a rounding whose ends meet the next roundings at mitres moves by
# -1 mm (those roundings trimmed back, no step at the mitres) and by 1 mm
# (steps along its normal at the mitres) (mitcad#1974).
# Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P offset-split-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "offset-split-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "5174b829") # the reporter's archive
set(design_id "27cf6c58") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "offset split reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-396/*.zip")
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
  message(STATUS "offset split reported: skipped (no archive ${archive_id} in ${reported}/github-396)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/offset_split_corner_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "offset_split_corner_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the two pieces at the corner and the rounding's piece between mitres move into valid bodies of the expected volumes")
