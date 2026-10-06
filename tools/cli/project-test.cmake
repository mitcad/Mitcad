# SPDX-License-Identifier: MIT
# Project files of version 3 (P12a) through mitcad-cli: a project's store
# keeps each base feature's B-rep data once, the version 3 file opens with
# the same bodies as the single file it was converted from, and converted
# back it is the same file.
#
# cmake -DCLI=<mitcad-cli> -DSOURCE=<file.mitcad with base features>
#       -DWORK=<folder> -P project-test.cmake

foreach(var CLI SOURCE WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "project-test.cmake needs -D${var}=...")
  endif()
endforeach()

function(cli expect)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(expect STREQUAL "ok" AND NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  elseif(expect STREQUAL "fail" AND status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} should fail:\n${out}${err}")
  endif()
  set(out "${out}${err}" PARENT_SCOPE)
endfunction()

function(brep_files result)
  file(GLOB_RECURSE files "${WORK}/project/.mitcad/brep/*")
  list(LENGTH files count)
  set(${result} ${count} PARENT_SCOPE)
endfunction()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")

cli(ok project init "${WORK}/project" --no-history)
foreach(name .mitcad/project.json .gitattributes .gitignore)
  if(NOT EXISTS "${WORK}/project/${name}")
    message(FATAL_ERROR "project init did not write ${name}: ${out}")
  endif()
endforeach()

# Into the project: version 3, the data in the store.
set(v3 "${WORK}/project/parts/part.mitcad")
cli(ok convert "${SOURCE}" "${v3}" --format v3)
if(NOT out MATCHES "\\(version 3,")
  message(FATAL_ERROR "convert --format v3 did not write version 3: ${out}")
endif()
file(READ "${v3}" text)
if(NOT text MATCHES "\"sha256\": \"[0-9a-f]+\"" OR text MATCHES "\"data\":")
  message(FATAL_ERROR "the version 3 file does not refer to its B-rep data:\n${text}")
endif()
brep_files(stored)
if(stored EQUAL 0)
  message(FATAL_ERROR "no B-rep files in the project's store")
endif()

# Saved again (auto, in the project) as another file: no new B-rep file.
cli(ok convert "${v3}" "${WORK}/project/copy.mitcad")
brep_files(again)
if(NOT again EQUAL stored)
  message(FATAL_ERROR "the same B-rep data was stored again: ${stored} files, then ${again}")
endif()

# The same bodies (volumes, areas, faces) as the single file.
cli(ok info "${SOURCE}")
set(single_report "${out}")
cli(ok info "${v3}")
if(NOT out STREQUAL single_report)
  message(FATAL_ERROR "the version 3 file opens differently:\n${out}\nthe single file:\n${single_report}")
endif()

# Back to one file (v2), outside the project: the same file.
cli(ok convert "${v3}" "${WORK}/single.mitcad" --format v2)
file(READ "${SOURCE}" source_text)
file(READ "${WORK}/single.mitcad" back_text)
if(NOT back_text STREQUAL source_text)
  message(FATAL_ERROR "v2 -> v3 -> v2 changed the file")
endif()

# Version 3 needs a project.
cli(fail convert "${v3}" "${WORK}/outside.mitcad" --format v3)
if(NOT out MATCHES "is not in a Mitcad project")
  message(FATAL_ERROR "convert --format v3 outside a project: ${out}")
endif()

# Missing data: the file opens, its base features fail with a message.
file(GLOB_RECURSE files "${WORK}/project/.mitcad/brep/*")
list(GET files 0 first)
file(REMOVE "${first}")
cli(ok info "${v3}")
if(NOT out MATCHES "is missing from the project store \\(\\.mitcad/brep\\)")
  message(FATAL_ERROR "a missing B-rep file is not reported:\n${out}")
endif()
message(STATUS "project files of version 3: ${stored} B-rep files, same bodies, same file back")
