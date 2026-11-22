# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# GitHub #527's design (mitcad#2174, the reporting design of mitcad#2147;
# never committed): in `github-527/` under MITCAD_MITCAD_REPORTED, found by
# its content id (the first 8 hex digits of its SHA-256) and copied into
# WORK. Its root has sketches and suppressed placeholders only; its one
# component, a cylinder 80 mm across and 60 mm long, is placed six times,
# each placement turning the cylinder's axis along x. export-drawing's top
# view as the reporter wrote it (--views top --no-dimensions
# --no-hole-table, the sheet and the scale chosen by the export) draws each
# placement where the design puts it, a 60 x 80 mm outline whose corners
# are drawn points of the view, and prints "1 bodies in 6 placements".
# With the sixth occurrence hidden, its outline is left out and the line
# says "1 bodies in 5 placements". Skipped when the folder or the design
# is not there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P drawing-placements-reported-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "drawing-placements-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "1fd08fa4") # the reporter's design

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-527")
  message(STATUS "drawing placements reported: skipped (MITCAD_MITCAD_REPORTED/github-527 is not there)")
  return()
endif()
file(GLOB designs LIST_DIRECTORIES false "${reported}/github-527/*.mitcad")
set(found "")
foreach(candidate IN LISTS designs)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL design_id)
    set(found "${candidate}")
    break()
  endif()
endforeach()
if(found STREQUAL "")
  message(STATUS "drawing placements reported: skipped (no design ${design_id} in ${reported}/github-527)")
  return()
endif()

# A copy in WORK, so that nothing is written beside the reported file.
file(MAKE_DIRECTORY "${WORK}")
set(design "${WORK}/design.mitcad")
file(COPY_FILE "${found}" "${design}")

function(cli)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  endif()
  set(out "${out}" PARENT_SCOPE)
endfunction()

# micro(<text> <var>): a coordinate of the SVG (three decimals at most) in
# micrometres, an integer for math(EXPR).
function(micro text var)
  if(NOT text MATCHES "^(-?)([0-9]*)(\\.([0-9]*))?$")
    message(FATAL_ERROR "not a coordinate: ${text}")
  endif()
  set(sign "${CMAKE_MATCH_1}")
  set(whole "${CMAKE_MATCH_2}")
  set(fraction "${CMAKE_MATCH_4}000")
  string(SUBSTRING "${fraction}" 0 3 fraction)
  string(REGEX REPLACE "^0+" "" whole "${whole}")
  string(REGEX REPLACE "^0+" "" fraction "${fraction}")
  if(whole STREQUAL "")
    set(whole 0)
  endif()
  if(fraction STREQUAL "")
    set(fraction 0)
  endif()
  math(EXPR value "${whole} * 1000 + ${fraction}")
  if(sign STREQUAL "-")
    math(EXPR value "0 - ${value}")
  endif()
  set(${var} "${value}" PARENT_SCOPE)
endfunction()

# view_points(<svg> <xs> <ys>): the points the SVG's top view draws (the
# ends of its lines and arcs, the points of its polylines), each once, in
# micrometres on the sheet (SVG's y running down).
function(view_points svg xs_var ys_var)
  file(READ "${svg}" text)
  string(FIND "${text}" "id=\"view-top\">" start)
  if(start LESS 0)
    message(FATAL_ERROR "${svg}: no top view")
  endif()
  string(SUBSTRING "${text}" ${start} -1 view)
  string(FIND "${view}" "</g>" end)
  string(SUBSTRING "${view}" 0 ${end} view)
  set(pairs "")
  set(number "-?[0-9.]+")
  string(REGEX MATCHALL "x1=\"${number}\" y1=\"${number}\" x2=\"${number}\" y2=\"${number}\"" lines "${view}")
  foreach(line IN LISTS lines)
    string(REGEX MATCH "x1=\"(${number})\" y1=\"(${number})\" x2=\"(${number})\" y2=\"(${number})\"" _ "${line}")
    list(APPEND pairs "${CMAKE_MATCH_1},${CMAKE_MATCH_2}" "${CMAKE_MATCH_3},${CMAKE_MATCH_4}")
  endforeach()
  string(REGEX MATCHALL "points=\"[-0-9., ]+\"" polylines "${view}")
  foreach(polyline IN LISTS polylines)
    string(REGEX MATCHALL "${number},${number}" points "${polyline}")
    list(APPEND pairs ${points})
  endforeach()
  string(REGEX MATCHALL "d=\"M ${number} ${number} A ${number} ${number} 0 [01] 0 ${number} ${number}\"" arcs "${view}")
  foreach(arc IN LISTS arcs)
    string(REGEX MATCH "M (${number}) (${number}) A ${number} ${number} 0 [01] 0 (${number}) (${number})" _ "${arc}")
    list(APPEND pairs "${CMAKE_MATCH_1},${CMAKE_MATCH_2}" "${CMAKE_MATCH_3},${CMAKE_MATCH_4}")
  endforeach()
  list(REMOVE_DUPLICATES pairs)
  if(pairs STREQUAL "")
    message(FATAL_ERROR "${svg}: the top view draws nothing")
  endif()
  set(xs "")
  set(ys "")
  foreach(pair IN LISTS pairs)
    string(REPLACE "," ";" xy "${pair}")
    list(GET xy 0 x)
    list(GET xy 1 y)
    micro("${x}" x)
    micro("${y}" y)
    list(APPEND xs "${x}")
    list(APPEND ys "${y}")
  endforeach()
  set(${xs_var} "${xs}" PARENT_SCOPE)
  set(${ys_var} "${ys}" PARENT_SCOPE)
endfunction()

# The placements' origins in the design (x,y in mm, the occurrences' order):
# each outline runs from x to x + 60 and from y - 40 to y + 40, so that the
# whole runs from x 1000 to 6060 and from y -40 to 2040.
set(origins "1000,0" "2500,0" "4000,0" "6000,1500" "1000,2000" "3000,2000")

# outlines(<svg> <out> <shown> <left out>): the top view of <svg>, at the
# scale <out> printed, draws the four corners of each placement numbered
# in <shown> (1 to 6) and none of those in <left out>.
function(outlines svg out shown left_out)
  if(NOT out MATCHES ", ([0-9]+):([0-9]+)\n")
    message(FATAL_ERROR "no scale in: ${out}")
  endif()
  # Sheet micrometres per design millimetre: 1000 * a / b at a:b.
  set(a "${CMAKE_MATCH_1}")
  set(b "${CMAKE_MATCH_2}")
  # Within 2 mm of the design (and the SVG's rounding) of where it belongs;
  # the placements' corners are 60 mm apart at the least.
  math(EXPR tolerance "2000 * ${a} / ${b} + 20")
  view_points("${svg}" xs ys)
  # The view's box: its left side is the design's x 1000, its top (SVG's
  # least y) the design's y 2040.
  set(left "")
  set(right "")
  set(top "")
  set(bottom "")
  foreach(x IN LISTS xs)
    if(left STREQUAL "" OR x LESS left)
      set(left "${x}")
    endif()
    if(right STREQUAL "" OR x GREATER right)
      set(right "${x}")
    endif()
  endforeach()
  foreach(y IN LISTS ys)
    if(top STREQUAL "" OR y LESS top)
      set(top "${y}")
    endif()
    if(bottom STREQUAL "" OR y GREATER bottom)
      set(bottom "${y}")
    endif()
  endforeach()
  math(EXPR width "${right} - ${left}")
  math(EXPR height "${bottom} - ${top}")
  math(EXPR want_width "5060 * 1000 * ${a} / ${b}")
  math(EXPR want_height "2080 * 1000 * ${a} / ${b}")
  math(EXPR off_width "${width} - ${want_width}")
  math(EXPR off_height "${height} - ${want_height}")
  if(off_width GREATER tolerance OR off_width LESS -${tolerance}
     OR off_height GREATER tolerance OR off_height LESS -${tolerance})
    message(FATAL_ERROR "${svg}: the top view is ${width} x ${height} um on the sheet at ${a}:${b}, "
                        "not the ${want_width} x ${want_height} um of the six placements (5060 x 2080 mm)")
  endif()
  list(LENGTH xs count)
  math(EXPR last "${count} - 1")
  set(number 0)
  foreach(origin IN LISTS origins)
    math(EXPR number "${number} + 1")
    string(REPLACE "," ";" origin "${origin}")
    list(GET origin 0 ox)
    list(GET origin 1 oy)
    set(drawn 0)
    foreach(corner "0,-40" "0,40" "60,-40" "60,40")
      string(REPLACE "," ";" corner "${corner}")
      list(GET corner 0 cx)
      list(GET corner 1 cy)
      math(EXPR mx "${ox} + ${cx}")
      math(EXPR my "${oy} + ${cy}")
      math(EXPR sx "${left} + (${mx} - 1000) * 1000 * ${a} / ${b}")
      math(EXPR sy "${top} + (2040 - ${my}) * 1000 * ${a} / ${b}")
      set(hit FALSE)
      foreach(i RANGE ${last})
        list(GET xs ${i} x)
        list(GET ys ${i} y)
        math(EXPR dx "${x} - ${sx}")
        math(EXPR dy "${y} - ${sy}")
        if(NOT dx GREATER tolerance AND NOT dx LESS -${tolerance}
           AND NOT dy GREATER tolerance AND NOT dy LESS -${tolerance})
          set(hit TRUE)
          break()
        endif()
      endforeach()
      if(hit)
        math(EXPR drawn "${drawn} + 1")
      endif()
      if(number IN_LIST shown AND NOT hit)
        message(FATAL_ERROR "${svg}: placement ${number} is not drawn at its place: "
                            "no point of the top view at its corner (${mx}, ${my}) mm")
      endif()
    endforeach()
    if(number IN_LIST left_out AND drawn GREATER 0)
      message(FATAL_ERROR "${svg}: the hidden placement ${number} is drawn (${drawn} of its corners)")
    endif()
  endforeach()
endfunction()

set(options --views top --no-dimensions --no-hole-table)

# As the reporter wrote it: every placement drawn.
set(svg "${WORK}/top.svg")
cli(export-drawing "${design}" "${svg}" ${options})
if(NOT out MATCHES "Exported [^\n]*top\\.svg: drawing, 1 views, 1 bodies in 6 placements, ")
  message(FATAL_ERROR "unexpected output: ${out}")
endif()
outlines("${svg}" "${out}" "1;2;3;4;5;6" "")
message(STATUS "ok   the six placements drawn at their places, 1 bodies in 6 placements")

# The sixth occurrence hidden: left out of the view and the count.
set(script "${WORK}/hide.json")
file(WRITE "${script}" [=[
[{"cmd": "set_occurrence_visible", "occurrence": "Collar:6", "visible": false}]
]=])
set(hidden "${WORK}/hidden.mitcad")
cli(run "${script}" --open "${design}" --save "${hidden}")
set(svg "${WORK}/hidden.svg")
cli(export-drawing "${hidden}" "${svg}" ${options})
if(NOT out MATCHES "Exported [^\n]*hidden\\.svg: drawing, 1 views, 1 bodies in 5 placements, ")
  message(FATAL_ERROR "unexpected output: ${out}")
endif()
outlines("${svg}" "${out}" "1;2;3;4;5" "6")
message(STATUS "ok   the hidden placement left out, 1 bodies in 5 placements")
