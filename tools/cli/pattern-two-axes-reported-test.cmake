# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Circular Pattern about two axes (mitcad#1588) on the reported cubic
# lattice (never committed): `pattern-two-axes/lattice-cube.mitcad` under
# MITCAD_MITCAD_REPORTED. As saved it opens with its two circular patterns
# and its fillet as before; with the centre ball in Revolve1, one pattern
# about X and Z of Revolve1 and Pipe1 and Fillet1 on all 40 joints it is
# one body whose fillet works (pattern_two_axes_reported.json). Skipped
# when the folder or the design is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P pattern-two-axes-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "pattern-two-axes-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "pattern two axes reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
set(source "${reported}/pattern-two-axes/lattice-cube.mitcad")
if(NOT EXISTS "${source}")
  message(STATUS "pattern two axes reported: skipped (no pattern-two-axes design in ${reported})")
  return()
endif()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(design "${WORK}/reported.mitcad")
file(COPY_FILE "${source}" "${design}")

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/pattern_two_axes_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "pattern_two_axes_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   pattern_two_axes_reported")
