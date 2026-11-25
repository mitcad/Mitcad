# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Replace Face of a cylinder's round top onto a slanted planar face
# (mitcad#2382, GitHub #643) on a reported design (never committed): its
# archive in `github-643/` under MITCAD_MITCAD_REPORTED (the report's
# attachment, stored as `github-643/default.zip`, mitcad#2407), found by its
# content id (the first 8 hex digits of its SHA-256), the design in it by its
# own, unpacked into WORK. The design's file name is not ASCII (the archive
# also carries it as a UTF-8 path field); it is found by its content id, so
# only its `.mitcad` ending matters. Opened by mitcad-cli run with
# replace_face_tilted_round_reported.json: the tops of two cylinders standing
# in a shelled box replaced by the plane of a slanted surface strip build,
# each new top the ellipse where the cylinder meets the plane. Skipped when
# the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P replace-face-tilted-round-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "replace-face-tilted-round-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "a62b7c34") # the reporter's archive
set(design_id "9729242c") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "replace face tilted round reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-643/*.zip")
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
  message(STATUS "replace face tilted round reported: skipped (no archive ${archive_id} in ${reported}/github-643)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/replace_face_tilted_round_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "replace_face_tilted_round_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported cylinder tops are replaced by the slanted plane, each an ellipse on it")
