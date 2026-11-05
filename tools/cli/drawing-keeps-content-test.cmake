# SPDX-License-Identifier: MIT
# A drawing keeps what was put into it (mitcad#1293, GitHub #144): the
# script tests/drawing_keeps_content.json checks a front view of Plate
# placed twice as the 3D view's visibility and the placements change, and
# saves the design (Plate:2 hidden, a third placement shown). Its view's
# recorded subject taken out, the file is one written before views kept
# their bodies: opened, the view is pinned to what the file shows (Plate:1
# and Plate:3), and the open says so ("Drawing pinned: Drawing1 now keeps
# ...").
#
# cmake -DCLI=<mitcad-cli> -DSCRIPT=<drawing_keeps_content.json> -DWORK=<folder> -P drawing-keeps-content-test.cmake

foreach(var CLI SCRIPT WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "drawing-keeps-content-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")

function(cli)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  endif()
  set(out "${out}" PARENT_SCOPE)
endfunction()

set(design "${WORK}/plates.mitcad")
cli(run "${SCRIPT}" --save "${design}")
message(STATUS "ok   the view keeps its bodies through hiding, a new placement and a deletion")

# The file as an older MitCad wrote it: the base view without a subject.
file(READ "${design}" text)
string(JSON subject ERROR_VARIABLE missing GET "${text}" drawings 0 sheets 0 views 0 subject type)
if(NOT subject STREQUAL "bodies")
  message(FATAL_ERROR "the saved view's subject is not its bodies: ${subject} ${missing}")
endif()
string(JSON old REMOVE "${text}" drawings 0 sheets 0 views 0 subject)
set(older "${WORK}/plates-older.mitcad")
file(WRITE "${older}" "${old}")

set(check "${WORK}/pinned.json")
file(WRITE "${check}" [=[
[
  {"expect": {"query": {"query": "drawing", "drawing": "D1", "geometry": false},
    "result": {"sheets": [{"views": [
      {"id": 1, "subject": {"type": "bodies", "bodies": [{"body": "F2.b0", "occurrence": "O1"}, {"body": "F2.b0", "occurrence": "O3"}]}},
      {"id": 2}]}]}}},
  {"expect": {"query": {"query": "document"}, "result": {"undo": null}}}
]
]=])
cli(run "${check}" --open "${older}")
if(NOT out MATCHES "Drawing pinned: Drawing1 now keeps what its views showed when the design was opened \\(2 bodies\\)")
  message(FATAL_ERROR "no line of the pinned drawing in the output:\n${out}")
endif()
message(STATUS "ok   the older file's view pinned to what it showed, and said so")
