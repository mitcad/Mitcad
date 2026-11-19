# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Pieces a fillet leaves between two of its own blend faces (mitcad#1858),
# on the reported nut design whose two roundings of 5 (F91 and F93 of its
# timeline) left 3 and 6 such pieces and ran the whole body's defeaturing
# for 155.6 s and 84.0 s before they were kept with a warning (port patch
# 0871 and the remnant patch now close them; the design itself is never
# committed): its design found under MITCAD_MITCAD_REPORTED by its content
# id (the first 8 hex digits of its SHA-256), its project (the folder with
# .mitcad/project.json above it) copied into WORK so that nothing is
# written beside the reported files. Recomputed by mitcad-cli info
# --timings: both roundings are ok (no warning, no error) and, where the
# recompute lists their times, each took under 10 s. Skipped when the
# design is not there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P dressup-blend-remnants-reported-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "dressup-blend-remnants-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "40a4a0e4") # the nut design (the state of 13.11.2026)
set(design_size 3845674)

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "dressup blend remnants reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${reported}/*.mitcad")
set(design "")
foreach(candidate IN LISTS designs)
  file(SIZE "${candidate}" size)
  if(NOT size EQUAL design_size)
    continue()
  endif()
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL design_id)
    set(design "${candidate}")
    break()
  endif()
endforeach()
if(design STREQUAL "")
  message(STATUS "dressup blend remnants reported: skipped (no design ${design_id} under ${reported})")
  return()
endif()

# The project: the nearest folder above the design with .mitcad/project.json.
get_filename_component(root "${design}" DIRECTORY)
while(NOT EXISTS "${root}/.mitcad/project.json")
  get_filename_component(parent "${root}" DIRECTORY)
  if(parent STREQUAL root)
    message(FATAL_ERROR "the design ${design_id} is in no project")
  endif()
  set(root "${parent}")
endwhile()
file(RELATIVE_PATH inside "${root}" "${design}")
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}/project")
file(COPY "${root}/" DESTINATION "${WORK}/project")

execute_process(COMMAND "${CLI}" info --timings "${WORK}/project/${inside}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "mitcad-cli info failed (${status}):\n${out}${err}")
endif()
foreach(uid F91 F93)
  # The timeline's line comes first: "  F91 <name> (fillet): <status>".
  if(NOT out MATCHES "\n  ${uid} [^\n]*\\(fillet\\): ([^\n]*)")
    message(FATAL_ERROR "no rounding ${uid} in the report:\n${out}")
  endif()
  set(state "${CMAKE_MATCH_1}")
  if(NOT state STREQUAL "ok")
    message(FATAL_ERROR "the rounding ${uid} is not ok: ${state}")
  endif()
  # Its time where the recompute lists it among the slowest.
  if(out MATCHES "\n  ${uid} [^\n]*\\(fillet\\): ([0-9]+)(\\.[0-9]+)? ms")
    if(CMAKE_MATCH_1 GREATER 10000)
      message(FATAL_ERROR "the rounding ${uid} took ${CMAKE_MATCH_1} ms")
    endif()
    message(STATUS "${uid}: ${CMAKE_MATCH_1} ms")
  endif()
endforeach()
message(STATUS "ok   both roundings rebuild with no piece left and no warning, each under 10 s")
