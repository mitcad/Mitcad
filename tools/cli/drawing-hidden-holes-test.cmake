# SPDX-License-Identifier: MIT
# A view's automatic centre marks only on the holes it draws (mitcad#1286):
# the bar tests/drawing_cross_holes.json makes (19 x 19 x 180, six M5
# tapped holes through it from one side and five from the side next to it,
# each a linear pattern), its drawing D1 written as SVG by export-drawing.
# The front and the right view each see one row of holes end on, a centre
# mark on each (6 + 5); the other row lies across them inside the bar,
# hidden, and the top view (the bar's end) sees both rows so. Before the
# fix, those hidden holes' centrelines crossed the bar between the holes
# seen end on and ran past its outline (5 + 6 more), and the end view had a
# cross (2 more): 24 marks instead of 11.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPT=<drawing_cross_holes.json> -DWORK=<folder> -P drawing-hidden-holes-test.cmake

foreach(var CLI SCRIPT WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "drawing-hidden-holes-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")

function(cli)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  endif()
endfunction()

set(design "${WORK}/cross-holes.mitcad")
cli(run "${SCRIPT}" --save "${design}")
set(svg "${WORK}/cross-holes.svg")
cli(export-drawing "${design}" "${svg}" --drawing D1)
file(READ "${svg}" text)

string(REGEX MATCHALL "<g class=\"view\"" views "${text}")
list(LENGTH views n)
if(NOT n EQUAL 3)
  message(FATAL_ERROR "views in the SVG: ${n}, expected 3")
endif()
message(STATUS "ok   views in the SVG: 3")

string(REGEX MATCHALL "<g class=\"center-mark\"" marks "${text}")
list(LENGTH marks n)
if(NOT n EQUAL 11)
  message(FATAL_ERROR "centre marks in the SVG: ${n}, expected 11 (one on each hole seen end on, "
                      "none on the holes hidden inside the bar)")
endif()
message(STATUS "ok   centre marks: 11, on the holes seen end on only")
