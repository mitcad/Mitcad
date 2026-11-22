# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Shells on reporters' designs (never committed): GitHub #466 (mitcad#2050),
# a plate with a cylinder on one rounded corner, its foot rounded, shelled
# 2 mm inside with the cylinder's top and the plate's bottom removed. Each
# report's archive is found in its folder under MITCAD_MITCAD_REPORTED by
# its content id (the first 8 hex digits of the SHA-256), the design in it
# by its own, unpacked into WORK so that nothing is written beside the
# reported files, and opened by mitcad-cli run with its script
# (shell_reported*.json): the Shell builds into one solid with walls of the
# thickness given. Skipped when the folder or every archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P shell-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "shell-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

# Each case: the report's folder, the reporter's archive, the design in it
# (content ids) and the script that opens it.
set(cases
  "github-466:53c6486f:b6232239:shell_reported466.json" # 2 mm inside, two faces open
  "github-466:53c6486f:b6232239:shell_reported466_plain.json") # the same without its roundings

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "shell reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
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
    message(STATUS "shell reported: no archive ${archive_id} in ${reported}/${folder}")
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
  message(STATUS "shell reported: skipped (no reported archive found)")
endif()
