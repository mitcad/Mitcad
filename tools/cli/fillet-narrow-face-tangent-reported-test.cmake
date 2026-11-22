# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# The rounding of GitHub #560's design wider than the riser beside its edge,
# which goes on tangentially into a half disc (mitcad#2208; never
# committed): the archive in `github-560/` under MITCAD_MITCAD_REPORTED,
# found by its content id (the first 8 hex digits of its SHA-256), the
# design in it by its own, unpacked into WORK and opened by mitcad-cli run
# with fillet_narrow_face_tangent_reported.json. Fillet2 takes the riser away
# and rolls on over the half disc at its exact size, ending on the start
# face and the cut face, in the design's order (Fillet1 after it), alone and
# after Fillet1: one valid solid without a note each time, the volume the
# section's exact one, within the box of the body before it. Skipped when
# the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-narrow-face-tangent-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-narrow-face-tangent-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "85253318") # the reporter's archive
set(design_id "dfb5add8") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-560")
  message(STATUS "fillet narrow face tangent reported: skipped (MITCAD_MITCAD_REPORTED/github-560 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-560/*.zip")
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
  message(STATUS "fillet narrow face tangent reported: skipped (no archive ${archive_id} in ${reported}/github-560)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/fillet_narrow_face_tangent_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "fillet_narrow_face_tangent_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported rounding takes the riser away and rolls on over the half disc, in both orders")
