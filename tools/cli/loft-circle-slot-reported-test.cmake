# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# The owner's designs of a loft from a circle to a slot (mitcad#2141,
# mitcad#2143; never committed): the project folder
# `shell-loft-circle-slot/` under MITCAD_MITCAD_REPORTED, each design found
# by its content id (the first 8 hex digits of its SHA-256), the folder
# copied into WORK and each design opened by mitcad-cli run: the solid loft
# shelled 2 mm inside with the slot's end removed
# (shell_loft_circle_slot_reported.json), the surface loft thickened 1 mm
# to each side and symmetric (thicken_loft_circle_slot_reported.json). Both
# failed while the loft's faces met at creases along the lines between
# them. Skipped when the folder or a design is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P loft-circle-slot-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "loft-circle-slot-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(shell_id "fe1bf903") # the solid loft, the slot first
set(thicken_id "13b076ee") # the surface loft, the circle first

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/shell-loft-circle-slot")
  message(STATUS "loft circle slot reported: skipped (no shell-loft-circle-slot under MITCAD_MITCAD_REPORTED)")
  return()
endif()
# Copied into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(COPY "${reported}/shell-loft-circle-slot/" DESTINATION "${WORK}/project")
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/project/*.mitcad")
set(shell_design "")
set(thicken_design "")
foreach(candidate IN LISTS designs)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL shell_id)
    set(shell_design "${candidate}")
  elseif(id STREQUAL thicken_id)
    set(thicken_design "${candidate}")
  endif()
endforeach()
if(shell_design STREQUAL "" OR thicken_design STREQUAL "")
  message(STATUS "loft circle slot reported: skipped (no design ${shell_id} or ${thicken_id} in shell-loft-circle-slot)")
  return()
endif()
execute_process(COMMAND "${CLI}" run "${SCRIPTS}/shell_loft_circle_slot_reported.json" --open "${shell_design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "shell_loft_circle_slot_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported loft from the slot to the circle shells 2 mm inside, the slot's end open")
execute_process(COMMAND "${CLI}" run "${SCRIPTS}/thicken_loft_circle_slot_reported.json" --open "${thicken_design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "thicken_loft_circle_slot_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported surface loft from the circle to the slot thickens 1 mm to each side and symmetric")
