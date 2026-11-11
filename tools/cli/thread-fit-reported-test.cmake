# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Auto Fix of a thread on a hole too small for it (mitcad#1632) on a
# reported design (never committed): the project subset under
# MITCAD_MITCAD_REPORTED with its `.mitcad` store, the design found by its
# content id (the first 8 hex digits of its SHA-256). Copied into WORK,
# opened by mitcad-cli run with thread_fit_reported.json: Auto Fix on
# Thread2 is refused as the design is (its Chamfer3, on the step between
# the bore and the thread's crest, would fail once the step is gone); with
# Chamfer3 suppressed it sets the bore's radius dimension from 72/2 to
# 73.835/2 (written so in the saved design), the wall dimension stays 3 mm,
# the thread's fit warning goes, and no feature that built fails. Skipped
# when the folder or the design is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P thread-fit-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "thread-fit-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "e847ebd2") # the design with Thread2 on the Ø72 bore

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "thread fit reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${reported}/*.mitcad")
set(source "")
foreach(design IN LISTS designs)
  file(SHA256 "${design}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL design_id)
    set(source "${design}")
    break()
  endif()
endforeach()
if(source STREQUAL "")
  message(STATUS "thread fit reported: skipped (no design ${design_id} in ${reported})")
  return()
endif()

# Its project: the folder above it with the project marker.
get_filename_component(root "${source}" DIRECTORY)
while(NOT EXISTS "${root}/.mitcad/project.json")
  get_filename_component(parent "${root}" DIRECTORY)
  if(parent STREQUAL root)
    message(FATAL_ERROR "${source} is in no MitCad project")
  endif()
  set(root "${parent}")
endwhile()
file(RELATIVE_PATH inside "${root}" "${source}")

# A copy, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(COPY "${root}/" DESTINATION "${WORK}/project")
set(copy "${WORK}/project/${inside}")
set(fixed "${WORK}/fixed.mitcad")

# The features that fail before (none is expected to fail after that did
# not before).
function(failing report result)
  string(REGEX MATCHALL "\n  F[0-9]+ [^\n]*\\([a-z_]+\\): error[^\n]*" rows "${report}")
  set(names)
  foreach(row IN LISTS rows)
    string(REGEX REPLACE "^\n  (F[0-9]+) .*$" "\\1" uid "${row}")
    list(APPEND names "${uid}")
  endforeach()
  set(${result} "${names}" PARENT_SCOPE)
endfunction()

execute_process(COMMAND "${CLI}" info "${copy}" RESULT_VARIABLE status OUTPUT_VARIABLE before ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "opening the design failed (${status}):\n${before}${err}")
endif()
failing("${before}" failed_before)

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/thread_fit_reported.json" --open "${copy}" --save "${fixed}"
                RESULT_VARIABLE status OUTPUT_VARIABLE after ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "thread_fit_reported failed (${status}):\n${after}${err}")
endif()
failing("${after}" failed_after)
foreach(uid IN LISTS failed_after)
  if(NOT uid IN_LIST failed_before)
    message(FATAL_ERROR "${uid} fails after the fit:\n${after}")
  endif()
endforeach()
if(after MATCHES "Thread2 \\(thread\\): [^\n]*smaller than the thread's minor diameter")
  message(FATAL_ERROR "Thread2 still warns of the hole's diameter:\n${after}")
endif()

# The radius keeps its form.
file(READ "${fixed}" saved)
if(NOT saved MATCHES "\"name\": *\"d37\",[^}]*\"expression\": *\"73\\.835/2\"")
  string(REGEX MATCH "\"name\": *\"d37\",[^}]*" d37 "${saved}")
  message(FATAL_ERROR "d37 is not 73.835/2 in the saved design: ${d37}")
endif()
message(STATUS "ok   Thread2's bore fitted: d37 = 73.835/2, the wall 3 mm, nothing newly failing")
