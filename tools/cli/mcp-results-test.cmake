# SPDX-License-Identifier: MIT
# The headless MCP server's model_command results (mitcad#401, mitcad#403),
# through mitcad-cli's stdio transport. A command is an error (isError)
# only when a feature it added or edited fails: with a failed cut in the
# timeline, an unrelated parameter, a new body, an insert and a deletion
# are not errors, and each lists the cut in failed_features. add_feature
# with "after" inserts after that feature in one command, the marker
# staying at the end. The error text of a failing add_feature, alone and
# as an array's entry, says the feature stays in the timeline and how to
# remove it (mitcad#1481).
#
# cmake -DCLI=<mitcad-cli> -DWORK=<test folder> -P mcp-results-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "mcp-results-test.cmake needs -D${var}=...")
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
# command(<id> <command>): one model_command of the document D1.
function(command id cmd)
  request(${id} model_command "{\"document\": \"D1\", \"command\": ${cmd}}")
  set(input "${input}" PARENT_SCOPE)
endfunction()
# box(<out> <corner> <operation> [<more fields>]): an add_feature of a box.
function(box out corner operation)
  set(${out} "{\"cmd\": \"add_feature\"${ARGN}, \"def\": {\"type\": \"box\", \"plane\": {\"type\": \"origin\", \"plane\": \"xy\"}, \"corner\": ${corner}, \"length\": 30, \"width\": 20, \"height\": 10, \"operation\": \"${operation}\"}}" PARENT_SCOPE)
endfunction()

string(APPEND input
  [=[{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "mcp-results-test", "version": "1"}}}]=] "\n"
  [=[{"jsonrpc": "2.0", "method": "notifications/initialized"}]=] "\n")
request(2 document_create "{}")
# F1: a box.
box(first "[0, 0]" new_body)
command(3 "${first}")
# F2: a cut that misses the box: it fails, and it is this command's.
box(cut "[100, 100]" cut)
string(REPLACE "\"operation\": \"cut\"" "\"operation\": \"cut\", \"participants\": [\"F1.b0\"]" cut "${cut}")
command(4 "${cut}")
# Unrelated commands while the cut fails; F4 is inserted after F1, before
# the cut (mitcad#403).
command(5 [=[{"cmd": "add_parameter", "name": "wall", "value": 2}]=])
box(other "[0, 100]" new_body)
command(6 "${other}")
box(inserted "[200, 0]" new_body ", \"after\": \"F1\"")
# The whole timeline on request (mitcad#689).
request(7 model_command "{\"document\": \"D1\", \"command\": ${inserted}, \"timeline\": \"full\"}")
command(8 [=[{"cmd": "delete_feature", "uid": "F3"}]=])
# An array whose entry 1 adds another failing cut: it stops there, and its
# error text names that entry and the cut (mitcad#1481).
box(more "[0, 300]" new_body)
command(9 "[${more}, ${cut}, ${other}]")
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

# count(<response> <what> <expected>): the number of failed_features.
function(count response what expected)
  string(JSON length ERROR_VARIABLE failure LENGTH "${response}" result structuredContent failed_features)
  if(failure OR NOT length EQUAL expected)
    message(FATAL_ERROR "${what}: ${length} failed features, ${expected} expected ${failure}:\n${response}")
  endif()
endfunction()

# says(<response> <what> <text>): the error text holds this text
# (mitcad#1481: the failed feature stays in the timeline, and how to
# remove it).
function(says response what text)
  string(JSON error ERROR_VARIABLE failure GET "${response}" result structuredContent error)
  string(FIND "${error}" "${text}" at)
  if(failure OR at EQUAL -1)
    message(FATAL_ERROR "${what}: the error text '${error}' does not say '${text}' ${failure}:\n${response}")
  endif()
endfunction()

set(order F1 F4 F2 F3)
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
    count("${response}" "document_create" 0)
  elseif(id EQUAL 3)
    check("${response}" "the box" OFF result isError)
    count("${response}" "the box" 0)
  elseif(id EQUAL 4)
    check("${response}" "the failing cut" ON result isError)
    check("${response}" "the failing cut" ON result structuredContent applied)
    count("${response}" "the failing cut" 1)
    check("${response}" "the failing cut" F2 result structuredContent failed_features 0 uid)
    check("${response}" "the failing cut" ON result structuredContent failed_features 0 edited)
    says("${response}" "the failing cut" "F2 (")
    says("${response}" "the failing cut" ") was added but failed: ")
    says("${response}" "the failing cut" "It stays in the timeline")
    says("${response}" "the failing cut" "remove it with undo or delete_feature")
  elseif(id EQUAL 9)
    set(what "the array with a failing cut")
    check("${response}" "${what}" ON result isError)
    check("${response}" "${what}" ON result structuredContent applied)
    check("${response}" "${what}" 1 result structuredContent stopped_at)
    check("${response}" "${what}" 2 result structuredContent ran)
    count("${response}" "${what}" 2)
    string(JSON stopped GET "${response}" result structuredContent results 1 uid)
    says("${response}" "${what}" "commands[1]: ${stopped} (")
    says("${response}" "${what}" ") was added but failed: ")
    says("${response}" "${what}" "It stays in the timeline")
    says("${response}" "${what}" "remove it with undo or delete_feature")
    says("${response}" "${what}" "the entries after it did not run")
  elseif(id GREATER_EQUAL 5)
    set(what "command ${id} while the cut fails")
    check("${response}" "${what}" OFF result isError)
    count("${response}" "${what}" 1)
    check("${response}" "${what}" F2 result structuredContent failed_features 0 uid)
    check("${response}" "${what}" OFF result structuredContent failed_features 0 edited)
    check("${response}" "${what}" OFF result structuredContent failed_features 0 new)
    if(id EQUAL 5)
      # By default only what changed (mitcad#689): the cut, left failed,
      # is listed; the box the parameter did not touch is not.
      check("${response}" "the parameter" F2 result structuredContent changes features 0 uid)
      string(JSON timeline ERROR_VARIABLE absent GET "${response}" result structuredContent timeline)
      if(NOT absent)
        message(FATAL_ERROR "the parameter: the whole timeline without timeline full:\n${response}")
      endif()
    endif()
    if(id EQUAL 7)
      check("${response}" "the insert" F4 result structuredContent result uid)
      check("${response}" "the insert" 4 result structuredContent timeline marker)
      set(index 0)
      foreach(uid IN LISTS order)
        check("${response}" "the insert" ${uid} result structuredContent timeline features ${index} uid)
        math(EXPR index "${index} + 1")
      endforeach()
    endif()
  endif()
  math(EXPR checked "${checked} + 1")
endwhile()
if(NOT checked EQUAL 9)
  message(FATAL_ERROR "${checked} MCP responses, 9 expected:\n${errors}")
endif()
