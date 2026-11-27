# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Move Face, Offset Face and Press Pull on the reported designs of GitHub
# #714, #727 and #731 (mitcad#2568; follow-up of mitcad#2543, mitcad#2547 and
# mitcad#2548; never committed): their archives in `github-714/`,
# `github-727/` and `github-731/` under MITCAD_MITCAD_REPORTED (the issues'
# numbers; also found in `github-586/`, `github-632/` and `github-635/`, the
# triage cards' numbers), each found by its content id (the first 8 hex digits
# of its SHA-256), the design in it by its own, unpacked into WORK. Each
# script of `move_face_reported<issue>_*.json` runs on its design with
# mitcad-cli run --open: every case previewed and committed, one valid body
# with the expected volume (or the design's own, kept before a rigid move),
# the moved faces' places and names.
#
# - #714: the round boss's wall, chamfer and cap moved (12.4, 0, 0); the
#   eight-arc boss's sides and cap moved (0, 8, 0).
# - #727: the 17 faces of the filleted eight-arc boss (MoveFace1) moved
#   +2.875 and +3 along y, turned 31.5 and 32 degrees about z, and turned a
#   few degrees past the reporter's x and y limits, about the pivot the
#   application gives a free move of these faces.
# - #731: the raised block's end moved short of the plate's side, flush with
#   it, just past it and clearly past it by Move Face, Offset Face and Press
#   Pull (Straight, Along the walls); Move Face up to the plate's side and
#   up to the plate's body.
#
# Every script runs even when one fails, and the failures are listed
# together at the end. A design whose archive is not there is skipped with a
# line saying so; with none of them there the test is skipped.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P move-face-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "move-face-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "move face reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()

# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")

# The design `design_id` in the archive `archive_id` in one of `folders`
# under MITCAD_MITCAD_REPORTED, unpacked into WORK/<archive_id>; empty in
# `out` when the archive is not there.
function(reported_design out archive_id design_id)
  set(folders ${ARGN})
  set(archive "")
  foreach(folder IN LISTS folders)
    file(GLOB archives LIST_DIRECTORIES false "${reported}/${folder}/*.zip")
    foreach(candidate IN LISTS archives)
      file(SHA256 "${candidate}" sum)
      string(SUBSTRING "${sum}" 0 8 id)
      if(id STREQUAL archive_id)
        set(archive "${candidate}")
        break()
      endif()
    endforeach()
    if(NOT archive STREQUAL "")
      break()
    endif()
  endforeach()
  if(archive STREQUAL "")
    set(${out} "" PARENT_SCOPE)
    return()
  endif()
  file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/${archive_id}")
  file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/${archive_id}/*.mitcad")
  foreach(candidate IN LISTS designs)
    file(SHA256 "${candidate}" sum)
    string(SUBSTRING "${sum}" 0 8 id)
    if(id STREQUAL design_id)
      set(${out} "${candidate}" PARENT_SCOPE)
      return()
    endif()
  endforeach()
  message(FATAL_ERROR "the archive ${archive_id} (${archive}) holds no design ${design_id}")
endfunction()

reported_design(round "2957ea2a" "32057dd7" github-714 github-586)
reported_design(arcs "362ab8a8" "e43cd5da" github-714 github-586)
reported_design(filleted "ba115373" "d9329e38" github-727 github-632)
reported_design(stepped "a98288ab" "1c9b95b5" github-731 github-635)

# Each script with the design it runs on.
set(cases
  "move_face_reported714_round|${round}"
  "move_face_reported714_arcs|${arcs}"
  "move_face_reported727_moved|${filleted}"
  "move_face_reported727_turned_z|${filleted}"
  "move_face_reported727_turned_x|${filleted}"
  "move_face_reported727_turned_y|${filleted}"
  "move_face_reported731_move|${stepped}"
  "move_face_reported731_offset|${stepped}"
  "move_face_reported731_straight|${stepped}"
  "move_face_reported731_along|${stepped}"
  "move_face_reported731_to_object|${stepped}")

set(ran 0)
set(failures "")
foreach(entry IN LISTS cases)
  string(FIND "${entry}" "|" bar)
  string(SUBSTRING "${entry}" 0 ${bar} script)
  math(EXPR after "${bar} + 1")
  string(SUBSTRING "${entry}" ${after} -1 design)
  if(design STREQUAL "")
    message(STATUS "skip ${script}: its reported design is not there")
    continue()
  endif()
  math(EXPR ran "${ran} + 1")
  execute_process(COMMAND "${CLI}" run "${SCRIPTS}/${script}.json" --open "${design}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(status EQUAL 0)
    message(STATUS "ok   ${script}")
  else()
    message(STATUS "FAIL ${script}")
    string(APPEND failures "\n${script} failed (${status}):\n${err}${out}")
  endif()
endforeach()

if(ran EQUAL 0)
  message(STATUS "move face reported: skipped (none of the reported designs is under ${reported})")
  return()
endif()
if(NOT failures STREQUAL "")
  message(FATAL_ERROR "move face reported:${failures}")
endif()
message(STATUS "ok   the reported Move Face, Offset Face and Press Pull cases preview and commit")
