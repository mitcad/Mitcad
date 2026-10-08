# SPDX-License-Identifier: MIT
# Part files kept outside the repository (never committed): every .ipt
# under the folders of MITCAD_IPT_CORPUS (a path list, like PATH) is
# imported with mitcad-cli import-ipt (mitcad#60), with its parameters and
# features replayed against the ASM history. Every body must be built and
# every solid valid; every parameter expression must agree with its stored
# value (in the file and in Mitcad); the replay's final bodies must be the
# file's (none missing, none extra). The line per file gives the features'
# outcomes. A file with a STEP file of the same part next to
# it (<name>.stp or <name>.step) is compared with it: the same number of
# solids, each one's volume and area within a relative limit, 1e-6 unless
# the folder's references.tsv gives the file another one
# ("<name>.ipt<TAB><limit><TAB><why>": a reference that was not exported
# from this file models the part a little differently, and the limit is
# that difference as measured, so that a change in what the import reads
# still fails). Prints a line per file; skipped without the corpus.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> [-DRUNNER=<mitcad_run_parallel>]
#       -P ipt-corpus.cmake
#
# With RUNNER, each file is checked in a child process of its own (this
# script with -DFILE=<file>), several at once, and the lines come in the
# files' order (mitcad#70, core/tests/parallel_runs.hpp): MITCAD_CORPUS_JOBS,
# MITCAD_CORPUS_MEMORY and MITCAD_CORPUS_TIMEOUT. MITCAD_CORPUS_JOBS=1
# checks them one after another in this process.

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "ipt-corpus.cmake needs -D${var}=...")
  endif()
endforeach()

# The STEP reference of a file, if any.
function(reference_of file out)
  get_filename_component(dir "${file}" DIRECTORY)
  get_filename_component(stem "${file}" NAME_WE)
  set(found "")
  foreach(extension stp step STP STEP)
    if(NOT found AND EXISTS "${dir}/${stem}.${extension}")
      set(found "${dir}/${stem}.${extension}")
    endif()
  endforeach()
  set(${out} "${found}" PARENT_SCOPE)
endfunction()

# Checks one file and prints its line; `file_failed` tells the caller.
function(check_file file)
  get_filename_component(dir "${file}" DIRECTORY)
  get_filename_component(name "${file}" NAME)
  get_filename_component(stem "${file}" NAME_WE)
  get_filename_component(parent "${dir}" NAME)
  set(arguments import-ipt "${file}" --report "${WORK}/${parent}-${stem}.json")
  reference_of("${file}" reference)
  set(limit "")
  if(reference)
    list(APPEND arguments --reference "${reference}")
    if(EXISTS "${dir}/references.tsv")
      file(STRINGS "${dir}/references.tsv" lines)
      foreach(line IN LISTS lines)
        if(line MATCHES "^${name}\t([^\t]+)")
          set(limit "${CMAKE_MATCH_1}")
        endif()
      endforeach()
    endif()
    if(limit)
      list(APPEND arguments --max-relative "${limit}")
    endif()
  endif()
  execute_process(COMMAND "${CLI}" ${arguments} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  string(REGEX MATCH "Imported [0-9]+ bodies[^\n]*" summary "${out}")
  string(REGEX REPLACE "Imported ([0-9]+ bodies) of [^ ]+ " "\\1 " summary "${summary}")
  string(REGEX MATCH "reference [^\n]*" compared "${out}")
  string(REGEX MATCH "[0-9]+ timeline items: [^\n]*" items "${out}")
  string(REGEX MATCH "expressions: [^\n]*" expressions "${out}")
  if(items)
    set(summary "${summary}; ${items}")
  endif()
  # The parameters: every expression translated agrees with the file, and
  # with Mitcad's evaluation.
  if(expressions MATCHES "expressions: ([0-9]+) translated, ([0-9]+) agree with the stored values, ([0-9]+) differ")
    if(NOT CMAKE_MATCH_1 EQUAL CMAKE_MATCH_2 OR NOT CMAKE_MATCH_3 EQUAL 0)
      set(status 1)
    endif()
  endif()
  if(out MATCHES "parameters: [0-9]+ imported, [0-9]+ as values, [0-9]+ renamed, [0-9]+ skipped, ([0-9]+) mismatched")
    if(NOT CMAKE_MATCH_1 EQUAL 0)
      set(status 1)
    endif()
  endif()
  # The final bodies are the file's.
  if(out MATCHES "not in the replay|extra body in the replay")
    set(status 1)
  endif()
  # Every body built and valid.
  if(out MATCHES "Imported ([0-9]+) bodies of [^\n]* \\([0-9]+ solids, ([0-9]+) valid; ([0-9]+) not built\\)")
    if(NOT CMAKE_MATCH_2 EQUAL CMAKE_MATCH_1 OR NOT CMAKE_MATCH_3 EQUAL 0)
      set(status 1)
    endif()
  else()
    set(status 1)
  endif()
  if(status EQUAL 0)
    if(compared)
      message("ok     ${parent}/${stem}: ${summary}; ${compared}")
    else()
      message("ok     ${parent}/${stem}: ${summary}; no reference")
    endif()
    set(file_failed FALSE PARENT_SCOPE)
  else()
    string(REGEX MATCHALL "\n  (not built|warning)[^\n]*|\n[^\n]*(not in the replay|extra body|mismatched)[^\n]*" problems "${out}")
    string(REGEX MATCHALL "\n    [^\n]*" differences "${out}")
    string(REGEX MATCHALL "\n  [^\n]*INVALID[^\n]*" invalid "${out}")
    message("FAILED ${parent}/${stem}: ${summary}; ${compared}${err}${invalid}${problems}${differences}")
    set(file_failed TRUE PARENT_SCOPE)
  endif()
endfunction()

# A child of the parallel run: one file.
if(DEFINED FILE)
  check_file("${FILE}")
  return()
endif()

set(corpus "$ENV{MITCAD_IPT_CORPUS}")
if(NOT CMAKE_HOST_WIN32)
  string(REPLACE ":" ";" corpus "${corpus}")
endif()
set(files)
foreach(dir IN LISTS corpus)
  if(IS_DIRECTORY "${dir}")
    file(GLOB_RECURSE found "${dir}/*.ipt" "${dir}/*.IPT")
    list(APPEND files ${found})
  endif()
endforeach()
list(REMOVE_DUPLICATES files)
list(SORT files)
if(NOT files)
  message("ipt corpus: skipped (set MITCAD_IPT_CORPUS to folders of .ipt files)")
  return()
endif()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(failed)
set(compared 0)
list(LENGTH files count)
foreach(file IN LISTS files)
  reference_of("${file}" reference)
  if(reference)
    math(EXPR compared "${compared} + 1")
  endif()
endforeach()
set(jobs "$ENV{MITCAD_CORPUS_JOBS}")
if(DEFINED RUNNER AND NOT jobs STREQUAL "1")
  # Each file in a child process of its own; the runner prints their lines
  # in this order and "FAILED <name>: ..." for a child that did not finish.
  set(list "${WORK}/files.txt")
  file(WRITE "${list}" "")
  foreach(file IN LISTS files)
    get_filename_component(dir "${file}" DIRECTORY)
    get_filename_component(stem "${file}" NAME_WE)
    get_filename_component(parent "${dir}" NAME)
    file(APPEND "${list}" "${parent}/${stem}\t${CMAKE_COMMAND}\t-DCLI=${CLI}\t-DWORK=${WORK}\t-DFILE=${file}\t-P\t${CMAKE_CURRENT_LIST_FILE}\n")
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
    get_filename_component(dir "${file}" DIRECTORY)
    get_filename_component(stem "${file}" NAME_WE)
    get_filename_component(parent "${dir}" NAME)
    check_file("${file}")
    if(file_failed)
      list(APPEND failed "${parent}/${stem}")
    endif()
  endforeach()
endif()
list(LENGTH failed failures)
message("ipt corpus: ${count} files, ${compared} with a STEP reference, ${failures} failed")
if(failed)
  message(FATAL_ERROR "failed: ${failed}")
endif()
