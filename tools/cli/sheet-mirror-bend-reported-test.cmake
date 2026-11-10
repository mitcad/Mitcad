# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A sheet joined with its mirror image where a bend crosses the mirror plane
# (mitcad#1558) on the reported design of GitHub #209 (a face folded twice;
# never committed): the archive in `github-209-*/` under
# MITCAD_MITCAD_REPORTED, its design unpacked into WORK. Mirrored with Join
# about the wall its second fold's bend crosses, and the mirror as a new
# body joined with Combine, the part is one valid sheet each time
# (sheet_mirror_bend_reported.json). Skipped when the folder or the archive
# is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P sheet-mirror-bend-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "sheet-mirror-bend-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "sheet mirror bend reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-209-*/*.zip")
list(LENGTH archives n)
if(n EQUAL 0)
  message(STATUS "sheet mirror bend reported: skipped (no github-209 design in ${reported})")
  return()
endif()
list(GET archives 0 archive)

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}")
file(GLOB designs LIST_DIRECTORIES false "${WORK}/*.mitcad")
list(LENGTH designs n)
if(NOT n EQUAL 1)
  message(FATAL_ERROR "the github-209 archive holds ${n} designs, not one")
endif()
list(GET designs 0 design)

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/sheet_mirror_bend_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "sheet_mirror_bend_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   sheet_mirror_bend_reported")
