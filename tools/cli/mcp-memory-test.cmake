# SPDX-License-Identifier: MIT
# The headless MCP server's memory guard of model commands (mitcad#1480),
# through mitcad-cli's stdio transport, with a memory limit far below what
# the server holds (--memory-limit 1, one MiB): a model command, alone or
# in an array, is refused with an error that says the operation ran out
# of memory and names the limit, the document stays as it was (no undo
# step, not modified), and the server answers the calls after it and ends
# normally. The geometry is not the point: the guard measures before and
# while a command computes, and the lowered limit is exceeded from the
# start.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<test folder> -P mcp-memory-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "mcp-memory-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(MAKE_DIRECTORY "${WORK}")

set(input "")
# request(<id> <tool> <arguments>): one tools/call line.
function(request id tool arguments)
  string(APPEND input "{\"jsonrpc\": \"2.0\", \"id\": ${id}, \"method\": \"tools/call\", "
                      "\"params\": {\"name\": \"${tool}\", \"arguments\": ${arguments}}}\n")
  set(input "${input}" PARENT_SCOPE)
endfunction()

set(box [=[{"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": [0, 0], "length": 30, "width": 20, "height": 10, "operation": "new_body"}}]=])
string(REPLACE "[0, 0]" "[100, 0]" other "${box}")

string(APPEND input
  [=[{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "mcp-memory-test", "version": "1"}}}]=] "\n"
  [=[{"jsonrpc": "2.0", "method": "notifications/initialized"}]=] "\n")
request(2 document_create "{}")
# One command.
request(3 model_command "{\"document\": \"D1\", \"command\": ${box}}")
# An array: refused at its first entry, nothing of it stays.
request(4 model_command "{\"document\": \"D1\", \"command\": [${box}, ${other}]}")
# The server answers the next calls: the document as it was.
request(5 model_query [=[{"document": "D1", "query": {"query": "document"}}]=])
request(6 document_list "{}")
file(WRITE "${WORK}/requests.jsonl" "${input}")

execute_process(COMMAND "${CLI}" mcp --workspace "${WORK}" --memory-limit 1
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
    check("${response}" "the box" ON result isError)
    says("${response}" "the box" "the operation ran out of memory at" "(the command memory limit)"
         "as it was before the command")
  elseif(id EQUAL 4)
    check("${response}" "the array" ON result isError)
    check("${response}" "the array" OFF result structuredContent applied)
    check("${response}" "the array" 0 result structuredContent entry)
    says("${response}" "the array" "commands[0]: the operation ran out of memory at")
  elseif(id EQUAL 5)
    check("${response}" "the document after" OFF result isError)
    check("${response}" "the document after" 0 result structuredContent result undo_depth)
    check("${response}" "the document after" OFF result structuredContent result modified)
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
