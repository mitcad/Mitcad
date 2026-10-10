# SPDX-License-Identifier: MIT
# Assembly files kept outside the repository (never committed): every .iam
# under the folders of MITCAD_IPT_CORPUS (a path list, like PATH) is
# imported with mitcad-cli import-iam (mitcad#60, stage 4), each part with
# its parameters and features, the corpus folders searched for referenced
# files by name. Every occurrence that is not suppressed must be placed,
# every part file found must import, every placement must agree with the
# one the file displays (where the file keeps one), and every placed
# part's bodies must lie within the range box the file stores for it.
# Files that are not in the corpus (library parts, files of other
# projects) are reported, not failed. Prints a line per file; skipped
# without the corpus.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> [-DRUNNER=<mitcad_run_parallel>]
#       -P iam-corpus.cmake
#
# With RUNNER, each file is checked in a child process of its own (this
# script with -DFILE=<file>), several at once, and the lines come in the
# files' order (mitcad#70, core/tests/parallel_runs.hpp): MITCAD_CORPUS_JOBS,
# MITCAD_CORPUS_MEMORY and MITCAD_CORPUS_TIMEOUT. MITCAD_CORPUS_JOBS=1
# checks them one after another in this process.

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "iam-corpus.cmake needs -D${var}=...")
  endif()
endforeach()

set(corpus "$ENV{MITCAD_IPT_CORPUS}")
if(NOT CMAKE_HOST_WIN32)
  string(REPLACE ":" ";" corpus "${corpus}")
endif()

# A file's label in the lines: its path in its corpus folder without the
# extension (copies of a project hold assemblies of the same name).
function(label_of file out)
  set(label "")
  foreach(dir IN LISTS corpus)
    if(NOT label AND IS_DIRECTORY "${dir}")
      file(RELATIVE_PATH relative "${dir}" "${file}")
      if(NOT relative MATCHES "^\\.\\./")
        set(label "${relative}")
      endif()
    endif()
  endforeach()
  if(NOT label)
    get_filename_component(label "${file}" NAME)
  endif()
  string(REGEX REPLACE "\\.[^./]*$" "" label "${label}")
  set(${out} "${label}" PARENT_SCOPE)
endfunction()

# Checks one file and prints its line; `file_failed` tells the caller.
function(check_file file)
  label_of("${file}" label)
  string(REPLACE "/" "-" report "${label}")
  set(arguments import-iam "${file}" --report "${WORK}/${report}.json")
  foreach(folder IN LISTS corpus)
    if(IS_DIRECTORY "${folder}")
      list(APPEND arguments --search "${folder}")
    endif()
  endforeach()
  execute_process(COMMAND "${CLI}" ${arguments} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  string(REGEX MATCH "Imported assembly [^\n]*" summary "${out}")
  string(REGEX REPLACE "Imported assembly [^:]+: " "" summary "${summary}")
  string(REGEX MATCH "files found: [^\n]*" found "${out}")
  string(REGEX MATCH "check: [0-9]+ of [0-9]+ parts. bodies lie within[^\n]*" boxes "${out}")
  string(REGEX MATCH "check: [0-9]+ of [0-9]+ placements agree[^\n]*" centres "${out}")
  if(NOT summary)
    set(status 1)
  endif()
  # Every occurrence placed, every part file found imported.
  if(summary MATCHES "[^0-9]0 not placed" AND summary MATCHES " 0 failed;")
  else()
    set(status 1)
  endif()
  if(status EQUAL 0)
    message("ok     ${label}: ${summary}; ${found}; ${boxes}; ${centres}")
    set(file_failed FALSE PARENT_SCOPE)
  else()
    string(REGEX MATCHALL "\n  (check failed|warning)[^\n]*|\n  part [^\n]*failed[^\n]*" problems "${out}")
    message("FAILED ${label}: ${summary}; ${found}; ${boxes}; ${centres}${err}${problems}")
    set(file_failed TRUE PARENT_SCOPE)
  endif()
endfunction()

# A child of the parallel run: one file.
if(DEFINED FILE)
  check_file("${FILE}")
  return()
endif()

set(files)
foreach(dir IN LISTS corpus)
  if(IS_DIRECTORY "${dir}")
    file(GLOB_RECURSE found "${dir}/*.iam" "${dir}/*.IAM")
    list(APPEND files ${found})
  endif()
endforeach()
list(REMOVE_DUPLICATES files)
list(SORT files)
if(NOT files)
  message("iam corpus: skipped (set MITCAD_IPT_CORPUS to folders of .iam files)")
  return()
endif()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(failed)
list(LENGTH files count)
set(jobs "$ENV{MITCAD_CORPUS_JOBS}")
if(DEFINED RUNNER AND NOT jobs STREQUAL "1")
  # Each file in a child process of its own; the runner prints their lines
  # in this order and "FAILED <name>: ..." for a child that did not finish.
  set(list "${WORK}/files.txt")
  file(WRITE "${list}" "")
  foreach(file IN LISTS files)
    label_of("${file}" label)
    file(APPEND "${list}" "${label}\t${CMAKE_COMMAND}\t-DCLI=${CLI}\t-DWORK=${WORK}\t-DFILE=${file}\t-P\t${CMAKE_CURRENT_LIST_FILE}\n")
  endforeach()
  execute_process(COMMAND "${RUNNER}" "${list}" RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE out)
  if(status GREATER 1)
    message(FATAL_ERROR "${RUNNER}: ${out}")
  endif()
  string(REGEX REPLACE "\n$" "" shown "${out}")
  message("${shown}")
  string(REGEX MATCHALL "(^|\n)FAILED [^:\n]*:" lines "${out}")
  foreach(line IN LISTS lines)
    string(REGEX REPLACE "^\n?FAILED ([^:\n]*):$" "\\1" name "${line}")
    list(APPEND failed "${name}")
  endforeach()
  if(failed)
    list(REMOVE_DUPLICATES failed)
  endif()
else()
  foreach(file IN LISTS files)
    label_of("${file}" label)
    check_file("${file}")
    if(file_failed)
      list(APPEND failed "${label}")
    endif()
  endforeach()
endif()
list(LENGTH failed failures)
message("iam corpus: ${count} files, ${failures} failed")
if(failed)
  message(FATAL_ERROR "failed: ${failed}")
endif()
