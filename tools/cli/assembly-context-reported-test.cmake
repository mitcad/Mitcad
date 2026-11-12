# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Automatic assembly cuts and joints (mitcad#1667) on the jointed mechanism
# of GitHub #253 (never committed): its archive in `github-253/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits
# of its SHA-256), the design in it by its own, unpacked into WORK. Opened
# by mitcad-cli run with assembly_context_reported.json: the links' and
# rods' cuts leave alone the links that move against them through joints,
# so Joint12 and Joint13 resolve on the hole edges they were picked on and
# the two link bodies keep their volume and face count when the primary
# lever is dragged one way and back. Skipped when the folder or the archive
# is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P assembly-context-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "assembly-context-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "df89881b") # the reporter's archive: the design and its parts
set(design_id "17249a6c") # the mechanism's design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "assembly context reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-253/*.zip")
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
  message(STATUS "assembly context reported: skipped (no archive ${archive_id} in ${reported}/github-253)")
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

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/assembly_context_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "assembly_context_reported failed (${status}):\n${out}${err}")
endif()
if(out MATCHES "\n  F[0-9]+ [^\n]*\\(joint\\): error[^\n]*")
  message(FATAL_ERROR "a joint fails:\n${out}")
endif()
message(STATUS "ok   Joint12 and Joint13 resolve, the link bodies keep their volume and faces through two drags")
