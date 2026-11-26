# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# The roundings of GitHub #635's design past its counterbores (mitcad#2365;
# never committed): the archive in `github-635/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits of
# its SHA-256), the design in it by its own, unpacked into WORK and opened
# by mitcad-cli run with fillet_counterbore_seam_reported.json. The block's
# end edges are rounded with 2 where a counterbore's rim comes 1 mm from
# each end face; the counterbore's wall cuts each rounding alike at both
# ends (at one of them the wall's seam runs up from the rim under the
# rounding, OCCT port patch 2365): the design's own set, each chamfer edge
# alone, both together and one end's pair, one valid solid without a note
# each time, the volumes and the faces beside each counterbore those of the
# mirror image. Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-counterbore-seam-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-counterbore-seam-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "292067b3") # the reporter's archive
set(design_id "14547552") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-635")
  message(STATUS "fillet counterbore seam reported: skipped (MITCAD_MITCAD_REPORTED/github-635 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-635/*.zip")
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
  message(STATUS "fillet counterbore seam reported: skipped (no archive ${archive_id} in ${reported}/github-635)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/fillet_counterbore_seam_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "fillet_counterbore_seam_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported roundings are cut by each counterbore's wall alike at both ends")
