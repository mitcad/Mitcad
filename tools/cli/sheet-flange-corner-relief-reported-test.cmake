# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Sheet Rules' Corner Relief at the corners of a flange on four edges
# (mitcad#2411, mitcad#2424) on the reported design of GitHub #614 (never
# committed): stored as it is in `github-614/` under MITCAD_MITCAD_REPORTED,
# found by its content id (the first 8 hex digits of its SHA-256) and copied
# into WORK. Opened by mitcad-cli run with
# sheet_flange_corner_relief_reported.json: as saved (rules without the
# flag) the bends end short of the corners; with the rules edited to
# Rectangular, Round and Tear 1 mm the corners differ and every flat pattern
# is valid. Skipped when the folder or the design is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P sheet-flange-corner-relief-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "sheet-flange-corner-relief-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "8815659c") # the report's attachment

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "sheet flange corner relief reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB stored LIST_DIRECTORIES false "${reported}/github-614/*.mitcad")
set(found "")
foreach(candidate IN LISTS stored)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL design_id)
    set(found "${candidate}")
    break()
  endif()
endforeach()
if(found STREQUAL "")
  message(STATUS "sheet flange corner relief reported: skipped (no design ${design_id} in ${reported}/github-614)")
  return()
endif()

# Copied into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
get_filename_component(name "${found}" NAME)
file(COPY_FILE "${found}" "${WORK}/${name}")

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/sheet_flange_corner_relief_reported.json" --open "${WORK}/${name}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "sheet_flange_corner_relief_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported design's corners differ for Rectangular, Round and Tear, each flat pattern valid")
