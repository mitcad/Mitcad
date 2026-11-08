# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A sheet design reported after an edit of its base sketch's outline
# (mitcad#1444, GitHub #183; never committed): `github-183-*.mitcad` under
# MITCAD_MITCAD_REPORTED, a sheet face with a fillet and two folds on it
# whose sketch had curves of the outline replaced after the face was made.
# Before the fix the face failed with `sheet profile r{…} is unavailable`
# and every feature on it with it. Opened with mitcad-cli info: the face
# finds its changed profile and warns (`region r{…} found as r{…} after a
# change in …`), and no feature fails. Skipped when the folder or the
# design is not there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P sheet-profile-found-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "sheet-profile-found-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "sheet profile found: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB designs LIST_DIRECTORIES false "${reported}/github-183-*.mitcad")
list(LENGTH designs n)
if(n EQUAL 0)
  message(STATUS "sheet profile found: skipped (no github-183 design in ${reported})")
  return()
endif()
list(GET designs 0 source)

# A copy, so that nothing is written beside the reported file.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(design "${WORK}/reported.mitcad")
file(COPY_FILE "${source}" "${design}")

execute_process(COMMAND "${CLI}" info "${design}" RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "opening the design failed (${status}):\n${out}${err}")
endif()
if(out MATCHES "is unavailable")
  message(FATAL_ERROR "a sheet profile is still unavailable:\n${out}")
endif()
if(NOT out MATCHES "\\(sheet_face\\): warning: [^\n]*region r{[^\n]* found as r{[^\n]* after a change in ")
  message(FATAL_ERROR "the sheet face did not warn that it found its changed profile:\n${out}")
endif()
if(out MATCHES "\\): error: ")
  message(FATAL_ERROR "a feature failed:\n${out}")
endif()
message(STATUS "ok   the sheet face found its changed profile; no feature failed")
