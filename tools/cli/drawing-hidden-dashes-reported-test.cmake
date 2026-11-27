# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# GitHub #695's design (mitcad#2477; never committed): its archive in
# `github-695/` under MITCAD_MITCAD_REPORTED and the design in it, each
# found by its content id (the first 8 hex digits of its SHA-256), unpacked
# into WORK. A shelled body with one drawing: a base view from the back
# with hidden edges, two orthographic views projected from it (taking its
# style) and an isometric view projected from a corner with tangent edges.
# Its sheet as SVG, what File > Print prints (export-drawing --drawing D1):
#   - hidden edges drawn in one to three views (the isometric one has
#     none), every one with the pen table's hidden pen: 0.18 mm wide,
#     3 mm dashes 1 mm apart, round caps;
#   - no dashed circle element (a dashed circle is a path from its start,
#     counter-clockwise on paper, as the screen and the PDF draw it);
#   - the visible outlines (0.35 mm) continuous.
# With the base view's Hidden Edges cleared (edit_drawing_view, style
# hidden_lines false) no view draws a hidden edge, the visible outlines
# still drawn, continuous. Skipped when the folder, the archive or the design is
# not there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P drawing-hidden-dashes-reported-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "drawing-hidden-dashes-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "bdd79499") # the reporter's archive
set(design_id "967e5ff2") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-695")
  message(STATUS "drawing hidden dashes reported: skipped (MITCAD_MITCAD_REPORTED/github-695 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-695/*.zip")
set(archive "")
foreach(candidate IN LISTS archives)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL archive_id)
    set(archive "${candidate}")
    break()
  endif()
endforeach()
if(archive STREQUAL "")
  message(STATUS "drawing hidden dashes reported: skipped (no archive ${archive_id} in ${reported}/github-695)")
  return()
endif()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}/unpacked")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/unpacked")
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/unpacked/*.mitcad")
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
  message(FATAL_ERROR "the reported archive holds no design ${design_id}")
endif()

function(cli)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  endif()
endfunction()

# sheet_counts(<svg> <prefix>): of the SVG's view groups, sets
# <prefix>_hidden_views (how many draw a hidden edge), <prefix>_hidden (the
# hidden edges), <prefix>_visible (the visible outlines, 0.35 mm), and
# fails on a hidden edge with another pen, a dashed circle element or a
# dashed visible outline.
function(sheet_counts svg prefix)
  file(READ "${svg}" text)
  # One list item per group (the writer's groups do not nest), from its
  # start to the next one's; of the view groups, what comes before its end.
  string(REPLACE ";" "," text "${text}")
  string(REPLACE "<g " ";<g " pieces "${text}")
  set(groups "")
  foreach(piece IN LISTS pieces)
    if(piece MATCHES "^<g class=\"view\"")
      string(FIND "${piece}" "</g>" end)
      string(SUBSTRING "${piece}" 0 ${end} group)
      list(APPEND groups "${group}")
    endif()
  endforeach()
  list(LENGTH groups count)
  if(count LESS 4)
    message(FATAL_ERROR "${svg}: ${count} view groups, the drawing has 4 views")
  endif()
  set(hidden_views 0)
  set(hidden 0)
  set(visible 0)
  foreach(group IN LISTS groups)
    string(REGEX MATCHALL "<[a-z]+ [^>]*stroke-dasharray=\"3 1\"[^>]*>" dashed "${group}")
    list(LENGTH dashed n)
    if(n GREATER 0)
      math(EXPR hidden_views "${hidden_views} + 1")
      math(EXPR hidden "${hidden} + ${n}")
    endif()
    foreach(item IN LISTS dashed)
      if(NOT item MATCHES "stroke-width=\"0\\.18\"" OR NOT item MATCHES "stroke-linecap=\"round\"")
        message(FATAL_ERROR "${svg}: a hidden edge not drawn with the hidden pen (0.18 mm, round caps): ${item}")
      endif()
      if(item MATCHES "^<circle ")
        message(FATAL_ERROR "${svg}: a dashed circle element (its dashes would start at 0 degrees, "
                            "clockwise on paper): ${item}")
      endif()
    endforeach()
    string(REGEX MATCHALL "<[a-z]+ [^>]*stroke-width=\"0\\.35\"[^>]*>" outlines "${group}")
    foreach(item IN LISTS outlines)
      if(item MATCHES "stroke-dasharray")
        message(FATAL_ERROR "${svg}: a visible outline dashed: ${item}")
      endif()
    endforeach()
    list(LENGTH outlines n)
    math(EXPR visible "${visible} + ${n}")
  endforeach()
  set(${prefix}_hidden_views "${hidden_views}" PARENT_SCOPE)
  set(${prefix}_hidden "${hidden}" PARENT_SCOPE)
  set(${prefix}_visible "${visible}" PARENT_SCOPE)
endfunction()

set(svg "${WORK}/sheet.svg")
cli(export-drawing "${design}" "${svg}" --drawing D1)
sheet_counts("${svg}" shown)
if(shown_hidden_views LESS 1 OR shown_hidden_views GREATER 3)
  message(FATAL_ERROR "${svg}: ${shown_hidden_views} views draw hidden edges; the base view and the two "
                      "orthographic views projected from it may, the isometric view does not")
endif()
if(shown_visible LESS 1)
  message(FATAL_ERROR "${svg}: no visible outline")
endif()
message(STATUS "ok   ${shown_hidden} hidden edges in ${shown_hidden_views} views, all 0.18 mm with 3 mm dashes "
               "1 mm apart; ${shown_visible} visible outlines continuous")

# The base view's Hidden Edges cleared: none in any view.
set(script "${WORK}/no-hidden.json")
file(WRITE "${script}" [=[
[{"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "style": {"hidden_lines": false}}]
]=])
set(cleared "${WORK}/no-hidden.mitcad")
cli(run "${script}" --open "${design}" --save "${cleared}")
set(svg "${WORK}/no-hidden.svg")
cli(export-drawing "${cleared}" "${svg}" --drawing D1)
sheet_counts("${svg}" cleared)
if(NOT cleared_hidden EQUAL 0)
  message(FATAL_ERROR "${svg}: ${cleared_hidden} hidden edges with the base view's Hidden Edges cleared")
endif()
if(cleared_visible LESS 1)
  message(FATAL_ERROR "${svg}: no visible outline with Hidden Edges cleared")
endif()
message(STATUS "ok   Hidden Edges cleared: no hidden edge, ${cleared_visible} visible outlines continuous "
               "(${shown_visible} with them)")
