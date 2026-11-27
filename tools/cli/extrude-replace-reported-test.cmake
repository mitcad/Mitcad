# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Extrude and Replace Face on a reported design (GitHub #674, mitcad#2538;
# never committed): its archive in `github-531/` under MITCAD_MITCAD_REPORTED
# (the report's attachment, triage card 531), found by its content id (the
# first 8 hex digits of its SHA-256), the design in it by its own, unpacked
# into WORK. Opened by mitcad-cli run with extrude_replace_reported674.json:
# the ends of ribs replaced by the rounding of the bowl they stick out of
# merge with it (no outlines left on it); the ribs up to the body or To Next
# fail in words where they reach past the bowl; the reporter's Join of a tab
# on the rim up to a rib's underside To Adjacent Faces fails in words that
# name the part without an end, and up to the body it builds. Skipped when
# the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P extrude-replace-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "extrude-replace-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "04c1268b") # the reporter's archive
set(design_id "9f9dbf83") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "extrude replace reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-531/*.zip")
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
  message(STATUS "extrude replace reported: skipped (no archive ${archive_id} in ${reported}/github-531)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/extrude_replace_reported674.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "extrude_replace_reported674 failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the replaced rib ends merge with the rounding; the unended parts are named in words")
