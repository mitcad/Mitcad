# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A reported sheet-metal design keeps its sheet (mitcad#2117, GitHub #497):
# its archive in `github-497/` under MITCAD_MITCAD_REPORTED (never
# committed), found by its content id (the first 8 hex digits of its
# SHA-256), the design in it by its own, unpacked into WORK. Imported with
# mitcad-cli import-f3d: its base flange, its three edge flanges and its
# Unfold and Refold come in parametric, their inputs read from the file and
# verified against the file's history; every final body matches the file's
# by volume (within 1e-4); at the end at least one body is a valid sheet,
# and each valid sheet has a valid flat pattern (mitcad-cli run on the
# imported design). Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P f3d-sheet-unfold-reported-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "f3d-sheet-unfold-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "50589b55") # the reporter's archive
set(design_id "e3d580a6") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-497")
  message(STATUS "f3d sheet unfold reported: skipped (MITCAD_MITCAD_REPORTED/github-497 is not there)")
  return()
endif()

function(find_by_id folder pattern id result)
  file(GLOB_RECURSE candidates LIST_DIRECTORIES false "${folder}/${pattern}")
  set(${result} "" PARENT_SCOPE)
  foreach(candidate IN LISTS candidates)
    file(SHA256 "${candidate}" sum)
    string(SUBSTRING "${sum}" 0 8 found)
    if(found STREQUAL id)
      set(${result} "${candidate}" PARENT_SCOPE)
      return()
    endif()
  endforeach()
endfunction()

find_by_id("${reported}/github-497" "*.zip" "${archive_id}" archive)
if(archive STREQUAL "")
  message(STATUS "f3d sheet unfold reported: skipped (no archive ${archive_id} in ${reported}/github-497)")
  return()
endif()

# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/unpacked")
find_by_id("${WORK}/unpacked" "*.f3d" "${design_id}" design)
if(design STREQUAL "")
  message(FATAL_ERROR "the archive ${archive_id} holds no design ${design_id}")
endif()

execute_process(
  COMMAND "${CLI}" import-f3d "${design}" --report "${WORK}/report.json"
          --save "${WORK}/imported.mitcad" --json
  OUTPUT_FILE "${WORK}/import.json" ERROR_VARIABLE err RESULT_VARIABLE status)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "mitcad-cli import-f3d failed (${status}):\n${err}")
endif()
file(READ "${WORK}/report.json" report)

set(problems "")

# The sheet items: parametric, their inputs read, verified.
set(sheet_items BaseFlange22 EdgeFlange23 EdgeFlange24 EdgeFlange25 Unfold6 Refold5)
set(seen "")
string(JSON count LENGTH "${report}" designs 0 items)
math(EXPR last "${count} - 1")
foreach(i RANGE ${last})
  string(JSON name GET "${report}" designs 0 items ${i} name)
  list(FIND sheet_items "${name}" wanted)
  if(wanted EQUAL -1)
    continue()
  endif()
  list(APPEND seen "${name}")
  string(JSON outcome GET "${report}" designs 0 items ${i} outcome)
  string(JSON verified ERROR_VARIABLE no_verified GET "${report}" designs 0 items ${i} verified)
  string(JSON provenance ERROR_VARIABLE no_provenance GET "${report}" designs 0 items ${i}
         input_provenance)
  string(JSON note ERROR_VARIABLE no_note GET "${report}" designs 0 items ${i} note)
  if(no_note)
    set(note "")
  endif()
  if(NOT outcome STREQUAL "parametric")
    list(APPEND problems "${name}: ${outcome}, not parametric (${note})")
  elseif(no_verified OR NOT verified)
    list(APPEND problems "${name}: parametric, not verified against the file's history (${note})")
  elseif(name MATCHES "^(EdgeFlange|Unfold|Refold)" AND (no_provenance OR NOT provenance STREQUAL "read"))
    list(APPEND problems "${name}: its inputs ${provenance}, not read (${note})")
  else()
    message(STATUS "ok   ${name}: parametric, verified (${note})")
  endif()
endforeach()
foreach(name IN LISTS sheet_items)
  list(FIND seen "${name}" found)
  if(found EQUAL -1)
    list(APPEND problems "${name}: not in the import report")
  endif()
endforeach()

# The final bodies against those stored in the file, by volume.
string(JSON body_count LENGTH "${report}" designs 0 bodies)
if(body_count EQUAL 0)
  list(APPEND problems "no stored body compared")
else()
  math(EXPR last_body "${body_count} - 1")
  foreach(b RANGE ${last_body})
    string(JSON file GET "${report}" designs 0 bodies ${b} file)
    string(JSON dv ERROR_VARIABLE dv_missing GET "${report}" designs 0 bodies ${b} volume_difference)
    if(dv_missing)
      list(APPEND problems "the stored body ${file} has no body in the import")
      continue()
    endif()
    string(REGEX REPLACE "^-" "" dv_abs "${dv}")
    if(dv_abs MATCHES "[eE]-")
      set(dv_abs 0)
    endif()
    if(dv_abs GREATER 0.0001)
      list(APPEND problems "the body of ${file} differs in volume by ${dv} (relative)")
    else()
      message(STATUS "ok   the body of ${file} matches the file's by volume (${dv})")
    endif()
  endforeach()
endif()

# At the end of the timeline: a valid sheet with a valid flat pattern.
file(READ "${WORK}/import.json" imported)
string(JSON count LENGTH "${imported}" document bodies)
set(sheets 0)
if(count GREATER 0)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON uid GET "${imported}" document bodies ${i} uid)
    file(WRITE "${WORK}/sheet-${i}.json"
         "[{\"comment\":\"SPDX-License-Identifier: MIT. Written by f3d-sheet-unfold-reported-test.cmake.\"},\n"
         " {\"expect\":{\"query\":{\"query\":\"sheet_state\",\"body\":\"${uid}\"},\"result\":{\"status\":\"valid\"}}}]\n")
    execute_process(COMMAND "${CLI}" run "${WORK}/sheet-${i}.json" --open "${WORK}/imported.mitcad"
                    RESULT_VARIABLE status OUTPUT_QUIET ERROR_QUIET)
    if(NOT status EQUAL 0)
      continue()
    endif()
    math(EXPR sheets "${sheets} + 1")
    file(WRITE "${WORK}/flat-${i}.json"
         "[{\"comment\":\"SPDX-License-Identifier: MIT. Written by f3d-sheet-unfold-reported-test.cmake: the body's Flat Pattern item made, then its flat pattern asked for.\"},\n"
         " {\"cmd\":\"set_flat_pattern\",\"body\":\"${uid}\"},\n"
         " {\"expect\":{\"query\":{\"query\":\"flat_pattern\",\"body\":\"${uid}\"},\"result\":{\"status\":\"valid\"}}}]\n")
    execute_process(COMMAND "${CLI}" run "${WORK}/flat-${i}.json" --open "${WORK}/imported.mitcad"
                    RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
    if(NOT status EQUAL 0)
      list(APPEND problems "the sheet ${uid} has no valid flat pattern:\n${out}${err}")
    else()
      message(STATUS "ok   ${uid} is a valid sheet with a valid flat pattern")
    endif()
  endforeach()
endif()
if(sheets EQUAL 0)
  list(APPEND problems "no body is a valid sheet at the end of the timeline")
endif()

if(problems)
  list(JOIN problems "\n  " text)
  message(FATAL_ERROR "f3d sheet unfold reported:\n  ${text}")
endif()
