# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A mirror of features rebuilt at the image (mitcad#2430, GitHub #640) on a
# reported design (never committed): its archive in `github-640/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits
# of its SHA-256), the design in it by its own, unpacked into WORK. Opened
# by mitcad-cli run with mirror_to_next_reported.json: the design's mirror,
# saved before mitcad#2430 without a compute option, keeps its result as
# saved; edited so that it stores Adjust, the mirrored extrusions are built
# again at the image and reach the faces on their side. Skipped when the
# folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P mirror-to-next-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "mirror-to-next-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "3e00396c") # the reporter's archive
set(design_id "410b79b2") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "mirror to next reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-640/*.zip")
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
  message(STATUS "mirror to next reported: skipped (no archive ${archive_id} in ${reported}/github-640)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/mirror_to_next_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "mirror_to_next_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the saved mirror keeps its result; edited to Adjust, its extrusions are built again at the image")
