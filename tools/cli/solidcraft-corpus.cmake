# SPDX-License-Identifier: MIT
# SolidCraft designs kept outside the repository (never committed): every
# .sc or .solidcraft under the folders of MITCAD_SOLIDCRAFT_CORPUS (a path list,
# like PATH) is imported with mitcad-cli import-solidcraft; one with
# <name>.expected.json next to it (SolidCraft's own evaluation of the
# design, `solidcraft-cli eval`, or a subset of it) is checked against it.
# A file listed in <folder>/known.txt (a line per file name, then a tab
# and the reason) may fail its check; it is reported, not failed. Prints a
# line per file; skipped without the corpus.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P solidcraft-corpus.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "solidcraft-corpus.cmake needs -D${var}=...")
  endif()
endforeach()
if(NOT DEFINED ENV{MITCAD_SOLIDCRAFT_CORPUS} OR "$ENV{MITCAD_SOLIDCRAFT_CORPUS}" STREQUAL "")
  message("SolidCraft corpus: skipped (set MITCAD_SOLIDCRAFT_CORPUS to folders of .sc or .solidcraft files)")
  return()
endif()
file(MAKE_DIRECTORY "${WORK}")
set(folders "$ENV{MITCAD_SOLIDCRAFT_CORPUS}")
if(NOT WIN32)
  string(REPLACE ":" ";" folders "${folders}")
endif()
set(files)
foreach(dir IN LISTS folders)
  # .sc as SolidCraft saves designs now (mitcad#1622); any case.
  file(GLOB_RECURSE found "${dir}/*.solidcraft" "${dir}/*.sc" "${dir}/*.SC")
  list(APPEND files ${found})
endforeach()
# *.sc and *.SC find a file twice where names ignore case (Windows, macOS).
list(REMOVE_DUPLICATES files)
list(SORT files)
set(failed 0)
set(total 0)
foreach(file IN LISTS files)
  math(EXPR total "${total} + 1")
  get_filename_component(dir "${file}" DIRECTORY)
  get_filename_component(stem "${file}" NAME_WE)
  get_filename_component(name "${file}" NAME)
  set(arguments import-solidcraft "${file}" --report "${WORK}/${stem}.json")
  if(EXISTS "${dir}/${stem}.expected.json")
    list(APPEND arguments --expected "${dir}/${stem}.expected.json")
  endif()
  execute_process(COMMAND "${CLI}" ${arguments} RESULT_VARIABLE result OUTPUT_VARIABLE out ERROR_VARIABLE err
                  TIMEOUT 900)
  string(REGEX MATCH "^[^\n]*" first "${out}")
  string(REGEX MATCH "check: [^\n]*" check "${out}")
  set(known "")
  if(EXISTS "${dir}/known.txt")
    file(STRINGS "${dir}/known.txt" lines)
    foreach(line IN LISTS lines)
      if(line MATCHES "^${name}\t(.*)$")
        set(known "${CMAKE_MATCH_1}")
      endif()
    endforeach()
  endif()
  if(result EQUAL 0)
    message("ok ${stem}: ${first} ${check}")
  elseif(NOT known STREQUAL "")
    message("known ${stem}: ${first} ${check} (${known})")
  else()
    message("FAILED ${stem}: ${first} ${check}${err}")
    math(EXPR failed "${failed} + 1")
  endif()
endforeach()
message("SolidCraft corpus: ${total} designs, ${failed} failed")
if(failed GREATER 0)
  message(FATAL_ERROR "${failed} of ${total} designs failed")
endif()
