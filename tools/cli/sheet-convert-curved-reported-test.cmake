# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Convert to Sheet Metal on the reported transition (mitcad#2435, GitHub
# #684; never committed): the report's zip under `github-684/` in
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits
# of its SHA-256), its design unpacked into WORK and opened by mitcad-cli
# run with sheet_convert_curved_reported.json: the loft's four sides are
# refused, each named with how much it bends two ways; the neck's cylinder
# (four faces stored as splines) of the body shelled and split becomes one
# sheet whose blank and folded volume are the analytic ones, and its Flat
# Pattern a DXF. Skipped when the folder or the zip is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P sheet-convert-curved-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "sheet-convert-curved-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(zip_id "ec953d93")

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-684")
  message(STATUS "sheet convert curved reported: skipped (no github-684 under MITCAD_MITCAD_REPORTED)")
  return()
endif()
file(GLOB zips LIST_DIRECTORIES false "${reported}/github-684/*.zip")
set(zip "")
foreach(candidate IN LISTS zips)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL zip_id)
    set(zip "${candidate}")
    break()
  endif()
endforeach()
if(zip STREQUAL "")
  message(STATUS "sheet convert curved reported: skipped (no zip ${zip_id} in github-684)")
  return()
endif()
# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(ARCHIVE_EXTRACT INPUT "${zip}" DESTINATION "${WORK}")
if(NOT EXISTS "${WORK}/design.mitcad")
  message(FATAL_ERROR "the report's zip holds no design.mitcad")
endif()
execute_process(COMMAND "${CLI}" run "${SCRIPTS}/sheet_convert_curved_reported.json" --open "${WORK}/design.mitcad"
                WORKING_DIRECTORY "${WORK}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "sheet_convert_curved_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the loft's sides are refused as bending two ways; the neck converts and its blank is exact")
