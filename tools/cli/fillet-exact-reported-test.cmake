# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A rounding as large as the side faces beside its edges (GitHub #353,
# mitcad#1819) on the reporter's designs (never committed): their archives
# in `github-353/` and `github-375/` under MITCAD_MITCAD_REPORTED, found by their
# content ids (the first 8 hex digits of the SHA-256), each design in its
# archive by its own, unpacked into WORK. Each is opened by mitcad-cli run
# with its script (fillet_exact_reported*.json):
# the design's rounding is built at its size, with no warning or
# information. Skipped when the folder or every archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-exact-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-exact-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

# Each case: the report's folder, the reporter's archive, the design in it
# (content ids) and the script that opens it.
set(cases
  "github-353:2cd84339:4bde7722:fillet_exact_reported.json" # four edges
  "github-353:b7522a40:39960088:fillet_exact_reported2.json" # eight edges
  "github-353:39896406:d2414f5a:fillet_exact_reported3.json" # a cube, 4.999
  "github-353:39896406:d2414f5a:fillet_exact_reported3_half.json" # the cube at 5
  "github-375:92354767:38e97e9a:fillet_exact_reported375.json") # then a chamfer

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "fillet exact reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(ran 0)
foreach(case IN LISTS cases)
  string(REPLACE ":" ";" fields "${case}")
  list(GET fields 0 folder)
  list(GET fields 1 archive_id)
  list(GET fields 2 design_id)
  list(GET fields 3 script)
  file(GLOB archives LIST_DIRECTORIES false "${reported}/${folder}/*.zip")
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
    message(STATUS "fillet exact reported: no archive ${archive_id} in ${reported}/${folder}")
    continue()
  endif()
  if(NOT IS_DIRECTORY "${WORK}/${archive_id}")
    file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/${archive_id}")
  endif()
  file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/${archive_id}/*.mitcad")
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
  execute_process(COMMAND "${CLI}" run "${SCRIPTS}/${script}" --open "${design}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "${script} failed (${status}):\n${out}${err}")
  endif()
  message(STATUS "ok   ${script}")
  math(EXPR ran "${ran} + 1")
endforeach()
if(ran EQUAL 0)
  message(STATUS "fillet exact reported: skipped (no reported archive found)")
endif()
