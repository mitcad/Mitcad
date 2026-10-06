# SPDX-License-Identifier: MIT
# Comparison of versions (P12c) through mitcad-cli: a block's project file
# in a project with history, recorded, edited (a dimension of its sketch,
# its extrusion's distance, a new parameter and a fillet) and recorded
# again; then the two versions, a version against the file as saved, and
# two files compared, as text and JSON, with the bodies' volumes and areas.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P diff-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "diff-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(project "${WORK}/project")
set(part "${project}/part.mitcad")
set(author "Mitcad Test <test@example.invalid>")

function(cli expect)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(expect STREQUAL "ok" AND NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  elseif(expect STREQUAL "fail" AND status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} should fail:\n${out}${err}")
  elseif(expect STREQUAL "usage" AND NOT status EQUAL 2)
    message(FATAL_ERROR "mitcad-cli ${ARGN} should be refused (${status}):\n${out}${err}")
  endif()
  set(out "${out}${err}" PARENT_SCOPE)
endfunction()

# The output has each pattern (regular expressions).
function(expect what)
  foreach(pattern ${ARGN})
    if(NOT out MATCHES "${pattern}")
      message(FATAL_ERROR "${what}: no '${pattern}' in:\n${out}")
    endif()
  endforeach()
endfunction()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
cli(ok project init "${project}" --author "${author}")

# The first version: a 60 x 40 x 10 block.
file(WRITE "${WORK}/block.json" [=[
[{"cmd": "sketch.create"},
 {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
 {"cmd": "add_feature", "def": {"type": "extrude",
  "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
  "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}]
]=])
cli(ok run "${WORK}/block.json" --save "${part}")
cli(ok version save "${part}" -m "Block" --author "${author}")

# The second: 70 x 40 x 15 with a filleted edge and a new parameter.
file(WRITE "${WORK}/edit.json" [=[
[{"cmd": "set_parameter", "name": "d1", "value": 70},
 {"cmd": "set_parameter", "name": "d3", "value": 15},
 {"cmd": "add_parameter", "name": "wall", "value": 2, "comment": "Wall thickness"},
 {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
  "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 2}}]
]=])
cli(ok run "${WORK}/edit.json" --open "${part}" --save "${part}")
cli(ok version save "${part}" -m "Wider, taller, rounded" --author "${author}")

# The two versions, in the model's terms.
cli(ok diff "${part}" HEAD~1 HEAD)
expect("the versions"
  "^Changes in part\\.mitcad from [0-9a-f]+ to [0-9a-f]+:\nSummary: \\+2 parameters, d1 60 mm -> 70 mm, d3 10 mm -> 15 mm, \\+1 feature, 2 features modified\n"
  "\nParameters:\n  d1 \\([^)]*\\): 60 mm -> 70 mm\n  d3 \\(Extrude1 distance\\): 10 mm -> 15 mm\n  wall \\(Wall thickness\\) = 2 mm: added\n  d4 \\(Fillet1 radius\\) = 2 mm: added\n"
  "\nTimeline:\n  Sketch1 \\(F1\\): k5 length 60 mm -> 70 mm\n  Extrude1 \\(F2\\): distance 10 mm -> 15 mm\n  Fillet1 \\(F3, fillet\\): added after Extrude1\n$")
cli(ok diff "${part}" HEAD~1 HEAD --json)
string(JSON summary GET "${out}" summary)
string(JSON first GET "${out}" features 0 values 0 text)
string(JSON added GET "${out}" features 2 kind)
string(JSON from GET "${out}" from)
string(JSON path GET "${out}" path)
if(NOT summary MATCHES "^\\+2 parameters, d1 60 mm -> 70 mm" OR NOT first STREQUAL "k5 length 60 mm -> 70 mm"
   OR NOT added STREQUAL "added" OR NOT from MATCHES "^[0-9a-f]+$" OR NOT path STREQUAL "part.mitcad")
  message(FATAL_ERROR "the versions as JSON:\n${out}")
endif()

# The latest version and the file as saved: the same, then not.
cli(ok diff "${part}" HEAD)
expect("the latest version" "^Changes in part\\.mitcad from [0-9a-f]+ to the saved file:\nNo differences\n$")
file(WRITE "${WORK}/rename.json" [=[[{"cmd": "rename_feature", "uid": "F3", "name": "Round"}]]=])
cli(ok run "${WORK}/rename.json" --open "${part}" --save "${part}")
cli(ok diff "${part}" HEAD)
expect("a saved change" "\nTimeline:\n  Round \\(F3\\): renamed from Fillet1\n")

# Two files, with the bodies' volumes and areas.
cli(ok version show "${part}" HEAD~1 --save "${WORK}/first.mitcad")
cli(ok diff "${WORK}/first.mitcad" "${part}" --geometry)
expect("two files"
  "^Changes from [^\n]*first\\.mitcad to [^\n]*part\\.mitcad:\nSummary: [^\n]*, \\+1 feature, 2 features modified, volume 24000 mm\\^3 -> 41987\\.12[0-9] mm\\^3\n"
  "\n  Round \\(F3, fillet\\): added after Extrude1\n"
  "\nGeometry:\n  Body1 \\(F2\\.b0\\): volume 24000 -> 41987\\.12[0-9] mm\\^3 \\(\\+17987\\.12[0-9], \\+74\\.95%\\); area 6800 -> 8885\\.4[0-9]+ mm\\^2 \\(\\+2085\\.4[0-9]+, \\+30\\.67%\\)\n$")
cli(ok diff "${part}" HEAD~1 --geometry --json)
string(JSON to TYPE "${out}" to)
string(JSON volume GET "${out}" geometry volume to)
if(NOT to STREQUAL "NULL" OR NOT volume MATCHES "^41987\\.12")
  message(FATAL_ERROR "a version and the saved file with the geometry, as JSON:\n${out}")
endif()
cli(ok diff "${part}" HEAD~1 HEAD --geometry)
expect("the versions with the geometry"
  "^Changes in part\\.mitcad from [0-9a-f]+ to [0-9a-f]+:\n"
  "\nGeometry:\n  Body1 \\(F2\\.b0\\): volume 24000 -> 41987\\.12")

# Wrong arguments and versions that are not there.
cli(usage diff "${part}")
cli(usage diff "${part}" HEAD HEAD~1 HEAD~2)
cli(fail diff "${part}" 0000000)
expect("a version that is not there" "there is no version '0000000'")
cli(fail diff "${WORK}/first.mitcad" "${WORK}/missing.mitcad")
expect("a file that is not there" "missing\\.mitcad")
message(STATUS "comparison of versions: two versions, a version and the saved file, two files")
