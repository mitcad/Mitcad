# SPDX-License-Identifier: MIT
# A laid-out drawing of an imported assembly (mitcad#2147, GitHub #527):
# tests/drawing_assembly.solidcraft.json, a root block and a component
# placed four times along x, imported and saved; export-drawing's top view
# draws the block and each placement (the view's upright lines: two per
# box) and prints "2 bodies in 5 placements". With Saddle:3 hidden, the
# view leaves it out and the line says "2 bodies in 4 placements".
#
# cmake -DCLI=<mitcad-cli> -DDESIGN=<drawing_assembly.solidcraft.json> -DWORK=<folder> -P drawing-assembly-test.cmake

foreach(var CLI DESIGN WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "drawing-assembly-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(MAKE_DIRECTORY "${WORK}")

function(cli)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  endif()
  set(out "${out}" PARENT_SCOPE)
endfunction()

# upright(<svg> <var>): the x coordinates of the lines of the SVG's top view
# that run along the sheet's y axis, each once.
function(upright svg var)
  file(READ "${svg}" text)
  string(FIND "${text}" "id=\"view-top\">" start)
  if(start LESS 0)
    message(FATAL_ERROR "${svg}: no top view")
  endif()
  string(SUBSTRING "${text}" ${start} -1 view)
  string(FIND "${view}" "</g>" end)
  string(SUBSTRING "${view}" 0 ${end} view)
  set(xs "")
  string(REGEX MATCHALL "<line x1=\"[-0-9.]+\" y1=\"[-0-9.]+\" x2=\"[-0-9.]+\" y2=\"[-0-9.]+\"" lines "${view}")
  foreach(line IN LISTS lines)
    string(REGEX MATCH "x1=\"([-0-9.]+)\" y1=\"([-0-9.]+)\" x2=\"([-0-9.]+)\" y2=\"([-0-9.]+)\"" _ "${line}")
    if(CMAKE_MATCH_1 STREQUAL CMAKE_MATCH_3 AND NOT CMAKE_MATCH_2 STREQUAL CMAKE_MATCH_4)
      list(APPEND xs "${CMAKE_MATCH_1}")
    endif()
  endforeach()
  string(REGEX MATCHALL "<polyline points=\"[-0-9.]+,[-0-9.]+ [-0-9.]+,[-0-9.]+\"" lines "${view}")
  foreach(line IN LISTS lines)
    string(REGEX MATCH "\"([-0-9.]+),([-0-9.]+) ([-0-9.]+),([-0-9.]+)\"" _ "${line}")
    if(CMAKE_MATCH_1 STREQUAL CMAKE_MATCH_3 AND NOT CMAKE_MATCH_2 STREQUAL CMAKE_MATCH_4)
      list(APPEND xs "${CMAKE_MATCH_1}")
    endif()
  endforeach()
  list(REMOVE_DUPLICATES xs)
  set(${var} "${xs}" PARENT_SCOPE)
endfunction()

set(design "${WORK}/assembly.mitcad")
cli(import-solidcraft "${DESIGN}" --save "${design}")
set(options --sheet A3 --views top --scale 1:10 --no-dimensions --no-hole-table)

set(svg "${WORK}/assembly.svg")
cli(export-drawing "${design}" "${svg}" ${options})
if(NOT out MATCHES "Exported [^\n]*assembly\\.svg: drawing, 1 views, 2 bodies in 5 placements, A3, 1:10")
  message(FATAL_ERROR "unexpected output: ${out}")
endif()
upright("${svg}" xs)
list(LENGTH xs count)
if(NOT count EQUAL 10)
  message(FATAL_ERROR "the top view's upright lines are at ${xs}, not at the sides of the block and four placements")
endif()
message(STATUS "ok   the block and the four placements drawn, 2 bodies in 5 placements")

# Saddle:3 hidden.
set(script "${WORK}/hide.json")
file(WRITE "${script}" [=[
[{"cmd": "set_occurrence_visible", "occurrence": "Saddle:3", "visible": false}]
]=])
set(hidden "${WORK}/hidden.mitcad")
cli(run "${script}" --open "${design}" --save "${hidden}")
set(svg "${WORK}/hidden.svg")
cli(export-drawing "${hidden}" "${svg}" ${options})
if(NOT out MATCHES "Exported [^\n]*hidden\\.svg: drawing, 1 views, 2 bodies in 4 placements, A3, 1:10")
  message(FATAL_ERROR "unexpected output: ${out}")
endif()
upright("${svg}" xs)
list(LENGTH xs count)
if(NOT count EQUAL 8)
  message(FATAL_ERROR "the top view's upright lines are at ${xs}, not at the sides of the block and three placements")
endif()
message(STATUS "ok   the hidden placement left out, 2 bodies in 4 placements")
