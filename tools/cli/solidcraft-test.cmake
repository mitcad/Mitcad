# SPDX-License-Identifier: MIT
# The .solidcraft import through mitcad-cli (mitcad#450) on the test
# designs written by hand for it (core/solidcraft/tests/designs): each
# <name>.solidcraft.json with a <name>.expected.json next to it is imported
# and checked against those values (bodies, volumes, boxes, timeline,
# parameters, sketches); box-fillet-hole is also imported with a parameter
# changed (--set width=60 mm) and checked against the values worked out for
# that change; the imported design is saved and opened again, and the
# container form (a zip of document.json) imports as the JSON does.
#
# cmake -DCLI=<mitcad-cli> -DDESIGNS=<folder> -DWORK=<folder> -P solidcraft-test.cmake

foreach(var CLI DESIGNS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "solidcraft-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(MAKE_DIRECTORY "${WORK}")

# run(<name> <args>...): mitcad-cli with the arguments; fails the test
# when it fails, and prints its first line.
function(run name)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE result OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT result EQUAL 0)
    message(FATAL_ERROR "${name}: exit ${result}\n${out}\n${err}")
  endif()
  string(REGEX MATCH "^[^\n]*" first "${out}")
  message(STATUS "${name}: ${first}")
  set(output "${out}" PARENT_SCOPE)
endfunction()

file(GLOB designs "${DESIGNS}/*.solidcraft.json")
list(SORT designs)
set(checked 0)
foreach(design IN LISTS designs)
  get_filename_component(file "${design}" NAME)
  string(REPLACE ".solidcraft.json" "" stem "${file}")
  set(expected "${DESIGNS}/${stem}.expected.json")
  if(NOT EXISTS "${expected}")
    continue()
  endif()
  run("${stem}" import-solidcraft "${design}" --expected "${expected}" --report "${WORK}/${stem}.json"
      --save "${WORK}/${stem}.mitcad")
  if(NOT output MATCHES "check: passed")
    message(FATAL_ERROR "${stem}: the check did not pass\n${output}")
  endif()
  # The design saved opens again with every feature computing.
  run("${stem} reopened" info "${WORK}/${stem}.mitcad")
  if(output MATCHES "\\): error:")
    message(FATAL_ERROR "${stem}: a feature fails after reopening\n${output}")
  endif()
  math(EXPR checked "${checked} + 1")
endforeach()
if(checked LESS 5)
  message(FATAL_ERROR "only ${checked} test designs found in ${DESIGNS}")
endif()

# A parameter changed after the import rebuilds the model.
run("box-fillet-hole, width 60" import-solidcraft "${DESIGNS}/box-fillet-hole.solidcraft.json"
    --set "width=60 mm" --expected "${DESIGNS}/box-fillet-hole.width60.expected.json")
if(NOT output MATCHES "check: passed")
  message(FATAL_ERROR "box-fillet-hole with width 60: the check did not pass\n${output}")
endif()
# Convert Mesh's tolerance is a parameter (mitcad#462): within 0.05 mm the
# hole's chords stay planes.
run("mesh-convert, tol 0.05" import-solidcraft "${DESIGNS}/mesh-convert.solidcraft.json"
    --set "tol=0.05 mm" --expected "${DESIGNS}/mesh-convert.tol005.expected.json")
if(NOT output MATCHES "check: passed")
  message(FATAL_ERROR "mesh-convert with tol 0.05: the check did not pass\n${output}")
endif()
# A frame member follows its 3D line (mitcad#2157): the rail's length is a
# parameter of the line's dimension, and the member runs the new length.
run("frame-members, rail 150" import-solidcraft "${DESIGNS}/frame-members.solidcraft.json"
    --set "rail=150 mm" --expected "${DESIGNS}/frame-members.rail150.expected.json")
if(NOT output MATCHES "check: passed")
  message(FATAL_ERROR "frame-members with rail 150: the check did not pass\n${output}")
endif()

# The container: document.json in a zip with container.json.
set(pack "${WORK}/pack")
file(MAKE_DIRECTORY "${pack}")
file(WRITE "${pack}/container.json" "{\"format\":\"solidcraft-container/1\"}")
configure_file("${DESIGNS}/box-fillet-hole.solidcraft.json" "${pack}/document.json" COPYONLY)
execute_process(COMMAND "${CMAKE_COMMAND}" -E tar cf "${WORK}/box-fillet-hole.solidcraft" --format=zip
                        container.json document.json
                WORKING_DIRECTORY "${pack}" RESULT_VARIABLE result)
if(NOT result EQUAL 0)
  message(FATAL_ERROR "could not pack the container")
endif()
run("container" import-solidcraft "${WORK}/box-fillet-hole.solidcraft"
    --expected "${DESIGNS}/box-fillet-hole.expected.json")
if(NOT output MATCHES "check: passed")
  message(FATAL_ERROR "the container: the check did not pass\n${output}")
endif()

# A file that is no design is refused with a reason.
file(WRITE "${WORK}/not-a-design.solidcraft" "{\"format\": \"other/1\"}")
execute_process(COMMAND "${CLI}" import-solidcraft "${WORK}/not-a-design.solidcraft"
                RESULT_VARIABLE result OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(result EQUAL 0 OR NOT "${out}${err}" MATCHES "not a SolidCraft design")
  message(FATAL_ERROR "a file that is no design was not refused: ${out}${err}")
endif()
# The headless MCP server opens a .solidcraft design with its history
# (document_open): a new document with its features.
set(mcp "${WORK}/mcp")
file(MAKE_DIRECTORY "${mcp}")
configure_file("${DESIGNS}/box-fillet-hole.solidcraft.json" "${mcp}/part.solidcraft" COPYONLY)
file(WRITE "${mcp}/requests.jsonl"
  [=[{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "solidcraft-test", "version": "1"}}}]=] "\n"
  [=[{"jsonrpc": "2.0", "method": "notifications/initialized"}]=] "\n"
  [=[{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "document_open", "arguments": {"path": "part.solidcraft"}}}]=] "\n"
  [=[{"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "model_query", "arguments": {"document": "D1", "query": {"query": "timeline"}}}}]=] "\n")
execute_process(COMMAND "${CLI}" mcp --workspace "${mcp}" INPUT_FILE "${mcp}/requests.jsonl"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0 OR NOT out MATCHES "Fillet1" OR out MATCHES "\"isError\": ?true")
  message(FATAL_ERROR "MCP document_open of a .solidcraft design: ${status}\n${out}\n${err}")
endif()

message(STATUS "solidcraft: ${checked} designs checked")
