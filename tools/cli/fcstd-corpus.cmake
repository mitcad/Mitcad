# SPDX-License-Identifier: MIT
# FreeCAD documents kept outside the repository (never committed): every
# .FCStd under the folders of MITCAD_FCSTD_CORPUS (a path list, like PATH)
# is imported with mitcad-cli import-fcstd; one with a dump next to it
# (<name>.json, tools/freecad-export/dump.py) is compared with it: the
# volumes, areas and world centres of mass of its bodies, parts and links.
# A reference model whose script (tools/freecad-export/models/<name>.py)
# says "# Mitcad expects: no fallback" must replay its history without a
# fallback. A model whose script has "# Mitcad change: <parameter> =
# <expression>" lines and a variant <name>_changed (the same change made
# in FreeCAD) is imported again with those parameters changed
# (import-fcstd --set) and compared with the variant's dump. Prints a line
# per file; skipped without the corpus.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P fcstd-corpus.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fcstd-corpus.cmake needs -D${var}=...")
  endif()
endforeach()

set(corpus "$ENV{MITCAD_FCSTD_CORPUS}")
if(NOT CMAKE_HOST_WIN32)
  string(REPLACE ":" ";" corpus "${corpus}")
endif()
set(files)
foreach(dir IN LISTS corpus)
  if(IS_DIRECTORY "${dir}")
    file(GLOB_RECURSE found "${dir}/*.FCStd" "${dir}/*.fcstd")
    list(APPEND files ${found})
  endif()
endforeach()
list(REMOVE_DUPLICATES files)
list(SORT files)
if(NOT files)
  message("FreeCAD corpus: skipped (set MITCAD_FCSTD_CORPUS to folders of .FCStd files)")
  return()
endif()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(failed)
set(compared 0)
list(LENGTH files count)
foreach(file IN LISTS files)
  get_filename_component(dir "${file}" DIRECTORY)
  get_filename_component(stem "${file}" NAME_WE)
  get_filename_component(parent "${dir}" NAME)
  set(dump "${dir}/${stem}.json")
  set(arguments import-fcstd "${file}" --report "${WORK}/${parent}-${stem}.json")
  if(EXISTS "${dump}")
    list(APPEND arguments --reference "${dump}")
    math(EXPR compared "${compared} + 1")
  endif()
  execute_process(COMMAND "${CLI}" ${arguments} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  string(REGEX MATCH "[0-9]+ objects: [0-9]+ bodies[^\n]*" summary "${out}")
  string(REGEX MATCH "reference: [^\n]*" reference "${out}")
  # The model's expectation of the history.
  set(script "${CMAKE_CURRENT_LIST_DIR}/../freecad-export/models/${stem}.py")
  if(status EQUAL 0 AND EXISTS "${script}")
    file(STRINGS "${script}" expects REGEX "^# Mitcad expects: no fallback")
    if(expects AND NOT summary MATCHES " 0 fallback")
      set(status 1)
      string(REGEX MATCHALL "\n  feature [^\n]*: fallback[^\n]*\n    note: [^\n]*" fallbacks "${out}")
      string(REPLACE ";" "" fallbacks "${fallbacks}")
      string(REPLACE "\n  feature" "\n    feature" fallbacks "${fallbacks}")
      set(out "\n    expected no fallback:${fallbacks}")
    endif()
  endif()
  # The model's parameters changed as its variant has them in FreeCAD
  # ("# Mitcad change: <parameter> = <expression>"): the result against
  # the variant's dump.
  if(status EQUAL 0 AND EXISTS "${script}" AND EXISTS "${dir}/${stem}_changed.json")
    file(STRINGS "${script}" changes REGEX "^# Mitcad change: ")
    set(sets)
    foreach(line IN LISTS changes)
      string(REGEX REPLACE "^# Mitcad change: *([^ =]+) *= *(.*)$" "\\1=\\2" change "${line}")
      list(APPEND sets --set "${change}")
    endforeach()
    if(sets)
      execute_process(COMMAND "${CLI}" import-fcstd "${file}" ${sets}
                              --reference "${dir}/${stem}_changed.json"
                              --report "${WORK}/${parent}-${stem}-changed.json"
                      RESULT_VARIABLE changed_status OUTPUT_VARIABLE changed_out ERROR_VARIABLE changed_err)
      string(REGEX MATCH "reference: [^\n]*" changed_reference "${changed_out}")
      string(REPLACE ";" " " shown "${sets}")
      set(reference "${reference}; changed (${shown}): ${changed_reference}")
      if(NOT changed_status EQUAL 0)
        set(status 1)
        string(REGEX MATCHALL "\n    [^\n]*" changed_differences "${changed_out}")
        set(out "\n    after ${shown}:${changed_err}${changed_differences}")
      endif()
    endif()
  endif()
  if(status EQUAL 0)
    message("ok     ${parent}/${stem}: ${summary}; ${reference}")
  else()
    string(REGEX MATCHALL "\n    [^\n]*" differences "${out}")
    message("FAILED ${parent}/${stem}: ${summary}; ${reference}${err}${differences}")
    list(APPEND failed "${parent}/${stem}")
  endif()
endforeach()
list(LENGTH failed failures)
message("FreeCAD corpus: ${count} files, ${compared} with a dump, ${failures} failed")
if(failed)
  message(FATAL_ERROR "failed: ${failed}")
endif()
