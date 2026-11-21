# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# The concave edge of GitHub #449's design rounded and chamfered with 1 mm
# (mitcad#2039, OCCT port patch 1830; never committed): the archive in
# `github-449/` under MITCAD_MITCAD_REPORTED, found by its content id (the
# first 8 hex digits of its SHA-256), the design in it by its own, unpacked
# into WORK and opened by mitcad-cli run with
# fillet_tangent_seam_reported.json. Both ends of the edge sit where a wall's
# flat side runs on tangentially into the low block's curved side and into
# the wall's curved top: the fillet (the design's Fillet2) and a chamfer in
# its place each build one valid solid without a note, the fillet adding
# (1 - pi/4) r^2 L less what the two curved faces cut off at the ends, the
# chamfer r^2 L / 2 less the same (L = 10 mm). Skipped when the folder or the
# archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-tangent-seam-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-tangent-seam-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "8b0e6902") # the reporter's fillet_bug.zip
set(design_id "92b086c6") # fillet_bug.mitcad in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-449")
  message(STATUS "fillet tangent seam reported: skipped (MITCAD_MITCAD_REPORTED/github-449 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-449/*.zip")
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
  message(STATUS "fillet tangent seam reported: skipped (no archive ${archive_id} in ${reported}/github-449)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/fillet_tangent_seam_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "fillet_tangent_seam_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported edge rounded and chamfered with 1 mm, each one valid solid without a note")
