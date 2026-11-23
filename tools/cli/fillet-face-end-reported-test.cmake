# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A rounding with Tangent Chain off whose edge runs smoothly into a corner
# rounded with the same radius, on GitHub #553's design (mitcad#2193; never
# committed): the archive in `github-553/` under MITCAD_MITCAD_REPORTED,
# found by its content id (the first 8 hex digits of its SHA-256), the
# design in it by its own, unpacked into WORK and opened by mitcad-cli run
# with fillet_face_end_reported.json: the lip's edge rounded with 2 ends
# against the pocket wall's rounding, one valid solid without a note.
# Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-face-end-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-face-end-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "b7383fe8") # the reporter's archive
set(design_id "d6e1c68b") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-553")
  message(STATUS "fillet face end reported: skipped (MITCAD_MITCAD_REPORTED/github-553 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-553/*.zip")
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
  message(STATUS "fillet face end reported: skipped (no archive ${archive_id} in ${reported}/github-553)")
  return()
endif()

# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/unpacked")
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/unpacked/*.mitcad")
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

# With the run-out's trace on (MITCAD_DEBUG_RUNOUT): a failure's output
# says which ending gave up where.
execute_process(COMMAND "${CMAKE_COMMAND}" -E env MITCAD_DEBUG_RUNOUT=1
                        "${CLI}" run "${SCRIPTS}/fillet_face_end_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "fillet_face_end_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the lip's edge rounded with 2 ends against the pocket wall's rounding, one valid solid without a note")
