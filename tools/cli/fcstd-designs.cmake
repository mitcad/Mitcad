# SPDX-License-Identifier: MIT
# Single FreeCAD documents checked one by one (mitcad#997): each found by
# its content id (the first 8 hex digits of its SHA-256) under the folders
# of MITCAD_FCSTD_CORPUS (a path list, like PATH) and the folders beside
# them (their parent folders, where reported documents are kept apart from
# the reference models), imported with mitcad-cli import-fcstd. Per
# document: no fallback, and the volume of the last feature checked (the
# Body's tip: all the Body's solids together) within the bounds given,
# FreeCAD's stored volume within 1e-6 of it relative.
#
# Skipped when none of the documents is found.
#
#   cmake -DCLI=<mitcad-cli> -DWORK=<dir> -P fcstd-designs.cmake

cmake_minimum_required(VERSION 3.24)

# <id>|<least volume, mm3>|<greatest volume, mm3>
set(designs
    # A sleeve padded as two halves 4 mm apart (two solids), a threaded
    # hole drilled into one half, a hole through both from a binder's
    # sketch, a cog padded just clear of the other half (a third solid)
    # and patterned round (26 cogs), and a chamfer on both halves: FreeCAD's
    # Body is 28 solids. The hole and the chamfer replay on the several
    # bodies, the pad keeps the cog apart (mitcad#997). FreeCAD's volume
    # 9378129.820 mm3.
    "f74d1d69|9378120.441|9378139.198"
)

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fcstd-designs.cmake needs -D${var}=...")
  endif()
endforeach()

# The corpus folders and their parents.
set(corpus "$ENV{MITCAD_FCSTD_CORPUS}")
if(NOT CMAKE_HOST_WIN32)
  string(REPLACE ":" ";" corpus "${corpus}")
endif()
set(folders "")
foreach(dir IN LISTS corpus)
  if(IS_DIRECTORY "${dir}")
    get_filename_component(parent "${dir}" DIRECTORY)
    list(APPEND folders "${parent}")
  endif()
endforeach()
list(REMOVE_DUPLICATES folders)
if(NOT folders)
  message("FreeCAD designs: skipped (MITCAD_FCSTD_CORPUS names no folder)")
  return()
endif()

set(wanted "")
foreach(design IN LISTS designs)
  string(REPLACE "|" ";" fields "${design}")
  list(GET fields 0 id)
  list(APPEND wanted "${id}")
endforeach()
set(found_ids "")
foreach(folder IN LISTS folders)
  file(GLOB_RECURSE files "${folder}/*.FCStd" "${folder}/*.fcstd")
  list(SORT files)
  foreach(f IN LISTS files)
    # (A name with a semicolon falls apart in the list: not looked at.)
    if(NOT EXISTS "${f}" OR IS_DIRECTORY "${f}")
      continue()
    endif()
    file(SHA256 "${f}" sum)
    string(SUBSTRING "${sum}" 0 8 id)
    list(FIND wanted "${id}" at)
    list(FIND found_ids "${id}" seen)
    if(at GREATER -1 AND seen EQUAL -1)
      list(APPEND found_ids "${id}")
      set(path_${id} "${f}")
    endif()
  endforeach()
endforeach()
if(NOT found_ids)
  message("FreeCAD designs: skipped (none of ${wanted} under ${folders})")
  return()
endif()

file(MAKE_DIRECTORY "${WORK}")
set(failed "")
foreach(design IN LISTS designs)
  string(REPLACE "|" ";" fields "${design}")
  list(GET fields 0 id)
  list(GET fields 1 least)
  list(GET fields 2 greatest)
  list(FIND found_ids "${id}" at)
  if(at EQUAL -1)
    message("not found ${id}")
    continue()
  endif()
  set(report "${WORK}/${id}.json")
  execute_process(COMMAND "${CLI}" import-fcstd "${path_${id}}" --report "${report}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  string(REGEX MATCH "[0-9]+ objects: [0-9]+ bodies[^\n]*" summary "${out}")
  set(problems "")
  if(NOT status EQUAL 0)
    string(APPEND problems "\n    the import failed (${status}): ${err}")
  elseif(NOT summary MATCHES ", 0 fallback,")
    string(REGEX MATCHALL "\n  feature [^\n]*: fallback[^\n]*\n    note: [^\n]*" fallbacks "${out}")
    string(REPLACE ";" "" fallbacks "${fallbacks}")
    string(REPLACE "\n  feature" "\n    feature" fallbacks "${fallbacks}")
    string(APPEND problems "\n    expected no fallback:${fallbacks}")
  endif()
  # The last feature checked: its volume, all the Body's solids.
  set(volume "")
  if(EXISTS "${report}")
    file(READ "${report}" json)
    string(JSON count ERROR_VARIABLE none LENGTH "${json}" features)
    if(NOT none AND count GREATER 0)
      math(EXPR last "${count} - 1")
      foreach(i RANGE ${last} 0 -1)
        string(JSON volume ERROR_VARIABLE none GET "${json}" features ${i} check volume)
        if(NOT none)
          string(JSON object GET "${json}" features ${i} object)
          break()
        endif()
        set(volume "")
      endforeach()
    endif()
  endif()
  if(volume STREQUAL "")
    string(APPEND problems "\n    no feature was checked")
  elseif(volume LESS least OR volume GREATER greatest)
    string(APPEND problems "\n    ${object}: volume ${volume} mm3, not within ${least} to ${greatest}")
  endif()
  if(problems)
    message("FAILED ${id}: ${summary}${problems}")
    list(APPEND failed "${id}")
  else()
    message("ok     ${id}: ${summary}; ${object}: volume ${volume} mm3")
  endif()
endforeach()
if(failed)
  message(FATAL_ERROR "failed: ${failed}")
endif()
