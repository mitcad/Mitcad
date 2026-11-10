# SPDX-License-Identifier: MIT
# Selections by geometry and the edges of a failed fillet (mitcad#1587),
# through mitcad-cli's headless MCP server. A cubic lattice (corner balls,
# rods, a centre ball): its revolve picks its six profile regions by a
# point inside each, and its fillet takes the edges between the rods and
# the balls by a filter. The stored definitions keep region keys and edge
# names. The centre ball is the revolve's, joined four times by the
# pattern about Z; while that join leaves the fault mitcad#1588 fixes,
# some joints fail while the others succeed, and the result names every
# edge that fails alone.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<test folder> -P mcp-selections-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "mcp-selections-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(MAKE_DIRECTORY "${WORK}")

set(input "")
# request(<id> <tool> <arguments>): one tools/call line.
function(request id tool arguments)
  # One request a line: the commands below are written over several.
  string(REPLACE "\n" " " arguments "${arguments}")
  string(APPEND input "{\"jsonrpc\": \"2.0\", \"id\": ${id}, \"method\": \"tools/call\", "
                      "\"params\": {\"name\": \"${tool}\", \"arguments\": ${arguments}}}\n")
  set(input "${input}" PARENT_SCOPE)
endfunction()
# command(<id> <command>): one model_command of the document D1.
function(command id cmd)
  request(${id} model_command "{\"document\": \"D1\", \"command\": ${cmd}}")
  set(input "${input}" PARENT_SCOPE)
endfunction()

string(APPEND input
  [=[{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "mcp-selections-test", "version": "1"}}}]=] "\n"
  [=[{"jsonrpc": "2.0", "method": "notifications/initialized"}]=] "\n")
request(2 document_create "{}")
# F1: the sketch on the plane through Z and the XY diagonal: the body
# diagonal c6 from corner to corner, the corner balls c7 and c8, the
# centre ball c9, the rod's side c12 two away from c6, and the edge rod's
# path c14 (construction).
command(3 [=[{"cmd": "add_feature", "def": {"type": "sketch", "plane": "xz",
  "frame": {"origin": [0, 0, 0], "x_axis": [0.7071067811865476, 0, 0.7071067811865476], "y_axis": [0, -1, 0]},
  "entities": [
    {"id": "p1", "type": "point", "at": [0, 0], "fixed": true},
    {"id": "p2", "type": "point", "at": [35.3553, 25]},
    {"id": "p3", "type": "point", "at": [-35.3553, -25]},
    {"id": "p4", "type": "point", "at": [41.479, 29.3301]},
    {"id": "p5", "type": "point", "at": [-41.479, -29.3301]},
    {"id": "c6", "type": "line", "start": "p5", "end": "p4"},
    {"id": "c7", "type": "circle", "center": "p2", "radius": 7.5},
    {"id": "c8", "type": "circle", "center": "p3", "radius": 7.5},
    {"id": "c9", "type": "circle", "center": "p1", "radius": 10},
    {"id": "p10", "type": "point", "at": [-30.608, -19.1936]},
    {"id": "p11", "type": "point", "at": [28.2986, 22.4596]},
    {"id": "c12", "type": "line", "start": "p10", "end": "p11"},
    {"id": "p13", "type": "point", "at": [35.3553, -25]},
    {"id": "c14", "type": "line", "start": "p13", "end": "p2", "construction": true}],
  "constraints": [
    {"id": "k1", "type": "coincident", "point": "p2", "entity": "c6"},
    {"id": "k2", "type": "coincident", "point": "p3", "entity": "c6"},
    {"id": "k3", "type": "coincident", "point": "p4", "entity": "c7"},
    {"id": "k4", "type": "coincident", "point": "p5", "entity": "c8"},
    {"id": "k5", "type": "parallel", "a": "c12", "b": "c6"},
    {"id": "k6", "type": "coincident", "point": "p10", "entity": "c8"},
    {"id": "k7", "type": "coincident", "point": "p11", "entity": "c7"},
    {"id": "k8", "type": "vertical", "line": "c14"}],
  "dimensions": [
    {"id": "k9", "type": "horizontal_distance", "a": "p1", "b": "p2", "value": "25 * sqrt(2)"},
    {"id": "k10", "type": "vertical_distance", "a": "p1", "b": "p2", "value": 25},
    {"id": "k11", "type": "horizontal_distance", "a": "p3", "b": "p1", "value": "25 * sqrt(2)"},
    {"id": "k12", "type": "vertical_distance", "a": "p3", "b": "p1", "value": 25},
    {"id": "k13", "type": "diameter", "curve": "c7", "value": 15},
    {"id": "k14", "type": "diameter", "curve": "c8", "value": 15},
    {"id": "k15", "type": "diameter", "curve": "c9", "value": 20},
    {"id": "k16", "type": "line_distance", "a": "c12", "b": "c6", "value": 2},
    {"id": "k17", "type": "length", "line": "c14", "value": 50}]}}]=])
# F2: the balls, the diagonal rod and the centre ball, each region by a
# point inside it on c12's side of c6.
command(4 [=[{"cmd": "add_feature", "def": {"type": "revolve",
  "profiles": [
    {"sketch": "F1", "point": [32.47, 29.08]},
    {"sketch": "F1", "point": [-38.24, -20.92]},
    {"sketch": "F1", "point": [-16.91, -10.73]},
    {"sketch": "F1", "point": [15.75, 12.36]},
    {"sketch": "F1", "point": [-0.577, 0.816]},
    {"sketch": "F1", "point": [-2.887, 4.082]}],
  "axis": {"sketch": "F1", "curve": "c6"}, "extent": {"type": "full"}, "operation": "new_body"}}]=])
# F3: one edge rod; F4 three of them about a body diagonal; F5 all four
# about Z.
command(5 [=[{"cmd": "add_feature", "def": {"type": "pipe", "path": {"sketch": "F1", "curves": ["c14"]},
  "section": "circular", "size": 4, "operation": "join"}}]=])
command(6 [=[{"cmd": "add_feature", "def": {"type": "circular_pattern", "objects": {"type": "features", "features": ["F3"]},
  "axis": {"origin": [0, 0, 0], "direction": [1, -1, 1]}, "quantity": 3, "angle": "360 deg", "compute": "identical"}}]=])
command(7 [=[{"cmd": "add_feature", "def": {"type": "circular_pattern", "objects": {"type": "features", "features": ["F2", "F4"]},
  "axis": "z", "quantity": 4, "angle": "360 deg"}}]=])
# F6: every joint of a rod and a ball by a filter.
command(8 [=[{"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
  "sets": [{"edges": {"between": ["cylinder", "sphere"]}, "size": {"type": "constant", "radius": 2}}]}}]=])
request(9 model_query [=[{"document": "D1", "query": {"query": "feature", "uid": "F2"}}]=])
request(10 model_query [=[{"document": "D1", "query": {"query": "feature", "uid": "F6"}}]=])
request(11 model_query [=[{"document": "D1", "query": {"query": "profiles", "sketch": "F1"}}]=])
# A point in no region is refused.
command(12 [=[{"cmd": "add_feature", "def": {"type": "revolve",
  "profiles": [{"sketch": "F1", "point": [80, 80]}],
  "axis": {"sketch": "F1", "curve": "c6"}, "extent": {"type": "full"}, "operation": "new_body"}}]=])
file(WRITE "${WORK}/requests.jsonl" "${input}")

execute_process(COMMAND "${CLI}" mcp --workspace "${WORK}"
                INPUT_FILE "${WORK}/requests.jsonl"
                RESULT_VARIABLE status OUTPUT_VARIABLE output ERROR_VARIABLE errors)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "mitcad-cli mcp: exit status '${status}':\n${errors}")
endif()
# The answers, to look at after a failure.
file(WRITE "${WORK}/responses.jsonl" "${output}")

# check(<response> <what> <expected> <json path>...): the value at the path.
function(check response what expected)
  string(JSON value ERROR_VARIABLE failure GET "${response}" ${ARGN})
  if(failure OR NOT "${value}" STREQUAL "${expected}")
    message(FATAL_ERROR "${what}: '${ARGN}' is '${value}', '${expected}' expected ${failure}:\n${response}")
  endif()
endfunction()
# has(<response> <what> <text>): the response holds this text.
function(has response what text)
  string(FIND "${response}" "${text}" at)
  if(at EQUAL -1)
    message(FATAL_ERROR "${what}: no '${text}' in:\n${response}")
  endif()
endfunction()
# lacks(<response> <what> <text>): the response does not hold this text.
function(lacks response what text)
  string(FIND "${response}" "${text}" at)
  if(NOT at EQUAL -1)
    message(FATAL_ERROR "${what}: '${text}' in:\n${response}")
  endif()
endfunction()

set(checked 0)
while(NOT output STREQUAL "")
  string(FIND "${output}" "\n" end)
  if(end EQUAL -1)
    message(FATAL_ERROR "an MCP response without its newline: ${output}")
  endif()
  string(SUBSTRING "${output}" 0 ${end} response)
  math(EXPR next "${end} + 1")
  string(SUBSTRING "${output}" ${next} -1 output)
  # Notifications (no id) are not answers.
  string(JSON id ERROR_VARIABLE notification GET "${response}" id)
  if(notification)
    continue()
  endif()
  string(JSON result ERROR_VARIABLE missing GET "${response}" result)
  if(missing)
    message(FATAL_ERROR "request ${id}: no result:\n${response}")
  endif()
  if(id GREATER_EQUAL 2 AND id LESS_EQUAL 7)
    check("${response}" "request ${id}" OFF result isError)
  elseif(id EQUAL 8)
    set(what "the fillet of every joint")
    check("${response}" "${what}" ON result structuredContent applied)
    string(JSON failed GET "${response}" result isError)
  endif()
  # The joint fails while the pattern joins the centre ball's coincident
  # copies as it does; mitcad#1588 changes that, and then the fillet is
  # simply built.
  if(id EQUAL 8 AND failed)
    has("${response}" "${what}" "tried one at a time")
    has("${response}" "${what}" "fail alone")
    string(JSON alone GET "${response}" result structuredContent failed_features 0 alone)
    has("${alone}" "${what}" "F5:inst3(F2:side(c12[c9,c7]))")
    string(JSON fail LENGTH "${alone}" fail)
    string(JSON succeed GET "${alone}" succeed)
    # Coincident pattern faces can split differently across kernels. The
    # diagnostic must name the known bad joint and account for every edge,
    # rather than assume that exactly one of the joints fails alone.
    if(fail LESS 1 OR succeed LESS 39)
      message(FATAL_ERROR "${what}: ${fail} edges fail alone and ${succeed} succeed, at least 1 and 39 expected:\n${response}")
    endif()
  elseif(id EQUAL 9)
    set(what "the revolve's profiles")
    has("${response}" "${what}" "region")
    # The six regions on c12's side.
    foreach(region "c7[c6,c12]" "c8[c6,c12]" "c12[c8,c9]" "c12[c9,c7]" "c9[c6,c12]" "c9[c12,c12]")
      has("${response}" "${what}" "${region}")
    endforeach()
  elseif(id EQUAL 10)
    set(what "the fillet's edges")
    has("${response}" "${what}" "\\\"edges\\\":[\\\"E{")
    lacks("${response}" "${what}" "between")
    if(failed)
      string(JSON edges GET "${response}" result structuredContent result def edges)
      string(JSON selected LENGTH "${edges}")
      math(EXPR diagnosed "${fail} + ${succeed}")
      if(NOT diagnosed EQUAL selected)
        message(FATAL_ERROR "${what}: ${diagnosed} diagnosed edges, ${selected} selected")
      endif()
      set(selected_edges "")
      math(EXPR last "${selected} - 1")
      foreach(index RANGE 0 ${last})
        string(JSON edge GET "${edges}" ${index})
        list(APPEND selected_edges "${edge}")
      endforeach()
      set(failed_edges "")
      math(EXPR last "${fail} - 1")
      foreach(index RANGE 0 ${last})
        string(JSON edge GET "${alone}" fail ${index})
        list(FIND selected_edges "${edge}" found)
        list(FIND failed_edges "${edge}" duplicate)
        if(found EQUAL -1 OR NOT duplicate EQUAL -1)
          message(FATAL_ERROR "${what}: failed edge is unselected or repeated: ${edge}")
        endif()
        list(APPEND failed_edges "${edge}")
      endforeach()
    endif()
  elseif(id EQUAL 11)
    has("${response}" "the profiles' measures" "\\\"area\\\":")
    has("${response}" "the profiles' measures" "\\\"center\\\":")
  elseif(id EQUAL 12)
    set(what "a point in no region")
    check("${response}" "${what}" ON result isError)
    has("${response}" "${what}" "no profile region of F1 holds the point")
  endif()
  math(EXPR checked "${checked} + 1")
endwhile()
if(NOT checked EQUAL 12)
  message(FATAL_ERROR "${checked} MCP responses, 11 expected:\n${errors}")
endif()
