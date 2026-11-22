# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# GitHub #526's reduced pipe network (mitcad#2175, a follow-up of
# mitcad#2157; never committed): the `.sc` file in `github-526/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits of
# its SHA-256), imported by mitcad-cli import-solidcraft. A format-63 design
# of three sketches of 3D lines and three round tube members (diameters 63
# and 40, walls 3 and 1.9, one run rising 1.5 m). Checked: no newer-design
# warning; every feature parametric without a note, no placeholder; each
# sketch a 3D sketch with all its lines; one Pipe per line of each member
# (the timeline holds 3D sketches and pipes only, each computing), each
# body named as the file names it with the volume and box of a tube of the
# member's diameter and wall along its line (pi/4 (D^2 - (D - 2 wall)^2)
# times the line's length) and valid by the kernel's checker; the design
# saved opens again with every feature computing. Skipped when the folder
# or the file is not there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P solidcraft-pipe-network-reported-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "solidcraft-pipe-network-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "87dfc069") # the reporter's reduced file

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-526")
  message(STATUS "solidcraft pipe network reported: skipped (MITCAD_MITCAD_REPORTED/github-526 is not there)")
  return()
endif()
file(GLOB candidates LIST_DIRECTORIES false "${reported}/github-526/*")
set(design "")
foreach(candidate IN LISTS candidates)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL design_id)
    set(design "${candidate}")
    break()
  endif()
endforeach()
if(design STREQUAL "")
  message(STATUS "solidcraft pipe network reported: skipped (no file ${design_id} in ${reported}/github-526)")
  return()
endif()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")

# The bodies: a tube of 63 x 3 (180 pi mm2) along the main run's three
# lines (5966, 3932 and the rising 1466) and the first branch (4000), one
# of 40 x 1.9 (72.39 pi mm2) along the second branch's two (2976.5 each).
file(WRITE "${WORK}/expected.json" [=[
{
  "volume_tolerance": 1e-6,
  "bbox_tolerance": 1e-3,
  "body_count": 6,
  "total": {"volume_mm3": 10041967.933582747},
  "bodies": [
    {"name": "Tube_Main_d63", "volume_mm3": 3373693.518837007,
     "bbox": {"min": [0, -31.5, -31.5], "max": [5966, 31.5, 31.5]}},
    {"name": "Tube_Main_d63 (2)", "volume_mm3": 2223493.616504712,
     "bbox": {"min": [5968.5, 34, -31.5], "max": [6031.5, 3966, 31.5]}},
    {"name": "Tube_Main_d63 (3)", "volume_mm3": 829003.4694292747,
     "bbox": {"min": [5968.5, 3968.5, 34], "max": [6031.5, 4031.5, 1500]}},
    {"name": "Tube_Branch_1_d63", "volume_mm3": 2261946.7105846512,
     "bbox": {"min": [0, 1968.5, -31.5], "max": [4000, 2031.5, 31.5]}},
    {"name": "Tube_Branch_2_d40", "volume_mm3": 676915.3091135507,
     "bbox": {"min": [-20, 6000, -20], "max": [20, 8976.5, 20]}},
    {"name": "Tube_Branch_2_d40 (2)", "volume_mm3": 676915.3091135507,
     "bbox": {"min": [23.5, 8980, -20], "max": [3000, 9020, 20]}}
  ],
  "parametric": 6,
  "valid": true
}
]=])

execute_process(COMMAND "${CLI}" import-solidcraft "${design}" --expected "${WORK}/expected.json"
                        --save "${WORK}/design.mitcad" --json
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "the import or its check failed (${status}):\n${out}\n${err}")
endif()

# fail(<what>): the test fails with the output.
macro(fail what)
  message(FATAL_ERROR "${what}\n${out}")
endmacro()

# Read as a design of format 63, without the newer-design warning.
string(JSON format GET "${out}" import format)
if(NOT format EQUAL 63)
  fail("format ${format}, expected 63")
endif()
string(JSON newer ERROR_VARIABLE no_newer GET "${out}" import newer)
if(no_newer STREQUAL "NOTFOUND")
  fail("the newer-design warning: ${newer}")
endif()
string(JSON passed GET "${out}" import check passed)
if(NOT passed)
  fail("the check did not pass")
endif()

# Six features of the file, three sketches and three members, each
# parametric without a note (a placeholder is unmapped or failed).
string(JSON features LENGTH "${out}" import features)
if(NOT features EQUAL 6)
  fail("${features} features of the file, expected 6")
endif()
set(sketches 0)
set(members 0)
math(EXPR last "${features} - 1")
foreach(i RANGE ${last})
  string(JSON name GET "${out}" import features ${i} name)
  string(JSON kind GET "${out}" import features ${i} kind)
  string(JSON outcome GET "${out}" import features ${i} outcome)
  string(JSON notes LENGTH "${out}" import features ${i} notes)
  if(NOT outcome STREQUAL "parametric" OR NOT notes EQUAL 0)
    fail("${name} (${kind}) came in ${outcome} with ${notes} notes")
  endif()
  if(kind STREQUAL "sketch")
    math(EXPR sketches "${sketches} + 1")
  elseif(kind STREQUAL "frame_member")
    math(EXPR members "${members} + 1")
  endif()
endforeach()
if(NOT sketches EQUAL 3 OR NOT members EQUAL 3)
  fail("${sketches} sketches and ${members} frame members, expected 3 and 3")
endif()

# The sketches' 3D lines all came in: 3, 1 and 2, none left out.
string(JSON count LENGTH "${out}" import sketches)
if(NOT count EQUAL 3)
  fail("${count} sketches in the report, expected 3")
endif()
set(lines "")
foreach(i RANGE 2)
  string(JSON curves GET "${out}" import sketches ${i} curves)
  string(JSON dropped LENGTH "${out}" import sketches ${i} dropped)
  if(NOT dropped EQUAL 0)
    fail("sketch ${i}: ${dropped} items left out")
  endif()
  list(APPEND lines "${curves}")
endforeach()
if(NOT lines STREQUAL "3;1;2")
  fail("the sketches' curves ${lines}, expected 3, 1 and 2")
endif()

# The timeline: three 3D sketches and six pipes (one per line), each
# computing; no planar sketch, sweep or placeholder.
string(JSON timeline LENGTH "${out}" document timeline features)
if(NOT timeline EQUAL 9)
  fail("${timeline} timeline features, expected 9")
endif()
set(spatial 0)
set(pipes 0)
math(EXPR last "${timeline} - 1")
foreach(i RANGE ${last})
  string(JSON name GET "${out}" document timeline features ${i} name)
  string(JSON type GET "${out}" document timeline features ${i} type)
  string(JSON state GET "${out}" document timeline features ${i} status)
  if(NOT state STREQUAL "ok")
    fail("${name} (${type}) is ${state}")
  endif()
  if(type STREQUAL "spatial_sketch")
    math(EXPR spatial "${spatial} + 1")
  elseif(type STREQUAL "pipe")
    math(EXPR pipes "${pipes} + 1")
  else()
    fail("${name} is a ${type}, expected 3D sketches and pipes only")
  endif()
endforeach()
if(NOT spatial EQUAL 3 OR NOT pipes EQUAL 6)
  fail("${spatial} 3D sketches and ${pipes} pipes, expected 3 and 6")
endif()

# Saved, it opens again with every feature computing.
execute_process(COMMAND "${CLI}" info "${WORK}/design.mitcad"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0 OR out MATCHES "\\): error:")
  message(FATAL_ERROR "the saved design does not open with every feature computing (${status}):\n${out}\n${err}")
endif()

message(STATUS "ok   the reported pipe network: 3 3D sketches, 6 pipes of the file's diameters and walls, all parametric and valid")
