# SPDX-License-Identifier: MIT
# The .serp import on real files (mitcad#1870): the published examples of
# the files' writer, kept outside the repository. Every file an index.tsv
# under MITCAD_SERP_CORPUS lists (the folder of each index and below) is
# imported through OCCT with `mitcad-cli run`, in the index's folder.
#
# index.tsv: a header row naming the columns, then a row per file. The
# column `file` (the file's path relative to the index) is required. The
# column `check` names a script of commands and expectations
# (core/model-api/src/script.rs) beside the index that imports the file
# and checks what was read from the file itself: the bodies the import
# makes (as many as the file holds solids, surfaces and meshes), their
# volumes and bounding boxes, the components of its blocks. A row whose
# `check` is empty or `no` is only imported: the import must succeed.
# Other columns (its source, its licence) are notes. Files are named by
# their content id (the first 8 hex digits of their SHA-256). Each file's
# design goes to WORK. Skipped without the corpus.
#
#   cmake -DCLI=<mitcad-cli> -DWORK=<dir> -P serp-corpus.cmake

# The policies of a current CMake (CMP0007: empty fields of a row kept).
cmake_minimum_required(VERSION 3.24)

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "serp-corpus.cmake needs -D${var}=...")
  endif()
endforeach()

if(NOT DEFINED ENV{MITCAD_SERP_CORPUS} OR NOT IS_DIRECTORY "$ENV{MITCAD_SERP_CORPUS}")
  message("serp corpus: skipped (MITCAD_SERP_CORPUS not set)")
  return()
endif()
file(GLOB_RECURSE indexes "$ENV{MITCAD_SERP_CORPUS}/index.tsv")
if(NOT indexes)
  message("serp corpus: skipped (no index.tsv under MITCAD_SERP_CORPUS)")
  return()
endif()
file(MAKE_DIRECTORY "${WORK}")
set(count 0)
set(failed "")
set(labels "")
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
    string(STRIP "${relative}" relative)
    if(NOT EXISTS "${dir}/${relative}")
      list(APPEND failed "a listed file is missing (row ${count})")
      continue()
    endif()
    set(check "")
    if(check_column GREATER_EQUAL 0 AND check_column LESS n)
      list(GET cells ${check_column} check)
      string(STRIP "${check}" check)
    endif()
    file(SHA256 "${dir}/${relative}" hash)
    string(SUBSTRING "${hash}" 0 8 id)
    math(EXPR count "${count} + 1")
    # A file listed twice keeps a label of its own.
    set(label "${id}")
    if(label IN_LIST labels)
      set(label "${id}-${count}")
    endif()
    list(APPEND labels "${label}")

    if(check STREQUAL "" OR check STREQUAL "no")
      # Only imported: a script of the one command, the path as a JSON
      # string.
      string(REPLACE "\\" "\\\\" quoted "${relative}")
      string(REPLACE "\"" "\\\"" quoted "${quoted}")
      set(script "${WORK}/${label}.json")
      file(WRITE "${script}" "[{\"cmd\": \"import_file\", \"path\": \"${quoted}\"}]\n")
    else()
      set(script "${dir}/${check}")
      if(NOT EXISTS "${script}")
        list(APPEND failed "${label}: its check script is missing")
        continue()
      endif()
    endif()
    execute_process(
      COMMAND "${CLI}" run "${script}" --save "${WORK}/${label}.mitcad"
      WORKING_DIRECTORY "${dir}"
      OUTPUT_VARIABLE out ERROR_VARIABLE err RESULT_VARIABLE status)
    message("== ${label}\n${out}")
    if(NOT status EQUAL 0)
      if(err STREQUAL "")
        set(err "mitcad-cli run exited with ${status}")
      endif()
      list(APPEND failed "${label}: ${err}")
    endif()
  endforeach()
endforeach()
if(count EQUAL 0)
  message("serp corpus: skipped (the indexes list no files)")
  return()
endif()
list(LENGTH failed bad)
if(bad GREATER 0)
  string(REPLACE ";" "\n" lines "${failed}")
  message(FATAL_ERROR "serp corpus: ${bad} of ${count} files fail their checks:\n${lines}")
endif()
message("serp corpus: ${count} files pass their checks")
