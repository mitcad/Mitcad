# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A flanged nozzle's foot rounded where the nozzle meets a dome on a tube
# across the line where the dome runs on tangentially into the tube, on
# GitHub #582's design (mitcad#2272; never committed): the archive in
# `github-582/` under MITCAD_MITCAD_REPORTED, found by its content id (the
# first 8 hex digits of its SHA-256), the design in it by its own, unpacked
# into WORK and opened by mitcad-cli run with each of the scripts
# fillet_nozzle_dome_reported*.json: the original nozzle's foot rounded with
# 1.5 picked by its dome edge and by its tube edge, a pattern copy's foot
# picked by its tube edge, and the foot added to the rounding the pattern
# repeats; each one valid solid at the exact size without a note, the
# copies' roundings still built. The pattern script is the benchmark's last
# two steps (mitcad#2372): the flange's chamfer (Chamfer2) and the rounding
# (Fillet1) before the pattern and CircularPattern2 repeating both, every
# copy chamfered and rounded (its faces there, its sharp edges gone).
# A script that fails is run again with the
# trace of the rounding's walk (MITCAD_TRACE_FILLET_WALK), its end shown.
# Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-nozzle-dome-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-nozzle-dome-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "61355af6") # the reporter's archive
set(design_id "80482199") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-582")
  message(STATUS "fillet nozzle dome reported: skipped (MITCAD_MITCAD_REPORTED/github-582 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-582/*.zip")
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
  message(STATUS "fillet nozzle dome reported: skipped (no archive ${archive_id} in ${reported}/github-582)")
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
foreach(script fillet_nozzle_dome_reported fillet_nozzle_dome_reported_tube fillet_nozzle_dome_reported_copy
               fillet_nozzle_dome_reported_pattern)
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
  message(FATAL_ERROR "fillet nozzle dome reported: failed: ${failed}")
endif()
message(STATUS "ok   the nozzle's foot rounded with 1.5 across the dome's edge with the tube, one valid solid without a note")
