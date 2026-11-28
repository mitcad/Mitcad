# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Whole circular rims rounded at their size (mitcad#2582, port patch 2582)
# on the reported designs of GitHub mitcad/Mitcad#741 (never committed):
# their three archives in `github-741/` under MITCAD_MITCAD_REPORTED, each
# found by its content id (the first 8 hex digits of its SHA-256), the
# design in it by its own, unpacked into WORK. Each opened by mitcad-cli
# run with its script: the cylinder's rim rounded into a hemisphere
# (fillet_whole_rims_reported1.json), then its other end's face into a ball
# (fillet_whole_rims_reported2.json), the annulus's four rims into a torus
# (fillet_whole_rims_reported3.json); each at its size, without a note.
# fillet_whole_rims.json checks the same on designs made from commands.
# Skipped when the folder or an archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-whole-rims-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-whole-rims-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

# archive id, design id, script, for each design
set(cases
  "24d4d0c3|320404ff|fillet_whole_rims_reported1.json"
  "0d41a8c0|f4810210|fillet_whole_rims_reported2.json"
  "1ec9e7b0|2e5539f2|fillet_whole_rims_reported3.json")

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "fillet whole rims reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-741/*.zip")

# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(index 0)
foreach(case IN LISTS cases)
  string(REPLACE "|" ";" parts "${case}")
  list(GET parts 0 archive_id)
  list(GET parts 1 design_id)
  list(GET parts 2 script)
  math(EXPR index "${index} + 1")
  set(archive "")
  foreach(candidate IN LISTS archives)
    file(SHA256 "${candidate}" sum)
    string(SUBSTRING "${sum}" 0 8 id)
    if(id STREQUAL archive_id)
      set(archive "${candidate}")
      break()
    endif()
  endforeach()
  if(archive STREQUAL "")
    message(STATUS "fillet whole rims reported: skipped (no archive ${archive_id} in ${reported}/github-741)")
    return()
  endif()
  file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/unpacked${index}")
  file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/unpacked${index}/*.mitcad")
  set(design "")
  foreach(candidate IN LISTS designs)
    file(SHA256 "${candidate}" sum)
    string(SUBSTRING "${sum}" 0 8 id)
    if(id STREQUAL design_id)
      set(design "${candidate}")
      break()
    endif()
  endforeach()
  if(design STREQUAL "")
    message(FATAL_ERROR "the archive ${archive_id} holds no design ${design_id}")
  endif()
  execute_process(COMMAND "${CLI}" run "${SCRIPTS}/${script}" --open "${design}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "${script} failed (${status}):\n${out}${err}")
  endif()
  message(STATUS "ok   ${script}")
endforeach()
message(STATUS "ok   the reported rims rounded into a hemisphere, a ball and a torus, each at its size")
