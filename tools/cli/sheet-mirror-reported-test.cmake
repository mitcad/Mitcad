# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Mirrors of sheet metal bodies (mitcad#1450) on the reported design of
# GitHub #188 (a sheet metal face folded twice and mirrored; never
# committed): `github-188-*.mitcad` under MITCAD_MITCAD_REPORTED. As saved,
# its mirror's copy is a valid sheet that a pick names and a Fold folds
# (sheet_mirror_reported.json); with the mirror joining the copy, the part is
# one valid sheet that later folds fold and whose Flat Pattern unfolds the
# whole part (sheet_mirror_reported_join.json). Before the fix the copy's
# sheet was invalid ("a generic copy or split has no sheet-domain adapter")
# and the joined sheet too ("material added"). Skipped when the folder or
# the design is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P sheet-mirror-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "sheet-mirror-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "sheet mirror reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB designs LIST_DIRECTORIES false "${reported}/github-188-*.mitcad")
list(LENGTH designs n)
if(n EQUAL 0)
  message(STATUS "sheet mirror reported: skipped (no github-188 design in ${reported})")
  return()
endif()
list(GET designs 0 source)

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(design "${WORK}/reported.mitcad")
file(COPY_FILE "${source}" "${design}")

foreach(script sheet_mirror_reported sheet_mirror_reported_join)
  execute_process(COMMAND "${CLI}" run "${SCRIPTS}/${script}.json" --open "${design}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "${script} failed (${status}):\n${out}${err}")
  endif()
  message(STATUS "ok   ${script}")
endforeach()
