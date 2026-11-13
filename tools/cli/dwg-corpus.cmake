# SPDX-License-Identifier: MIT
# The .dwg import on the trial models (mitcad#453): every drawing an
# index.tsv under MITCAD_DWG_CORPUS lists (the folder of each index and
# below) is imported with mitcad-cli import-dwg --check, which fails when
# a body is not built or not valid, the bodies lie outside the drawing's
# $EXTMIN/$EXTMAX, or a data sheet value of the row disagrees.
#
# index.tsv: a header row naming the columns, then a row per drawing. The
# column `file` (the drawing's path relative to the index) is required;
# these are checked when a row gives them (the import report's `checks`):
#
#   length_mm          the side of the bodies' box nearest to it (within
#                      1 mm or 0.5 %)
#   chord_distance_mm  with tube_diameter_mm, the nearest of the other
#                      sides (within 1 mm or 1 %)
#   tube_diameter_mm   a cylindrical face of the diameter (0.1 mm)
#   tube_wall_mm       and one of the inner diameter
#   mass_kg            the solids' volume times density_kg_m3 (default
#                      2700, aluminium), within 5 %
#   density_kg_m3
#   volume_mm3         the solids' volume (within 1 %), e.g. of a
#                      reference model of the product
#   solids, components their counts
#
# Other columns (the product, its source, the date) are notes. Drawings
# are named by their content id (the first 8 hex digits of their SHA-256).
# Skipped without the corpus.
#
#   cmake -DCLI=<mitcad-cli> -DWORK=<dir> [-DRUNNER=<mitcad_run_parallel>]
#         -P dwg-corpus.cmake
#
# With RUNNER, each drawing is imported in a child process of its own (this
# script with -DDRAWING=<file>), several at once, the lines in the rows'
# order (mitcad#875, core/tests/parallel_runs.hpp), each a file of the
# test runner's profile by its content id, so that the runner gives the
# test its children (tools/testrun/README.md, "Corpus tests"):
# MITCAD_CORPUS_JOBS, MITCAD_CORPUS_MEMORY and MITCAD_CORPUS_TIMEOUT as for
# the other corpus tests; MITCAD_CORPUS_JOBS=1 imports them one after
# another in this process.

# The policies of a current CMake: without them an older one (Ubuntu
# 24.04's 3.28, the Linux release's build root) drops a row's empty fields
# (CMP0007), and every value after one is read from the wrong column.
cmake_minimum_required(VERSION 3.24)

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "dwg-corpus.cmake needs -D${var}=...")
  endif()
endforeach()

# Imports one drawing and prints its lines; `drawing_error` tells the
# caller why it failed (empty when it passed).
function(check_drawing drawing id expect)
  set(expect_args "")
  if(NOT expect STREQUAL "")
    set(expect_args --expect "${expect}")
  endif()
  execute_process(
    COMMAND "${CLI}" import-dwg "${drawing}" --check --report "${WORK}/${id}.json" --save "${WORK}/${id}.mitcad"
            ${expect_args}
    OUTPUT_VARIABLE out ERROR_VARIABLE err RESULT_VARIABLE status)
  message("== ${id}\n${out}")
  set(drawing_error "" PARENT_SCOPE)
  if(NOT status EQUAL 0)
    if(err STREQUAL "")
      set(err "import-dwg exited with ${status}")
    endif()
    set(drawing_error "${err}" PARENT_SCOPE)
  endif()
endfunction()

# A child of the parallel run: one drawing.
if(DEFINED DRAWING)
  check_drawing("${DRAWING}" "${ID}" "${EXPECT}")
  if(NOT drawing_error STREQUAL "")
    message(FATAL_ERROR "${ID}: ${drawing_error}")
  endif()
  return()
endif()

if(NOT DEFINED ENV{MITCAD_DWG_CORPUS} OR NOT IS_DIRECTORY "$ENV{MITCAD_DWG_CORPUS}")
  message("dwg corpus: skipped (MITCAD_DWG_CORPUS not set)")
  return()
endif()
file(GLOB_RECURSE indexes "$ENV{MITCAD_DWG_CORPUS}/index.tsv")
if(NOT indexes)
  message("dwg corpus: skipped (no index.tsv under MITCAD_DWG_CORPUS)")
  return()
endif()
file(MAKE_DIRECTORY "${WORK}")
set(checked_columns length_mm chord_distance_mm tube_diameter_mm tube_wall_mm mass_kg density_kg_m3 volume_mm3 solids
  components)
set(count 0)
set(failed "")
set(labels "")
set(drawings "")
set(expects "")
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
      continue()
    endif()
    list(LENGTH cells n)
    if(n LESS_EQUAL file_column)
      continue()
    endif()
    list(GET cells ${file_column} relative)
    set(drawing "${dir}/${relative}")
    if(NOT EXISTS "${drawing}")
      list(APPEND failed "a listed drawing is missing (row ${count})")
      continue()
    endif()
    file(SHA256 "${drawing}" hash)
    string(SUBSTRING "${hash}" 0 8 id)
    set(expect "")
    foreach(column IN LISTS checked_columns)
      list(FIND header ${column} at)
      if(at GREATER_EQUAL 0 AND at LESS n)
        list(GET cells ${at} value)
        string(STRIP "${value}" value)
        if(value MATCHES "^[0-9]+(\\.[0-9]+)?$")
          if(expect STREQUAL "")
            set(expect "${column}=${value}")
          else()
            set(expect "${expect},${column}=${value}")
          endif()
        endif()
      endif()
    endforeach()
    math(EXPR count "${count} + 1")
    # A drawing listed twice keeps a label of its own.
    set(label "${id}")
    if(label IN_LIST labels)
      set(label "${id}-${count}")
    endif()
    list(APPEND labels "${label}")
    list(APPEND drawings "${drawing}")
    # "-" for none: an empty first element would not be kept.
    list(APPEND expects "-${expect}")
  endforeach()
endforeach()
if(count EQUAL 0)
  message("dwg corpus: skipped (the indexes list no drawings)")
  return()
endif()
math(EXPR last "${count} - 1")
set(jobs "$ENV{MITCAD_CORPUS_JOBS}")
if(DEFINED RUNNER AND NOT jobs STREQUAL "1")
  # Each drawing in a child process of its own; the runner prints their
  # lines in this order and "FAILED <label>: ..." for a child that failed.
  set(list "${WORK}/drawings.txt")
  file(WRITE "${list}" "")
  foreach(i RANGE ${last})
    list(GET labels ${i} label)
    list(GET drawings ${i} drawing)
    list(GET expects ${i} expect)
    string(SUBSTRING "${expect}" 1 -1 expect)
    file(APPEND "${list}" "${label}\t${CMAKE_COMMAND}\t-DCLI=${CLI}\t-DWORK=${WORK}\t-DDRAWING=${drawing}\t-DID=${label}\t-DEXPECT=${expect}\t-P\t${CMAKE_CURRENT_LIST_FILE}\n")
  endforeach()
  execute_process(COMMAND "${RUNNER}" "${list}" RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE out)
  # 0: every child passed, 1: some failed; anything else (the runner not
  # built, or ended) is no pass (mitcad#1718).
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
    list(GET drawings ${i} drawing)
    list(GET expects ${i} expect)
    string(SUBSTRING "${expect}" 1 -1 expect)
    check_drawing("${drawing}" "${label}" "${expect}")
    if(NOT drawing_error STREQUAL "")
      list(APPEND failed "${label}: ${drawing_error}")
    endif()
  endforeach()
endif()
list(LENGTH failed bad)
if(bad GREATER 0)
  string(REPLACE ";" "\n" lines "${failed}")
  message(FATAL_ERROR "dwg corpus: ${bad} of ${count} drawings fail their checks:\n${lines}")
endif()
message("dwg corpus: ${count} drawings pass their checks")
