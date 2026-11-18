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
# is the sketch's placement on its plane), every feature (the Fillet on the
# ends too) rebuilds in both imports, which report no warning and no skipped
# object (mitcad#1960), and the body reaches 5 mm further
# along -x (within 0.01 mm: the ribs, which may reach furthest, start a
# degree into the bend). Skipped when the folder or the archive is not
# there.
#
# A third import sets the bend radius 13 mm smaller, to 32 mm (mitcad#2009):
# every feature rebuilds again, and the body stays within its two pads'
# ends (y and x at most 2 mm past the axes' planes). Before OCCT port patch
# 1800 the corner where the roundings of a pad's end meet was trimmed by a
# curve three quarters of a turn the wrong way round there: a valid body
# that reached 3.8 mm beyond the pad.
#
# Four more imports set it to 30, 25, 24 and 20 mm (mitcad#2018): every
# feature rebuilds, the body stays within its pads, and its volume lies on
# the line through the volumes at 32 and 45 mm within 0.1 mm3 (the swept
# tube and ribs grow in proportion to the radius; the roundings' corners
# are approximated within hundredths of a cubic millimetre). Below about
# 31 mm the corner where the rounding of the rib 1 degree from each end
# meets the flat's rim is set back past the rim's end: before OCCT port
# patch 1801 the Fillet gave an invalid shape there or a body left open,
# and from 23.5 to 24.5 mm the rounding of that rib's outer edge failed
# before patch 1802.
#
# At 24.5 and 25 mm the body's box also reaches no further along y than
# the guide's outside at the end on the y axis: the bend radius plus 3.7
# (the profile's reach out from the bend's axis; the body's largest y
# and smallest x are both radius + 3.7 at 30 and 45 mm, a fine
# triangulation's too), and 0.01 mm: the box is the trimmed faces'
# (mitcad#2020). Before, the corner plates' surfaces beyond their faces
# took it to y = 31.57 at 24.5 mm.
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

# A number of millimetres in nanometres, rounded towards zero (CMake
# compares integers only); a number with a negative exponent is below them.
function(nanometres text out what)
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
  set(${out} "${nm}" PARENT_SCOPE)
endfunction()

# Whether |a - b - expected| is within `tolerance` nanometres.
function(near a b expected tolerance what)
  foreach(v a b expected)
    nanometres("${${v}}" ${v}_nm "${what}")
  endforeach()
  math(EXPR diff "${a_nm} - ${b_nm} - ${expected_nm}")
  if(diff LESS -${tolerance} OR diff GREATER ${tolerance})
    message(FATAL_ERROR "${what}: ${a} - ${b} is not ${expected}")
  endif()
endfunction()

# Whether every body lies within the two pads' ends: its smallest y and
# its largest x at most 2 mm (and 0.01 mm) past the axes' planes.
function(within_pads json what)
  string(JSON count LENGTH "${json}" document bodies)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON y GET "${json}" document bodies ${i} bbox min 1)
    string(JSON x GET "${json}" document bodies ${i} bbox max 0)
    nanometres("${y}" y_nm "${what}")
    nanometres("${x}" x_nm "${what}")
    if(y_nm LESS -2010000 OR x_nm GREATER 2010000)
      message(FATAL_ERROR "${what}: the body reaches beyond a pad's end (smallest y ${y}, largest x ${x})")
    endif()
  endforeach()
endfunction()

import(before)
import(after --set "BendRadius=50 mm")
import(smaller --set "BendRadius=32 mm")
set(lower_radii 30 25 24 20)
set(states before after smaller)
foreach(radius IN LISTS lower_radii)
  import(r${radius} --set "BendRadius=${radius} mm")
  list(APPEND states r${radius})
endforeach()
# 24.5 mm for the box's largest y (mitcad#2020); not on the volume line's
# list, whose arithmetic is in whole millimetres.
import(r24_5 --set "BendRadius=24.5 mm")
list(APPEND states r24_5)

# The profile follows the radius: 5 mm further along -x.
profile_x("${before}" x_before)
profile_x("${after}" x_after)
near("${x_after}" "${x_before}" "-5" 1000 "the profile sketch's centre")

# Every feature rebuilds, the Fillet on the ends included, before and
# after the change (mitcad#1960), and with the smaller radius
# (mitcad#2009).
foreach(state IN LISTS states)
  rebuilt("${${state}}" ok)
  string(JSON count LENGTH "${${state}}" document timeline features)
  list(LENGTH ok rebuilt_count)
  if(NOT rebuilt_count EQUAL count)
    message(FATAL_ERROR "${state} the change: ${rebuilt_count} of ${count} features rebuild (${ok})")
  endif()
  if(NOT "Fillet" IN_LIST ok)
    message(FATAL_ERROR "${state} the change: the Fillet is not on the timeline (${ok})")
  endif()
endforeach()

# The document opens without a warning and without skipped objects: the
# VarSet's values are parameters, the coordinate system is carried by the
# sketch on it (mitcad#1960).
foreach(state IN LISTS states)
  string(JSON warnings LENGTH "${${state}}" import warnings)
  if(NOT warnings EQUAL 0)
    string(JSON first GET "${${state}}" import warnings 0)
    message(FATAL_ERROR "${state} the change: ${warnings} import warnings, the first: ${first}")
  endif()
  string(JSON count LENGTH "${${state}}" import items)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON outcome GET "${${state}}" import items ${i} outcome)
    if(outcome STREQUAL "skipped")
      string(JSON name GET "${${state}}" import items ${i} name)
      string(JSON note ERROR_VARIABLE none GET "${${state}}" import items ${i} note)
      message(FATAL_ERROR "${state} the change: ${name} is skipped (${note})")
    endif()
  endforeach()
endforeach()

# The bend reaches 5 mm further.
reach("${before}" min_before)
reach("${after}" min_after)
near("${min_after}" "${min_before}" "-5" 10000 "the body's smallest x")

# The body stays within the pads' ends at every radius (mitcad#2009).
foreach(state IN LISTS states)
  within_pads("${${state}}" "${state} the change")
endforeach()

# Whether every body's largest y is at most the guide's outside at the end
# on the y axis: the bend radius `radius` (mm) plus the profile's reach
# 3.7 from the bend's axis, and 0.01 mm (mitcad#2020: the box over the
# trimmed faces, not the corner plates' surfaces beyond them).
function(within_outer_rim json radius what)
  nanometres("${radius}" radius_nm "${what}")
  math(EXPR limit "${radius_nm} + 3710000")
  string(JSON count LENGTH "${json}" document bodies)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON y GET "${json}" document bodies ${i} bbox max 1)
    nanometres("${y}" y_nm "${what}")
    if(y_nm GREATER limit)
      message(FATAL_ERROR "${what}: the body's box reaches y ${y}, past the guide's outside at ${radius} + 3.7")
    endif()
  endforeach()
endfunction()
within_outer_rim("${r24_5}" 24.5 "the bend radius 24.5 mm")
within_outer_rim("${r25}" 25 "the bend radius 25 mm")

# The volume of the one body, in millionths of a cubic millimetre.
function(volume_of json out what)
  string(JSON count LENGTH "${json}" document bodies)
  if(NOT count EQUAL 1)
    message(FATAL_ERROR "${what}: ${count} bodies, not one")
  endif()
  string(JSON v GET "${json}" document bodies 0 volume)
  nanometres("${v}" v_n "${what}")
  set(${out} "${v_n}" PARENT_SCOPE)
endfunction()

# The volume at each lower radius lies on the line through the volumes at
# 32 and 45 mm (the document's own radius), within 0.1 mm3 (mitcad#2018).
volume_of("${smaller}" v32 "the bend radius 32 mm")
volume_of("${before}" v45 "the bend radius 45 mm")
foreach(radius IN LISTS lower_radii)
  volume_of("${r${radius}}" v "the bend radius ${radius} mm")
  math(EXPR expected "${v32} + (${v45} - ${v32}) * (${radius} - 32) / 13")
  math(EXPR diff "${v} - ${expected}")
  if(diff LESS -100000 OR diff GREATER 100000)
    message(FATAL_ERROR "the bend radius ${radius} mm: the volume is ${diff} millionths of a mm3 off the line through 32 and 45 mm")
  endif()
endforeach()

message(STATUS "ok   the bend follows its radius: the profile and the body move 5 mm, every feature rebuilds at 20, 24, 24.5, 25, 30, 32, 45 and 50 mm, nothing skipped, the body within its pads and its volume on the line, its box within the guide at 24.5 and 25 mm")
