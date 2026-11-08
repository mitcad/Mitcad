# SPDX-License-Identifier: MIT
# The memory guard of model commands on the boolean that ran the headless
# MCP server out of memory (mitcad#1480, mitcad#1493), through mitcad-cli's
# stdio transport: a 1000 x 1000 x 10 mm plate and 38 overlapping spheres
# of radius 100-199 mm, flattened to a fifth of their height by one
# non-uniform scale (ellipsoids of revolution since mitcad#1504) so that
# their caps rise 3 mm above the plate, joined into the plate in one
# Combine. The server holds some 80 MiB with the bodies; the join takes
# about 1.7 GiB more within a second on 32 threads (each thread's solid
# classifiers of the pieces it sorts, mitcad#1529; as B-splines before
# mitcad#1504 over 2 GiB, growing by about 6 GiB a second, and invalid).
# With --memory-limit 300 the Combine is refused with the memory error
# within a second, the document stays as it was (no Combine, one undo
# step), and the server answers the calls after it and ends normally.
# With -DBUILT=ON and --memory-limit 4096 (mitcad#1529) the join builds:
# one valid body, the Combine added (two undo steps), in a few seconds.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<test folder> [-DBUILT=ON] -P mcp-memory-boolean-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "mcp-memory-boolean-test.cmake needs -D${var}=...")
  endif()
endforeach()
if(BUILT)
  set(limit 4096)
else()
  set(limit 300)
endif()
file(MAKE_DIRECTORY "${WORK}")

set(input "")
# request(<id> <tool> <arguments>): one tools/call line.
function(request id tool arguments)
  string(APPEND input "{\"jsonrpc\": \"2.0\", \"id\": ${id}, \"method\": \"tools/call\", "
                      "\"params\": {\"name\": \"${tool}\", \"arguments\": ${arguments}}}\n")
  set(input "${input}" PARENT_SCOPE)
endfunction()

# The plate, the spheres (seven a row, every other row shifted by 40 mm)
# and the scale, as one command array; the spheres' centres lie so that
# their tops, scaled by 0.2 about the origin, are at z = 13.
set(commands [=[{"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": [-500, -500], "length": 1000, "width": 1000, "height": 10, "operation": "new_body"}}]=])
set(spheres "")
foreach(k RANGE 0 37)
  math(EXPR row "${k} / 7")
  math(EXPR column "${k} % 7")
  math(EXPR x "-420 + 150 * ${column} + 40 * (${row} % 2)")
  math(EXPR y "-430 + 130 * ${row}")
  math(EXPR r "100 + (37 * ${k}) % 100")
  math(EXPR z "65 - ${r}")
  math(EXPR d "2 * ${r}")
  math(EXPR feature "${k} + 2")
  string(APPEND commands ", {\"cmd\": \"add_feature\", \"def\": {\"type\": \"sphere\", \"plane\": {\"origin\": [0, 0, ${z}], \"normal\": [0, 0, 1]}, \"center\": [${x}, ${y}], \"diameter\": ${d}, \"operation\": \"new_body\"}}")
  if(spheres)
    string(APPEND spheres ", ")
  endif()
  string(APPEND spheres "\"F${feature}.b0\"")
endforeach()
string(APPEND commands ", {\"cmd\": \"add_feature\", \"def\": {\"type\": \"scale\", \"bodies\": [${spheres}], \"point\": \"origin\", \"scale\": {\"type\": \"non_uniform\", \"x\": 1, \"y\": 1, \"z\": 0.2}}}")

string(APPEND input
  [=[{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "mcp-memory-boolean-test", "version": "1"}}}]=] "\n"
  [=[{"jsonrpc": "2.0", "method": "notifications/initialized"}]=] "\n")
request(2 document_create "{}")
request(3 model_command "{\"document\": \"D1\", \"command\": [${commands}]}")
# The join of every sphere into the plate.
request(4 model_command "{\"document\": \"D1\", \"command\": {\"cmd\": \"add_feature\", \"def\": {\"type\": \"combine\", \"target\": \"F1.b0\", \"tools\": [${spheres}], \"operation\": \"join\"}}}")
# The server answers the next calls: the document as it was.
request(5 model_query [=[{"document": "D1", "query": {"query": "document"}}]=])
request(6 document_list "{}")
file(WRITE "${WORK}/requests.jsonl" "${input}")

execute_process(COMMAND "${CLI}" mcp --workspace "${WORK}" --memory-limit ${limit}
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

# says(<response> <what> <text>...): the error contains each text.
function(says response what)
  string(JSON error ERROR_VARIABLE failure GET "${response}" result structuredContent error)
  if(failure)
    message(FATAL_ERROR "${what}: no error ${failure}:\n${response}")
  endif()
  foreach(text IN LISTS ARGN)
    string(FIND "${error}" "${text}" at)
    if(at EQUAL -1)
      message(FATAL_ERROR "${what}: the error does not say '${text}':\n${response}")
    endif()
  endforeach()
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
  if(id EQUAL 2)
    check("${response}" "document_create" OFF result isError)
  elseif(id EQUAL 3)
    check("${response}" "the plate and the spheres" OFF result isError)
  elseif(id EQUAL 4 AND BUILT)
    check("${response}" "the join" OFF result isError)
    check("${response}" "the join" ok result structuredContent changes features 0 status)
    check("${response}" "the join" 1 result structuredContent state bodies)
  elseif(id EQUAL 4)
    check("${response}" "the join" ON result isError)
    says("${response}" "the join" "the operation ran out of memory" "(the command memory limit)"
         "as it was before the command" "join the bodies in smaller groups")
  elseif(id EQUAL 5 AND BUILT)
    check("${response}" "the document after" OFF result isError)
    check("${response}" "the document after" 41 result structuredContent result features)
    check("${response}" "the document after" 1 result structuredContent result bodies)
    check("${response}" "the document after" 2 result structuredContent result undo_depth)
  elseif(id EQUAL 5)
    check("${response}" "the document after" OFF result isError)
    check("${response}" "the document after" 40 result structuredContent result features)
    check("${response}" "the document after" 39 result structuredContent result bodies)
    check("${response}" "the document after" 1 result structuredContent result undo_depth)
  elseif(id EQUAL 6)
    check("${response}" "document_list" OFF result isError)
    check("${response}" "document_list" D1 result structuredContent documents 0 document)
  endif()
  math(EXPR checked "${checked} + 1")
endwhile()
# The initialize response and five tool results.
if(NOT checked EQUAL 6)
  message(FATAL_ERROR "${checked} MCP responses, 6 expected:\n${errors}")
endif()
