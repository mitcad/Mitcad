# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# The bent guide of GitHub #372 (mitcad#1930; never committed): an archive
# in `github-372/` under MITCAD_MITCAD_REPORTED, found by its content id
# (the first 8 hex digits of its SHA-256), unpacked into WORK. Its profile
# sketch lies on a plane of a local coordinate system whose attachment
# offset along x is minus the bend radius (a VarSet's BendRadius), and the
# ribs' sketch takes the same radius as a dimension.
#
# The document is imported as it is, then again with the bend radius set
# 5 mm larger (mitcad-cli import-fcstd --set): the profile sketch's centre
# moves 5 mm further along -x with the ribs (the coordinate system's offset
# is the sketch's placement on its plane), no feature that rebuilt in the
# first import fails after the change, and the body reaches 5 mm further
# along -x (within 0.01 mm: the ribs, which may reach furthest, start a
# degree into the bend). Skipped when the folder or the archive is not
# there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P fcstd-bend-reported-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fcstd-bend-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-372")
  message(STATUS "fcstd bend reported: skipped (MITCAD_MITCAD_REPORTED/github-372 is not there)")
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

find_by_id("${reported}/github-372" "*.zip" "ecf7139b" archive)
if(archive STREQUAL "")
  message(STATUS "fcstd bend reported: skipped (no archive ecf7139b in ${reported}/github-372)")
  return()
endif()
# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}")
file(GLOB_RECURSE documents LIST_DIRECTORIES false "${WORK}/*.FCStd" "${WORK}/*.fcstd")
list(LENGTH documents count)
if(NOT count EQUAL 1)
  message(FATAL_ERROR "the archive ecf7139b holds ${count} FreeCAD documents, not one")
endif()
list(GET documents 0 document)

# Imports the document (with `ARGN`: --set name=expression) into `out`:
# the import report and the document report as JSON.
function(import out)
  execute_process(COMMAND "${CLI}" import-fcstd "${document}" ${ARGN} --json
                  RESULT_VARIABLE status OUTPUT_VARIABLE json ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "the import ${ARGN} failed (${status}):\n${err}")
  endif()
  set(${out} "${json}" PARENT_SCOPE)
endfunction()

# The centre (x) of the profile sketch, the object `Sketch`.
function(profile_x json out)
  string(JSON count LENGTH "${json}" import sketches)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON name GET "${json}" import sketches ${i} object)
    if(name STREQUAL "Sketch")
      string(JSON x ERROR_VARIABLE none GET "${json}" import sketches ${i} center 0)
      if(NOT none STREQUAL "NOTFOUND")
        message(FATAL_ERROR "the profile sketch has no centre: ${none}")
      endif()
      set(${out} "${x}" PARENT_SCOPE)
      return()
    endif()
  endforeach()
  message(FATAL_ERROR "the profile sketch was not imported")
endfunction()

# The timeline's features without an error, by name.
function(rebuilt json out)
  set(names)
  string(JSON count LENGTH "${json}" document timeline features)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON name GET "${json}" document timeline features ${i} name)
    string(JSON error ERROR_VARIABLE none GET "${json}" document timeline features ${i} error)
    if(NOT none STREQUAL "NOTFOUND" OR error STREQUAL "" OR error STREQUAL "null")
      list(APPEND names "${name}")
    endif()
  endforeach()
  set(${out} "${names}" PARENT_SCOPE)
endfunction()

# The smallest x of the bodies.
function(reach json out)
  set(least "")
  string(JSON count LENGTH "${json}" document bodies)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON x ERROR_VARIABLE none GET "${json}" document bodies ${i} bbox min 0)
    if(none STREQUAL "NOTFOUND" AND (least STREQUAL "" OR x LESS least))
      set(least "${x}")
    endif()
  endforeach()
  set(${out} "${least}" PARENT_SCOPE)
endfunction()

# Whether |a - b - expected| is within `tolerance` nanometres (CMake
# compares integers only).
function(near a b expected tolerance what)
  foreach(v a b expected)
    # Nanometres, rounded towards zero; a number with a negative exponent
    # is below them.
    set(text "${${v}}")
    if(text MATCHES "[eE]-")
      set(text "0")
    elseif(text MATCHES "[eE]")
      message(FATAL_ERROR "${what}: ${text} is out of range")
    endif()
    string(REGEX MATCH "^(-?)([0-9]*)\\.?([0-9]*)" match "${text}")
    set(sign "${CMAKE_MATCH_1}")
    set(whole "${CMAKE_MATCH_2}")
    string(SUBSTRING "${CMAKE_MATCH_3}000000" 0 6 fraction)
    if(whole STREQUAL "")
      set(whole 0)
    endif()
    string(REGEX REPLACE "^0+([0-9])" "\\1" fraction "${fraction}")
    math(EXPR nm "${whole} * 1000000 + ${fraction}")
    if(sign STREQUAL "-")
      math(EXPR nm "-${nm}")
    endif()
    set(${v}_nm "${nm}")
  endforeach()
  math(EXPR diff "${a_nm} - ${b_nm} - ${expected_nm}")
  if(diff LESS -${tolerance} OR diff GREATER ${tolerance})
    message(FATAL_ERROR "${what}: ${a} - ${b} is not ${expected}")
  endif()
endfunction()

import(before)
import(after --set "BendRadius=50 mm")

# The profile follows the radius: 5 mm further along -x.
profile_x("${before}" x_before)
profile_x("${after}" x_after)
near("${x_after}" "${x_before}" "-5" 1000 "the profile sketch's centre")

# What rebuilt before still rebuilds.
rebuilt("${before}" ok_before)
rebuilt("${after}" ok_after)
foreach(name IN LISTS ok_before)
  if(NOT name IN_LIST ok_after)
    message(FATAL_ERROR "${name} fails after the bend radius changed")
  endif()
endforeach()

# The bend reaches 5 mm further.
reach("${before}" min_before)
reach("${after}" min_after)
near("${min_after}" "${min_before}" "-5" 10000 "the body's smallest x")

message(STATUS "ok   the bend follows its radius: the profile and the body move 5 mm, nothing fails")
