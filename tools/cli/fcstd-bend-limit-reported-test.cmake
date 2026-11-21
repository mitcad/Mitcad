# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# The bent guide of GitHub #372 and #456 near its smallest bend radius
# (mitcad#2038; never committed): the archive in `github-372/` under
# MITCAD_MITCAD_REPORTED, found by its content id, unpacked into WORK.
#
# The design's limit: each of the three ribs spans 4 degrees about the
# bend's axis, and the Fillet rounds the edges of a rib's inner sloped side
# (a cone from 1.905 to 1.27 mm inside the bend radius R) with 0.5 at both
# the rib's start and its end. The side is (R - 1.905) 2 sin(2 degrees) wide
# at its foot: below R = 16.23 mm the two roundings' contacts (0.5 each, the
# faces at right angles) reach across it, and below about 16.7 mm their
# ends on the face under its foot cross each other (the kernel reports the
# roundings intersecting at 16.6 mm and builds at 16.8 mm). The profile
# reaches 3.7 mm across the axis, the ribs 3.53 mm: far below that.
#
# Above the limit, at 16.8 mm, through the import (--set) and through the
# application's set_parameter on a saved import (mitcad-cli run --open),
# with a unit and as a plain number: every feature rebuilds without an
# error or a warning and the body stays within its pads (the import at 45 mm
# without an error) (as
# fcstd-bend-reported-test.cmake checks it). Below the limit, at 15 mm
# (both ways): the Fillet is refused, its message saying in words why (the
# roundings reach into the face between them, which is narrower) with the
# edges and the face as places, no names or coordinates in the message.
# Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P fcstd-bend-limit-reported-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fcstd-bend-limit-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-372")
  message(STATUS "fcstd bend limit reported: skipped (MITCAD_MITCAD_REPORTED/github-372 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-372/*.zip")
set(archive "")
foreach(candidate IN LISTS archives)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL "ecf7139b")
    set(archive "${candidate}")
    break()
  endif()
endforeach()
if(archive STREQUAL "")
  message(STATUS "fcstd bend limit reported: skipped (no archive ecf7139b in ${reported}/github-372)")
  return()
endif()
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}")
file(GLOB_RECURSE documents LIST_DIRECTORIES false "${WORK}/*.FCStd" "${WORK}/*.fcstd")
list(LENGTH documents count)
if(NOT count EQUAL 1)
  message(FATAL_ERROR "the archive ecf7139b holds ${count} FreeCAD documents, not one")
endif()
list(GET documents 0 document)

# The timeline (JSON) of an import with `ARGN` (--set, --save).
function(import out)
  execute_process(COMMAND "${CLI}" import-fcstd "${document}" ${ARGN} --json
                  RESULT_VARIABLE status OUTPUT_VARIABLE json ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "the import ${ARGN} failed (${status}):\n${err}")
  endif()
  string(JSON timeline GET "${json}" document timeline)
  string(JSON bodies GET "${json}" document bodies)
  set(${out} "{\"timeline\": ${timeline}, \"bodies\": ${bodies}}" PARENT_SCOPE)
endfunction()

# The timeline of the saved import after set_parameter BendRadius = `value`
# (a JSON value: "16.8 mm" with its quotes, or 16.8).
function(set_radius out name value)
  file(WRITE "${WORK}/${name}.json" "[{\"cmd\": \"set_parameter\", \"name\": \"BendRadius\", \"value\": ${value}}]")
  execute_process(COMMAND "${CLI}" run "${WORK}/${name}.json" --open "${WORK}/bend-45.mitcad" --json
                  RESULT_VARIABLE status OUTPUT_VARIABLE json ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "set_parameter BendRadius ${value} failed (${status}):\n${err}")
  endif()
  set(${out} "${json}" PARENT_SCOPE)
endfunction()

# A number of millimetres in nanometres, rounded towards zero.
function(nanometres text out)
  if(text MATCHES "[eE]-")
    set(text "0")
  endif()
  string(REGEX MATCH "^(-?)([0-9]*)\\.?([0-9]*)" match "${text}")
  set(whole "${CMAKE_MATCH_2}")
  string(SUBSTRING "${CMAKE_MATCH_3}000000" 0 6 fraction)
  if(whole STREQUAL "")
    set(whole 0)
  endif()
  string(REGEX REPLACE "^0+([0-9])" "\\1" fraction "${fraction}")
  math(EXPR nm "${whole} * 1000000 + ${fraction}")
  if(CMAKE_MATCH_1 STREQUAL "-")
    math(EXPR nm "-${nm}")
  endif()
  set(${out} "${nm}" PARENT_SCOPE)
endfunction()

# Every feature rebuilt (no error; `strict`: no warning either), the body
# within the pads' ends.
function(rebuilt json what strict)
  string(JSON count LENGTH "${json}" timeline features)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON name GET "${json}" timeline features ${i} name)
    string(JSON status GET "${json}" timeline features ${i} status)
    if(NOT status STREQUAL "ok" AND (strict OR NOT status STREQUAL "warning"))
      string(JSON notices GET "${json}" timeline features ${i} notices)
      message(FATAL_ERROR "${what}: ${name} is ${status}: ${notices}")
    endif()
  endforeach()
  string(JSON bodies LENGTH "${json}" bodies)
  math(EXPR last "${bodies} - 1")
  foreach(i RANGE ${last})
    string(JSON y GET "${json}" bodies ${i} bbox min 1)
    string(JSON x GET "${json}" bodies ${i} bbox max 0)
    nanometres("${y}" y_nm)
    nanometres("${x}" x_nm)
    if(y_nm LESS -2010000 OR x_nm GREATER 2010000)
      message(FATAL_ERROR "${what}: the body reaches beyond a pad's end (smallest y ${y}, largest x ${x})")
    endif()
  endforeach()
endfunction()

# The Fillet refused, its message in words: why, without names or points,
# with its places.
function(refused json what)
  string(JSON count LENGTH "${json}" timeline features)
  math(EXPR last "${count} - 1")
  set(found FALSE)
  foreach(i RANGE ${last})
    string(JSON name GET "${json}" timeline features ${i} name)
    string(JSON status GET "${json}" timeline features ${i} status)
    if(NOT name STREQUAL "Fillet")
      if(NOT status STREQUAL "ok")
        message(FATAL_ERROR "${what}: ${name} is ${status}")
      endif()
      continue()
    endif()
    set(found TRUE)
    if(NOT status STREQUAL "error")
      message(FATAL_ERROR "${what}: the Fillet is ${status}, not refused")
    endif()
    string(JSON message GET "${json}" timeline features ${i} notices 0 message)
    string(JSON places LENGTH "${json}" timeline features ${i} notices 0 places)
    if(NOT message MATCHES "the roundings of two edges reach 0.5 and 0.5 mm into the face between them, which is only 0.9[0-9]* mm wide there: the face would vanish under them")
      message(FATAL_ERROR "${what}: the message does not say why in words: ${message}")
    endif()
    if(message MATCHES "[EVF][0-9]*[{:]|r\\{|\\[-?[0-9]")
      message(FATAL_ERROR "${what}: the message names faces, edges or points: ${message}")
    endif()
    if(places LESS 3)
      message(FATAL_ERROR "${what}: ${places} places, not the two edges and the face")
    endif()
  endforeach()
  if(NOT found)
    message(FATAL_ERROR "${what}: no Fillet on the timeline")
  endif()
endfunction()

import(at45 --save "${WORK}/bend-45.mitcad")
# (At 45 mm the Fillet warns of a corner patch 0.02 mm out of the body.)
rebuilt("${at45}" "the bend radius 45 mm" FALSE)

import(above --set "BendRadius=16.8 mm")
rebuilt("${above}" "the import at 16.8 mm" TRUE)
set_radius(above_unit unit "\"16.8 mm\"")
rebuilt("${above_unit}" "set_parameter 16.8 mm" TRUE)
set_radius(above_plain plain 16.8)
rebuilt("${above_plain}" "set_parameter 16.8" TRUE)

import(below --set "BendRadius=15 mm")
refused("${below}" "the import at 15 mm")
set_radius(below_plain below 15)
refused("${below_plain}" "set_parameter 15")

message(STATUS "ok   the bend at 16.8 mm rebuilds through the import and set_parameter (with a unit and without), within its pads; at 15 mm the Fillet is refused in words")
