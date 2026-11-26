# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A sketch on a drafted face after the Draft's fixed plane changes
# (mitcad#2408, follow-up of mitcad#2351, GitHub #622) on the reported design
# (never committed): its archive in `github-622/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits of
# its SHA-256), the design in it by its own, unpacked into WORK. Opened by
# mitcad-cli run with draft_neutral_plane_reported.json: with the Draft about
# the offset plane Plane2, as saved, every feature computes and the sketch
# lies on the drafted face; it follows the face when the fixed plane goes
# back to the XZ plane and to Plane2 again, and Redefine Sketch Plane takes
# the drafted face. Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P draft-neutral-plane-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "draft-neutral-plane-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "f29ca2b8") # the reporter's archive
set(design_id "920fb50d") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "draft neutral plane reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-622/*.zip")
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
  message(STATUS "draft neutral plane reported: skipped (no archive ${archive_id} in ${reported}/github-622)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/draft_neutral_plane_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "draft_neutral_plane_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the sketch on the reported drafted face follows the Draft's fixed plane and is redefined onto it")
