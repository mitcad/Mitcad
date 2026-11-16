# SPDX-License-Identifier: MIT
# References between the entries of a model_command array (mitcad#1897),
# through mitcad-cli's stdio transport: a parametric closed outline with a
# clockwise arc, its constraints and its extrusion in one array, each
# entry taking what an earlier one made by name ("as", {"ref": ...}); a
# forward reference refuses the array before anything runs; a reference
# of the wrong kind refuses its entry and undoes the entries before it.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<test folder> -P mcp-array-refs-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "mcp-array-refs-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(MAKE_DIRECTORY "${WORK}")

set(input "")
# command(<id> <command>): one model_command of the document D1.
function(command id cmd)
  string(APPEND input "{\"jsonrpc\": \"2.0\", \"id\": ${id}, \"method\": \"tools/call\", "
                      "\"params\": {\"name\": \"model_command\", \"arguments\": "
                      "{\"document\": \"D1\", \"command\": ${cmd}}}}\n")
  set(input "${input}" PARENT_SCOPE)
endfunction()

# The outline (as core/model-api/src/commands.md's example): entries 0-2
# parameters, 3 the sketch, 4-7 the curves, 8-9 constraints, 10 the arc's
# radius, 11 the extrusion. @L2START@ and @L3END@ are replaced below.
set(outline [=[[
{"cmd": "add_parameter", "name": "w", "value": 60},
{"cmd": "add_parameter", "name": "h", "value": 40},
{"cmd": "add_parameter", "name": "t", "value": 5},
{"cmd": "sketch.create", "as": "sk"},
{"cmd": "sketch.add_line", "sketch": {"ref": "sk.uid"}, "as": "l1", "start": ["0", "0"], "end": ["w", 0]},
{"cmd": "sketch.add_line", "sketch": {"ref": "sk.uid"}, "as": "l2", "start": @L2START@, "end": ["w", "h"]},
{"cmd": "sketch.arc", "sketch": {"ref": "sk.uid"}, "as": "a1", "mode": "three_point", "start": {"ref": "l2.end"}, "through": [30, 30], "end": [0, "h"]},
{"cmd": "sketch.add_line", "sketch": {"ref": "sk.uid"}, "as": "l3", "start": {"ref": "a1.end"}, "end": @L3END@},
{"cmd": "sketch.add_constraint", "sketch": {"ref": "sk.uid"}, "constraint": {"type": "horizontal", "line": {"ref": "l1.made.0"}}},
{"cmd": "sketch.add_constraint", "sketch": {"ref": "sk.uid"}, "constraint": {"type": "vertical", "line": {"ref": "l3.made.0"}}},
{"cmd": "sketch.add_dimension", "sketch": {"ref": "sk.uid"}, "dimension": {"type": "radius", "curve": {"ref": "a1.made.0"}}},
{"cmd": "add_feature", "as": "body", "def": {"type": "extrude", "profiles": [{"sketch": {"ref": "sk.uid"}, "point": [30, 10]}], "extent": {"type": "distance", "distance": "t"}, "operation": "new_body"}}
]]=])
string(REPLACE "\n" " " outline "${outline}")
# outline_with(<out> <l2 start> <l3 end>)
function(outline_with out l2start l3end)
  string(REPLACE "@L2START@" "${l2start}" text "${outline}")
  string(REPLACE "@L3END@" "${l3end}" text "${text}")
  set(${out} "${text}" PARENT_SCOPE)
endfunction()

string(APPEND input
  [=[{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "mcp-array-refs-test", "version": "1"}}}]=] "\n"
  [=[{"jsonrpc": "2.0", "method": "notifications/initialized"}]=] "\n"
  [=[{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "document_create", "arguments": {}}}]=] "\n")
# 3: l2 refers to the arc, which comes after it: nothing runs.
outline_with(forward [=[{"ref": "a1.end"}]=] [=[{"ref": "l1.start"}]=])
command(3 "${forward}")
# 4: l3 ends on l1's curve, not a point: entry 7 is refused, 0-6 undone.
outline_with(wrong [=[{"ref": "l1.end"}]=] [=[{"ref": "l1.made.0"}]=])
command(4 "${wrong}")
# 5: the outline and its extrusion.
outline_with(whole [=[{"ref": "l1.end"}]=] [=[{"ref": "l1.start"}]=])
command(5 "${whole}")
file(WRITE "${WORK}/requests.jsonl" "${input}")

execute_process(COMMAND "${CLI}" mcp --workspace "${WORK}"
                INPUT_FILE "${WORK}/requests.jsonl"
                RESULT_VARIABLE status OUTPUT_VARIABLE output ERROR_VARIABLE errors)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "mitcad-cli mcp: exit status '${status}':\n${errors}")
endif()

# check(<response> <what> <expected> <json path>...): the value at the path.
function(check response what expected)
  string(JSON value ERROR_VARIABLE failure GET "${response}" ${ARGN})
  if(failure OR NOT "${value}" STREQUAL "${expected}")
    message(FATAL_ERROR "${what}: '${ARGN}' is '${value}', '${expected}' expected ${failure}:\n${response}")
  endif()
endfunction()

# same(<response> <what> <path a> <path b>): two values of the result are
# the same (each path a ';' list).
function(same response what a b)
  string(JSON first ERROR_VARIABLE failure GET "${response}" result structuredContent ${a})
  string(JSON second ERROR_VARIABLE failure2 GET "${response}" result structuredContent ${b})
  if(failure OR failure2 OR NOT "${first}" STREQUAL "${second}" OR "${first}" STREQUAL "")
    message(FATAL_ERROR "${what}: '${a}' is '${first}', '${b}' is '${second}':\n${response}")
  endif()
endfunction()

# says(<response> <what> <text>): the error text holds this text.
function(says response what text)
  string(JSON error ERROR_VARIABLE failure GET "${response}" result structuredContent error)
  string(FIND "${error}" "${text}" at)
  if(failure OR at EQUAL -1)
    message(FATAL_ERROR "${what}: the error text '${error}' does not say '${text}' ${failure}:\n${response}")
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
  string(JSON id GET "${response}" id)
  string(JSON result ERROR_VARIABLE missing GET "${response}" result)
  if(missing)
    message(FATAL_ERROR "request ${id}: no result:\n${response}")
  endif()
  if(id EQUAL 3)
    set(what "the forward reference")
    check("${response}" "${what}" ON result isError)
    check("${response}" "${what}" OFF result structuredContent applied)
    check("${response}" "${what}" 5 result structuredContent entry)
    check("${response}" "${what}" 0 result structuredContent undone)
    says("${response}" "${what}" "commands[5]: ")
    says("${response}" "${what}" "which runs after this one")
  elseif(id EQUAL 4)
    set(what "the reference to a curve as a point")
    check("${response}" "${what}" ON result isError)
    check("${response}" "${what}" OFF result structuredContent applied)
    check("${response}" "${what}" 7 result structuredContent entry)
    says("${response}" "${what}" "commands[7]: end: ")
    says("${response}" "${what}" "not a point")
    says("${response}" "${what}" "were undone")
  elseif(id EQUAL 5)
    set(what "the outline")
    check("${response}" "${what}" OFF result isError)
    check("${response}" "${what}" ON result structuredContent applied)
    check("${response}" "${what}" 12 result structuredContent ran)
    check("${response}" "${what}" 0 result structuredContent results 10 status dof)
    # The references joined the curves; l3 starts at the arc's end as
    # written and closes the outline on l1's start.
    same("${response}" "${what}" "results;5;inputs;start" "results;4;inputs;end")
    same("${response}" "${what}" "results;7;inputs;start" "results;6;inputs;end")
    same("${response}" "${what}" "results;7;inputs;end" "results;4;inputs;start")
  endif()
  math(EXPR checked "${checked} + 1")
endwhile()
# initialize and document_create answer too.
if(NOT checked EQUAL 5)
  message(FATAL_ERROR "${checked} MCP responses, 5 expected:\n${errors}")
endif()
