# SPDX-License-Identifier: MIT
# Designs saved by earlier versions, kept outside the repository (never
# committed): every .mitcad under the folders of MITCAD_DESIGN_CORPUS (a
# path list, like PATH; each design in its project folder, with the
# project's .mitcad/brep store) is opened and recomputed with mitcad-cli
# info, and no feature of it may fail (mitcad#1613; sketches only before):
# a linked projection of a source that changed shape since must keep its
# geometry rather than tear its fixed curves apart (mitcad#249), and a
# feature an earlier version built must still build. A feature that fails
# for a reason of its own (the design was saved with it failing) is listed
# in a known.txt (in the design's project folder or any folder of the
# corpus), a line per feature with three tab-separated columns: the
# design's path relative to the known.txt's folder (forward slashes), the
# feature's uid, and the reason; lines starting with # are comments. A
# listed feature that builds again is reported, so that the list is kept
# short. Prints a line per file; skipped without the corpus.
#
# cmake -DCLI=<mitcad-cli> -P designs-corpus.cmake

cmake_minimum_required(VERSION 3.24)

if(NOT DEFINED CLI)
  message(FATAL_ERROR "designs-corpus.cmake needs -DCLI=...")
endif()

set(corpus "$ENV{MITCAD_DESIGN_CORPUS}")
if(NOT CMAKE_HOST_WIN32)
  string(REPLACE ":" ";" corpus "${corpus}")
endif()
set(files)
# The known failures, as "<design's full path>|<uid>" entries.
set(known)
foreach(dir IN LISTS corpus)
  if(IS_DIRECTORY "${dir}")
    file(GLOB_RECURSE found "${dir}/*.mitcad")
    # A project's own folder (.mitcad/) holds no designs.
    list(FILTER found EXCLUDE REGEX "/\\.mitcad/")
    list(APPEND files ${found})
    file(GLOB_RECURSE lists "${dir}/known.txt")
    list(FILTER lists EXCLUDE REGEX "/\\.mitcad/")
    foreach(list_file IN LISTS lists)
      get_filename_component(folder "${list_file}" DIRECTORY)
      # Its lines (a reason's semicolons, which would split a CMake list,
      # read as commas; a checkout's Windows line ends dropped).
      file(READ "${list_file}" text)
      string(REPLACE ";" "," text "${text}")
      string(REPLACE "\r" "" text "${text}")
      string(REPLACE "\n" ";" lines "${text}")
      foreach(line IN LISTS lines)
        if(line MATCHES "^#" OR line STREQUAL "")
          continue()
        endif()
        if(NOT line MATCHES "^([^\t]+)\t([^\t]+)\t(.+)$")
          message(FATAL_ERROR "${list_file}: a line needs three tab-separated columns "
            "(design, feature uid, reason): '${line}'")
        endif()
        get_filename_component(design "${folder}/${CMAKE_MATCH_1}" ABSOLUTE)
        list(APPEND known "${design}|${CMAKE_MATCH_2}")
      endforeach()
    endforeach()
  endif()
endforeach()
list(REMOVE_DUPLICATES files)
list(SORT files)
if(NOT files)
  message("design corpus: skipped (set MITCAD_DESIGN_CORPUS to folders of Mitcad projects)")
  return()
endif()

set(failed)
foreach(file IN LISTS files)
  get_filename_component(dir "${file}" DIRECTORY)
  get_filename_component(design "${file}" ABSOLUTE)
  execute_process(COMMAND "${CLI}" info "${file}"
    WORKING_DIRECTORY "${dir}"
    RESULT_VARIABLE code OUTPUT_VARIABLE out ERROR_VARIABLE err)
  # The timeline's lines: "  F12 Chamfer4 (chamfer): error: ..." (an
  # error's semicolons read as commas, so as not to split the list).
  string(REPLACE ";" "," out "${out}")
  string(REGEX MATCHALL "\n  [A-Za-z0-9_]+ [^\n]* \\([a-z_0-9]+\\): error[^\n]*" broken "${out}")
  set(unexpected)
  set(expected 0)
  foreach(line IN LISTS broken)
    string(REGEX REPLACE "^\n  ([A-Za-z0-9_]+) .*" "\\1" uid "${line}")
    string(REGEX REPLACE "^\n  " "" line "${line}")
    if("${design}|${uid}" IN_LIST known)
      math(EXPR expected "${expected} + 1")
    else()
      list(APPEND unexpected "${line}")
    endif()
  endforeach()
  # Listed features that build now.
  set(fixed)
  foreach(entry IN LISTS known)
    if(entry MATCHES "^(.*)\\|([^|]+)$" AND CMAKE_MATCH_1 STREQUAL design)
      set(uid "${CMAKE_MATCH_2}")
      if(NOT out MATCHES "\n  ${uid} [^\n]* \\([a-z_0-9]+\\): error")
        list(APPEND fixed "${uid}")
      endif()
    endif()
  endforeach()
  if(NOT code EQUAL 0)
    message("FAIL ${file}: mitcad-cli info exited with ${code}\n${err}")
    list(APPEND failed "${file}")
  elseif(unexpected)
    string(REPLACE ";" "\n" unexpected "${unexpected}")
    message("FAIL ${file}: features fail\n${unexpected}")
    list(APPEND failed "${file}")
  elseif(expected GREATER 0)
    message("ok   ${file} (${expected} known failure(s))")
  else()
    message("ok   ${file}")
  endif()
  if(fixed)
    string(REPLACE ";" ", " fixed "${fixed}")
    message("     known.txt lists ${fixed} of it, which build now: remove the line(s)")
  endif()
endforeach()
if(failed)
  list(LENGTH failed count)
  message(FATAL_ERROR "design corpus: ${count} file(s) failed")
endif()
