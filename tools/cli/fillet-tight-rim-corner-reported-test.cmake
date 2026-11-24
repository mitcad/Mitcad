# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A top's edge rounded with 3 past a cut-out's rim rounded with 2, the
# cut-out's axis in the top's plane, on GitHub #592's design (mitcad#2291;
# never committed): the archive in `github-592/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits
# of its SHA-256), the design in it by its own, unpacked into WORK and
# opened by mitcad-cli run with fillet_tight_rim_corner_reported.json: the
# roundings join at the rim's own ball (the port's patch 2291), one valid
# solid without a warning and no face under 0.01 mm2, and with 2 for both
# as clean as before. Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-tight-rim-corner-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-tight-rim-corner-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "d970e871") # the reporter's archive
set(design_id "7dd175ed") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-592")
  message(STATUS "fillet tight rim corner reported: skipped (MITCAD_MITCAD_REPORTED/github-592 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-592/*.zip")
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
  message(STATUS "fillet tight rim corner reported: skipped (no archive ${archive_id} in ${reported}/github-592)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/fillet_tight_rim_corner_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "fillet_tight_rim_corner_reported failed (${status}):\n${out}${err}")
endif()
# (The report of the design as the script leaves it, with Fillet2 of 2: the
# log shows that the design was opened and recomputed.)
message(STATUS "${out}")
message(STATUS "ok   the top's rounding of 3 joins the rim's rounding of 2 at the rim's own ball, one valid solid without a warning")
