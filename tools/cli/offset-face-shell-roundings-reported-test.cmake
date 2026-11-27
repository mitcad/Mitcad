# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Offset Face on the inside of a shelled block with rounded edges
# (mitcad#2457, GitHub #678) on a reported design (never committed): its
# archive in `github-678/` under MITCAD_MITCAD_REPORTED (the report's
# attachment), found by its content id (the first 8 hex digits of its
# SHA-256), the design in it by its own, unpacked into WORK. Opened by
# mitcad-cli run with offset_face_shell_roundings_reported.json: the inner
# roundings moved 2 mm one at a time, three together and all four, which
# the report's application crashed on (OCCT port patches 2418 and 2457),
# build. Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P offset-face-shell-roundings-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "offset-face-shell-roundings-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "6432a1d2") # the reporter's archive
set(design_id "2889c5a1") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "offset face shell roundings reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-678/*.zip")
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
  message(STATUS "offset face shell roundings reported: skipped (no archive ${archive_id} in ${reported}/github-678)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/offset_face_shell_roundings_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "offset_face_shell_roundings_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported inner roundings move one at a time, three together and all four")
