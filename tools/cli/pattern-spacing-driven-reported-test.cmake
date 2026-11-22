# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A sketch pattern's spacing from a driven dimension of its own sketch
# (mitcad#2057, GitHub #470) on the reported design (never committed): its
# archive in `github-470/` under MITCAD_MITCAD_REPORTED, found by its
# content id (the first 8 hex digits of its SHA-256), the design in it by
# its own, unpacked into WORK. Opened by mitcad-cli run with
# pattern_spacing_driven_reported.json: the pattern's spacing set to the
# driven length of the sketch's construction line is accepted, the copies
# lie that far along the pattern's direction, and they follow a new width
# by the new measure. Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P pattern-spacing-driven-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "pattern-spacing-driven-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "b007f76f") # the reporter's archive
set(design_id "cfac1994") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "pattern spacing driven reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-470/*.zip")
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
  message(STATUS "pattern spacing driven reported: skipped (no archive ${archive_id} in ${reported}/github-470)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/pattern_spacing_driven_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "pattern_spacing_driven_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the pattern's spacing uses the driven length of its own sketch, and the copies follow it")
