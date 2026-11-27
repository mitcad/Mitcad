# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Replace Face between a shelled body's inner ceiling and its rim (mitcad#2541,
# GitHub #717) on a reported design (never committed): its archive in
# `github-717/` under MITCAD_MITCAD_REPORTED (the report's attachment,
# stored as `github-717/default.zip`), found by its content id (the first 8
# hex digits of its SHA-256), the design in it by its own, unpacked into
# WORK. The design's file name is not ASCII; it is found by its content id,
# so only its `.mitcad` ending matters. Opened twice by mitcad-cli run: with
# replace_face_shell_rim_reported.json (the ceiling onto the rim) and with
# replace_face_shell_rim_lifted_reported.json (the rim onto the ceiling).
# Skipped when the folder or the archive is not there. On a failure the
# design's face and edge names are written to WORK/names.txt.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P replace-face-shell-rim-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "replace-face-shell-rim-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "33ac33a4") # the reporter's archive
set(design_id "e7d9fc58") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "replace face shell rim reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-717/*.zip")
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
  message(STATUS "replace face shell rim reported: skipped (no archive ${archive_id} in ${reported}/github-717)")
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

foreach(script replace_face_shell_rim_reported replace_face_shell_rim_lifted_reported)
  execute_process(COMMAND "${CLI}" run "${SCRIPTS}/${script}.json" --open "${design}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    # The names of the design's faces and edges beside the failure, for a
    # pick the design does not have (`mitcad-cli info --names`, in
    # WORK/names.txt).
    execute_process(COMMAND "${CLI}" info "${design}" --names
                    OUTPUT_FILE "${WORK}/names.txt" ERROR_VARIABLE names_err)
    message(FATAL_ERROR "${script} failed (${status}):\n${out}${err}\n"
                        "the design's face and edge names: ${WORK}/names.txt ${names_err}")
  endif()
endforeach()
message(STATUS "ok   the reported shell's inner ceiling onto its rim and the rim onto the ceiling, each joined to the other's face")
