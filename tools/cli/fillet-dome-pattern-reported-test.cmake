# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# The benchmark's last step on GitHub #577's design (mitcad#2380; never
# committed): the reporter's own build of the public nozzle-on-a-dome
# benchmark up to its last step, the archive in `github-577/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits of
# its SHA-256), the design in it by its own, unpacked into WORK and opened by
# mitcad-cli run with fillet_dome_pattern_reported.json: the step itself, a
# circular pattern of the nozzle's features (with both chamfers and both
# roundings) four times about the dome's axis, one valid solid without a
# note, every copy chamfered and rounded (its dressup faces there, its sharp
# edges gone). The same check on GitHub #582's design is
# cli.fillet_nozzle_dome_reported (mitcad#2272, mitcad#2372).
# A script that fails is run again with the trace of the rounding's walk
# (MITCAD_TRACE_FILLET_WALK), its end shown.
# Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-dome-pattern-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-dome-pattern-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "80aeb041") # the reporter's archive
set(design_id "203fbbe1") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-577")
  message(STATUS "fillet dome pattern reported: skipped (MITCAD_MITCAD_REPORTED/github-577 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-577/*.zip")
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
  message(STATUS "fillet dome pattern reported: skipped (no archive ${archive_id} in ${reported}/github-577)")
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

set(failed "")
foreach(script fillet_dome_pattern_reported)
  execute_process(COMMAND "${CLI}" run "${SCRIPTS}/${script}.json" --open "${design}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(status EQUAL 0)
    message(STATUS "ok   ${script}")
    continue()
  endif()
  # Again with the walk's trace: its end says where the rounding stopped.
  execute_process(COMMAND "${CMAKE_COMMAND}" -E env MITCAD_TRACE_FILLET_WALK=1
                          "${CLI}" run "${SCRIPTS}/${script}.json" --open "${design}"
                  RESULT_VARIABLE traced OUTPUT_VARIABLE trace_out ERROR_VARIABLE trace_err)
  string(LENGTH "${trace_err}" length)
  if(length GREATER 12000)
    math(EXPR from "${length} - 12000")
    string(SUBSTRING "${trace_err}" ${from} 12000 trace_err)
  endif()
  message(STATUS "FAIL ${script} (${status}):\n${out}${err}\n--- the end of the walk's trace:\n${trace_err}")
  list(APPEND failed "${script}")
endforeach()
if(failed)
  message(FATAL_ERROR "fillet dome pattern reported: failed: ${failed}")
endif()
message(STATUS "ok   the nozzle's features patterned four times about the dome's axis, every copy chamfered and rounded, one valid solid without a note")
