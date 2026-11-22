# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# An Extrude Cut up to a face behind its profiles (mitcad#2118) on the
# reported design of GitHub mitcad/Mitcad#492 (never committed): the zip of
# its report under MITCAD_MITCAD_REPORTED/github-492, unpacked here. Three
# slots sketched on the top face of an L-shaped body's leg are cut up to the
# leg's bottom face; the sketch's normal points out of the leg, so the cut
# turns round to reach the face (extrude_cut_reported.json). Before the fix
# it failed with "the target lies behind the start of the sweep". Skipped
# when the folder or the zip is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P extrude-cut-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "extrude-cut-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "extrude cut reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB zips LIST_DIRECTORIES false "${reported}/github-492/*.zip")
list(LENGTH zips n)
if(n EQUAL 0)
  message(STATUS "extrude cut reported: skipped (no github-492 report in ${reported})")
  return()
endif()
list(GET zips 0 zip)

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}/unpacked")
file(ARCHIVE_EXTRACT INPUT "${zip}" DESTINATION "${WORK}/unpacked")
file(GLOB designs LIST_DIRECTORIES false "${WORK}/unpacked/*.mitcad")
list(LENGTH designs n)
if(NOT n EQUAL 1)
  message(FATAL_ERROR "extrude cut reported: ${n} designs in ${zip}, expected one")
endif()
list(GET designs 0 source)
set(design "${WORK}/reported.mitcad")
file(COPY_FILE "${source}" "${design}")

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/extrude_cut_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "extrude_cut_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   extrude_cut_reported")
