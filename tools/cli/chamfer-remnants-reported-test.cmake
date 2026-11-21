# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Pieces of a face a chamfer takes away, left at the corners of a chain
# rounded more tightly than the chamfer reaches (mitcad#1821), on the
# reported nut design (never committed): its Chamfer8 runs round an
# octagonal rim rounded 1 on the nut's end, chamfered 2.55, and left 16
# pieces of the end face (0.0517 and 0.0028 mm2 at each corner) without a
# word. Its design found under MITCAD_MITCAD_REPORTED by its content id
# (the first 8 hex digits of its SHA-256), its project (the folder with
# .mitcad/project.json above it) copied into WORK so that nothing is
# written beside the reported files, opened by mitcad-cli run with
# chamfer_remnants_reported.json: the bevels meet at a mitre at each
# corner, Chamfer8 is ok without a warning and leaves no piece of the end
# face. Skipped when the design is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P chamfer-remnants-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "chamfer-remnants-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "87bfc0ce") # the nut design (the state of 18.11.2026)
set(design_size 1175113)

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "chamfer remnants reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
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
  message(STATUS "chamfer remnants reported: skipped (no design ${design_id} under ${reported})")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/chamfer_remnants_reported.json" --open "${WORK}/project/${inside}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "chamfer_remnants_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   Chamfer8 mitres its bevels at the corners, no piece of the end face left")
