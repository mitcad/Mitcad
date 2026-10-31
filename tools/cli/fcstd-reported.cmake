# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# FreeCAD documents reported with an import fault (never committed; kept
# with the corpora): each found by its content id (the first 8 hex digits
# of its SHA-256) under the folders of MITCAD_FCSTD_CORPUS and the
# `reported` folder next to each of them, imported with mitcad-cli
# import-fcstd, and the outcome of the objects its fault was about checked
# in the import report (a sketch or a feature: `parametric`, `partial`, …).
# Skipped when none of the files is there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P fcstd-reported.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fcstd-reported.cmake needs -D${var}=...")
  endif()
endforeach()

# "<content id> <object> <outcome>" each.
set(expected
  # mitcad#998: a sketch whose external geometry is the intersection of
  # another sketch's curves with its plane, one whose external geometry
  # is the intersection of a binder's edges with it, and one projecting a
  # face of an earlier feature that later features of its body split.
  "7a8cd05b Sketch001 parametric"
  "7a8cd05b Sketch002 parametric"
  "7a8cd05b Sketch018 parametric"
)

set(ids)
foreach(line IN LISTS expected)
  string(REGEX MATCH "^[0-9a-f]+" id "${line}")
  list(APPEND ids "${id}")
endforeach()
list(REMOVE_DUPLICATES ids)

set(corpus "$ENV{MITCAD_FCSTD_CORPUS}")
if(NOT CMAKE_HOST_WIN32)
  string(REPLACE ":" ";" corpus "${corpus}")
endif()
set(files)
foreach(dir IN LISTS corpus)
  foreach(root IN ITEMS "${dir}" "${dir}/../reported")
    if(IS_DIRECTORY "${root}")
      file(GLOB_RECURSE found "${root}/*.FCStd" "${root}/*.fcstd")
      list(APPEND files ${found})
    endif()
  endforeach()
endforeach()

# The files of the expected ids, once each.
set(chosen)
set(seen)
foreach(file IN LISTS files)
  file(SHA256 "${file}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  string(TOLOWER "${id}" id)
  if(id IN_LIST ids AND NOT id IN_LIST seen)
    list(APPEND seen "${id}")
    list(APPEND chosen "${id}=${file}")
  endif()
endforeach()
if(NOT chosen)
  message("reported FreeCAD files: skipped (none of ${ids} under MITCAD_FCSTD_CORPUS or its reported folders)")
  return()
endif()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(failed 0)
foreach(entry IN LISTS chosen)
  string(REGEX REPLACE "^([0-9a-f]+)=.*$" "\\1" id "${entry}")
  string(REGEX REPLACE "^[0-9a-f]+=" "" file "${entry}")
  set(report "${WORK}/${id}.json")
  execute_process(COMMAND "${CLI}" import-fcstd "${file}" --report "${report}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0 OR NOT EXISTS "${report}")
    message("FAILED ${id}: the import failed (${status}): ${err}")
    math(EXPR failed "${failed} + 1")
    continue()
  endif()
  file(READ "${report}" json)
  foreach(line IN LISTS expected)
    if(NOT line MATCHES "^${id} ([^ ]+) ([^ ]+)$")
      continue()
    endif()
    set(object "${CMAKE_MATCH_1}")
    set(want "${CMAKE_MATCH_2}")
    set(outcome "")
    set(notes "")
    foreach(group IN ITEMS sketches features)
      string(JSON count ERROR_VARIABLE none LENGTH "${json}" "${group}")
      if(none OR count EQUAL 0)
        continue()
      endif()
      math(EXPR last "${count} - 1")
      foreach(i RANGE ${last})
        string(JSON name GET "${json}" "${group}" ${i} object)
        if(name STREQUAL object)
          string(JSON outcome GET "${json}" "${group}" ${i} outcome)
          string(JSON notes ERROR_VARIABLE no_notes GET "${json}" "${group}" ${i} notes)
          string(JSON dropped ERROR_VARIABLE no_dropped GET "${json}" "${group}" ${i} dropped)
          if(NOT no_dropped)
            string(APPEND notes " ${dropped}")
          endif()
          break()
        endif()
      endforeach()
      if(outcome)
        break()
      endif()
    endforeach()
    if(outcome STREQUAL want)
      message("ok     ${id} ${object}: ${outcome}")
    else()
      message("FAILED ${id} ${object}: ${outcome}, not ${want}: ${notes}")
      math(EXPR failed "${failed} + 1")
    endif()
  endforeach()
endforeach()
if(failed GREATER 0)
  message(FATAL_ERROR "reported FreeCAD files: ${failed} failed")
endif()
