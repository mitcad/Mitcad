# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# GitHub #517's Y tube (mitcad#2140, port patch 2140; never committed): the
# archive in `github-517/` under MITCAD_MITCAD_REPORTED, found by its content
# id (the first 8 hex digits of its SHA-256), the design in it by its own,
# unpacked into WORK and opened by mitcad-cli run with
# y_tube_crotch_reported.json. The crotch's Fillet1 at the design's 6 mm and
# at the reporter's 50 mm, and the Shell of the reporter's screenshot after
# each (the three ends open, 2 mm inside, Sharp): each one valid solid
# without a note. Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P y-tube-crotch-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "y-tube-crotch-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "419d1d75") # the reporter's archive
set(design_id "c80c4526") # design.mitcad in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-517")
  message(STATUS "y tube crotch reported: skipped (MITCAD_MITCAD_REPORTED/github-517 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-517/*.zip")
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
  message(STATUS "y tube crotch reported: skipped (no archive ${archive_id} in ${reported}/github-517)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/y_tube_crotch_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "y_tube_crotch_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported crotch rounded with 6 and 50 and the tube shelled after each, valid without a note")
