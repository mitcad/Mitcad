# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A chamfer whose chain ends against a shelf's end wall (mitcad#1819, the
# owner's case rebuilt from the report): the project folder
# `ip65-1819-chamfer/` under MITCAD_MITCAD_REPORTED (never committed), its
# design found by its content id (the first 8 hex digits of its SHA-256),
# copied into WORK and opened by mitcad-cli run with
# chamfer_end_wall_reported.json: the bore's rim chamfered by 2.7 mm from
# one end wall of the band in the bore to the other builds, with no warning
# or information. Skipped when the folder is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P chamfer-end-wall-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "chamfer-end-wall-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "0aa1728e")

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/ip65-1819-chamfer")
  message(STATUS "chamfer end wall reported: skipped (no ip65-1819-chamfer under MITCAD_MITCAD_REPORTED)")
  return()
endif()
# Copied into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(COPY "${reported}/ip65-1819-chamfer/" DESTINATION "${WORK}/project")
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/project/*.mitcad")
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
  message(STATUS "chamfer end wall reported: skipped (no design ${design_id} in ip65-1819-chamfer)")
  return()
endif()
execute_process(COMMAND "${CLI}" run "${SCRIPTS}/chamfer_end_wall_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "chamfer_end_wall_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported chamfer ends on the band's end walls, without a note")
