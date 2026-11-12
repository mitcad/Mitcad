# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# The jointed mechanism of GitHub #253 (mitcad#1664; never committed): two
# archives in `github-253/` under MITCAD_MITCAD_REPORTED, each found by its
# content id (the first 8 hex digits of its SHA-256), the design in it by
# its own, unpacked into WORK:
#
# - the mechanism as sent first, its placements rotations: it opens with
#   nothing to straighten;
# - the design that no longer opened ("an occurrence transform must be a
#   rotation and a translation"), with its parts linked from a folder
#   beside it: older builds had compounded the drift of six placements
#   over many drags (up to 0.147 off a rotation, mitcad#1666). It opens
#   with a warning for each of the six, straightened to their nearest
#   rotations, every linked part read.
#
# Opening checks every occurrence placement (a rotation and a translation
# within 1e-6, or one that drifted, straightened with a load warning).
# Each design is opened with mitcad-cli info, then saved beside itself with
# mitcad-cli run --save and opened again: the saved design opens without a
# placement to straighten, so every placement it stores is a rotation and
# a translation. Skipped when the folder or an archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P placements-reported-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "placements-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "placements reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()

# The file under `folder` (a glob) whose content id is `id`, or "".
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

# Linked files the archive renamed: its file names have `_` where the links
# have `#`. Copied to the names the links give, beside the design.
function(restore_link_names design)
  get_filename_component(dir "${design}" DIRECTORY)
  file(READ "${design}" text)
  string(JSON count ERROR_VARIABLE none LENGTH "${text}" components)
  if(NOT none STREQUAL "NOTFOUND" OR count EQUAL 0)
    return()
  endif()
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON path ERROR_VARIABLE none GET "${text}" components ${i} link path)
    if(NOT none STREQUAL "NOTFOUND" OR EXISTS "${dir}/${path}")
      continue()
    endif()
    string(REPLACE "#" "_" stored "${path}")
    if(NOT EXISTS "${dir}/${stored}")
      message(FATAL_ERROR "the linked file ${path} is not in the archive")
    endif()
    file(COPY_FILE "${dir}/${stored}" "${dir}/${path}")
  endforeach()
endfunction()

# Opens the design `design_id` of the archive `archive_id`, expecting
# `straightened` placements straightened, then saves it and opens it again.
function(check_design label archive_id design_id straightened)
  find_by_id("${reported}/github-253" "*.zip" "${archive_id}" archive)
  if(archive STREQUAL "")
    message(STATUS "placements reported: skipped (no archive ${archive_id} in ${reported}/github-253)")
    return()
  endif()
  # Unpacked into WORK, so that nothing is written beside the reported files.
  set(work "${WORK}/${label}")
  file(REMOVE_RECURSE "${work}")
  file(MAKE_DIRECTORY "${work}")
  file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${work}")
  find_by_id("${work}" "*.mitcad" "${design_id}" design)
  if(design STREQUAL "")
    message(FATAL_ERROR "the archive ${archive_id} holds no design ${design_id}")
  endif()
  restore_link_names("${design}")

  # Opened as the application opens it.
  execute_process(COMMAND "${CLI}" info "${design}" RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "opening the ${label} design failed (${status}):\n${out}${err}")
  endif()
  if(out MATCHES "keeps the bodies saved with the project")
    message(FATAL_ERROR "a linked part of the ${label} design was not read:\n${out}")
  endif()
  string(REGEX MATCHALL "Warning: the placement of [^\n]+ was distorted and has been straightened" warnings "${out}")
  list(LENGTH warnings count)
  if(NOT count EQUAL straightened)
    message(FATAL_ERROR "the ${label} design straightened ${count} placements, not ${straightened}:\n${out}")
  endif()

  # Saved beside itself (its links stay relative) and opened again:
  # nothing left to straighten.
  set(script "${work}/save.json")
  file(WRITE "${script}" "[{\"comment\": \"The reported design, saved as it opens (mitcad#1664).\"}]\n")
  get_filename_component(design_dir "${design}" DIRECTORY)
  set(saved "${design_dir}/saved-again.mitcad")
  execute_process(COMMAND "${CLI}" run "${script}" --open "${design}" --save "${saved}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE run_out ERROR_VARIABLE run_err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "saving the ${label} design failed (${status}):\n${run_out}${run_err}")
  endif()
  execute_process(COMMAND "${CLI}" info "${saved}" RESULT_VARIABLE status OUTPUT_VARIABLE again
                  ERROR_VARIABLE again_err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "opening the saved ${label} design failed (${status}):\n${again}${again_err}")
  endif()
  if(again MATCHES "Warning: the placement of [^\n]+ was distorted and has been straightened")
    message(FATAL_ERROR "the saved ${label} design still has a placement to straighten:\n${again}")
  endif()
  message(STATUS "ok   the ${label} design opens (${count} placements straightened) and opens again as saved")
endfunction()

check_design(first "df89881b" "17249a6c" 0)
check_design(drifted "d77fdb1c" "0f734c3f" 6)
