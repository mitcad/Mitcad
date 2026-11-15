# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Rib Extend Ends from a boss's top edge (mitcad#1857, GitHub #345) on a
# reported design (never committed): its archive in `github-345/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits
# of its SHA-256), the design in it by its own, unpacked into WORK. Opened
# by mitcad-cli run with rib_extend_rim_reported.json: the reported rib,
# whose start continues over the higher boss's top, fills up to the wall
# below the edge without a warning and joins the body. Skipped when the
# folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P rib-extend-rim-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "rib-extend-rim-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "af3b839a") # the reporter's archive
set(design_id "66b32464") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "rib extend rim reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-345/*.zip")
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
  message(STATUS "rib extend rim reported: skipped (no archive ${archive_id} in ${reported}/github-345)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/rib_extend_rim_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "rib_extend_rim_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported rib fills up to the higher boss's wall without a warning and joins the body")
