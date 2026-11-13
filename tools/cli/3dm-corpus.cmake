# SPDX-License-Identifier: MIT
# The .3dm import on real models (mitcad#1715): every model an index.tsv
# under MITCAD_3DM_CORPUS lists (the folder of each index and below) is
# imported with mitcad-cli import-3dm --check, which fails when a body is
# not built or not valid, or a solid, a surface or a mesh is left out. A
# model that cannot be read, or that ends the import, fails too.
#
# index.tsv: a header row naming the columns, then a row per model. The
# column `file` (the model's path relative to the index) is required. A
# row whose column `check` says `no` is imported without --check (a model
# with objects the import leaves out on purpose, e.g. subdivision
# objects): it must still be read and imported. Other columns (its source,
# its licence) are notes. Models are named by their content id (the first
# 8 hex digits of their SHA-256). Each model's report and design go to
# WORK. Skipped without the corpus.
#
#   cmake -DCLI=<mitcad-cli> -DWORK=<dir> [-DRUNNER=<mitcad_run_parallel>]
#         -P 3dm-corpus.cmake
#
# With RUNNER, each model is imported in a child process of its own (this
# script with -DMODEL=<file>), several at once, as dwg-corpus.cmake does
# (MITCAD_CORPUS_JOBS=1 imports them one after another in this process).

# The policies of a current CMake (CMP0007: empty fields of a row kept).
cmake_minimum_required(VERSION 3.24)

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "3dm-corpus.cmake needs -D${var}=...")
  endif()
endforeach()

# Imports one model and prints its lines; `model_error` tells the caller
# why it failed (empty when it passed).
function(check_model model id check)
  set(check_args --check)
  if(check STREQUAL "no")
    set(check_args "")
  endif()
  execute_process(
    COMMAND "${CLI}" import-3dm "${model}" ${check_args} --report "${WORK}/${id}.json" --save "${WORK}/${id}.mitcad"
    OUTPUT_VARIABLE out ERROR_VARIABLE err RESULT_VARIABLE status)
  message("== ${id}\n${out}")
  set(model_error "" PARENT_SCOPE)
  if(NOT status EQUAL 0)
    if(err STREQUAL "")
      set(err "import-3dm exited with ${status}")
    endif()
    set(model_error "${err}" PARENT_SCOPE)
  endif()
endfunction()

# A child of the parallel run: one model.
if(DEFINED MODEL)
  check_model("${MODEL}" "${ID}" "${CHECK}")
  if(NOT model_error STREQUAL "")
    message(FATAL_ERROR "${ID}: ${model_error}")
  endif()
  return()
endif()

if(NOT DEFINED ENV{MITCAD_3DM_CORPUS} OR NOT IS_DIRECTORY "$ENV{MITCAD_3DM_CORPUS}")
  message("3dm corpus: skipped (MITCAD_3DM_CORPUS not set)")
  return()
endif()
file(GLOB_RECURSE indexes "$ENV{MITCAD_3DM_CORPUS}/index.tsv")
if(NOT indexes)
  message("3dm corpus: skipped (no index.tsv under MITCAD_3DM_CORPUS)")
  return()
endif()
file(MAKE_DIRECTORY "${WORK}")
set(count 0)
set(failed "")
set(labels "")
set(models "")
set(checks "")
foreach(index IN LISTS indexes)
  get_filename_component(dir "${index}" DIRECTORY)
  file(STRINGS "${index}" rows)
  set(header "")
  foreach(row IN LISTS rows)
    if(row MATCHES "^[ \t]*(#|$)")
      continue()
    endif()
    # Tabs separate the columns (empty columns kept).
    string(REPLACE "\t" ";" cells "${row}")
    if(header STREQUAL "")
      set(header "${cells}")
      list(FIND header file file_column)
      if(file_column LESS 0)
        message(FATAL_ERROR "${index}: the header has no column 'file'")
      endif()
      list(FIND header check check_column)
      continue()
    endif()
    list(LENGTH cells n)
    if(n LESS_EQUAL file_column)
      continue()
    endif()
    list(GET cells ${file_column} relative)
    set(model "${dir}/${relative}")
    if(NOT EXISTS "${model}")
      list(APPEND failed "a listed model is missing (row ${count})")
      continue()
    endif()
    set(check "yes")
    if(check_column GREATER_EQUAL 0 AND check_column LESS n)
      list(GET cells ${check_column} value)
      string(STRIP "${value}" value)
      if(value STREQUAL "no")
        set(check "no")
      endif()
    endif()
    file(SHA256 "${model}" hash)
    string(SUBSTRING "${hash}" 0 8 id)
    math(EXPR count "${count} + 1")
    # A model listed twice keeps a label of its own.
    set(label "${id}")
    if(label IN_LIST labels)
      set(label "${id}-${count}")
    endif()
    list(APPEND labels "${label}")
    list(APPEND models "${model}")
    list(APPEND checks "${check}")
  endforeach()
endforeach()
if(count EQUAL 0)
  message("3dm corpus: skipped (the indexes list no models)")
  return()
endif()
math(EXPR last "${count} - 1")
set(jobs "$ENV{MITCAD_CORPUS_JOBS}")
if(DEFINED RUNNER AND NOT jobs STREQUAL "1")
  # Each model in a child process of its own; the runner prints their
  # lines in this order and "FAILED <label>: ..." for a child that failed.
  set(list "${WORK}/models.txt")
  file(WRITE "${list}" "")
  foreach(i RANGE ${last})
    list(GET labels ${i} label)
    list(GET models ${i} model)
    list(GET checks ${i} check)
    file(APPEND "${list}" "${label}\t${CMAKE_COMMAND}\t-DCLI=${CLI}\t-DWORK=${WORK}\t-DMODEL=${model}\t-DID=${label}\t-DCHECK=${check}\t-P\t${CMAKE_CURRENT_LIST_FILE}\n")
  endforeach()
  execute_process(COMMAND "${RUNNER}" "${list}" RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE out)
  # 0: every child passed, 1: some failed; anything else (the runner not
  # built, or ended) is no pass.
  if(NOT status MATCHES "^[01]$")
    message(FATAL_ERROR "${RUNNER}: ${out}")
  endif()
  string(REGEX REPLACE "\n$" "" shown "${out}")
  message("${shown}")
  string(REGEX MATCHALL "(^|\n)FAILED [^:\n]*:" lines "${out}")
  foreach(line IN LISTS lines)
    string(REGEX REPLACE "^\n?FAILED ([^:\n]*):$" "\\1" name "${line}")
    list(APPEND failed "${name}: see its lines above")
  endforeach()
else()
  foreach(i RANGE ${last})
    list(GET labels ${i} label)
    list(GET models ${i} model)
    list(GET checks ${i} check)
    check_model("${model}" "${label}" "${check}")
    if(NOT model_error STREQUAL "")
      list(APPEND failed "${label}: ${model_error}")
    endif()
  endforeach()
endif()
list(LENGTH failed bad)
if(bad GREATER 0)
  string(REPLACE ";" "\n" lines "${failed}")
  message(FATAL_ERROR "3dm corpus: ${bad} of ${count} models fail their checks:\n${lines}")
endif()
message("3dm corpus: ${count} models pass their checks")
