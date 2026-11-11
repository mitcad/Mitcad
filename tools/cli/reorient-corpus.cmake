# SPDX-License-Identifier: MIT
# Reorient Design on the test files' assemblies (mitcad#537), kept outside
# the repository (never committed) and found by content id (the first 8
# hex digits of their SHA-256) under the folders of MITCAD_IAM_CORPUS, or
# MITCAD_IPT_CORPUS when that is unset (a
# path list, like PATH; an INDEX.tsv or index.tsv there names the files by
# id, else the .iam files are hashed):
#
# - c71542a1, a bench with drawers and a figure whose stored body OCCT's
#   checker finds invalid, imported with its parts' design history and with
#   their stored bodies only: every offered turn (X, Y and Z by 90, 180 and
#   270 degrees, XZ and YZ onto XY) is taken, so every body and occurrence
#   is where the turn takes it (the command refuses it otherwise);
# - 1dc60c2a, the drawer, with its parts' history: turned 270 about X, then
#   that turned design again by 90 and by 270 about Z.
#
# Prints a line per turn; skipped without the files.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P reorient-corpus.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "reorient-corpus.cmake needs -D${var}=...")
  endif()
endforeach()

set(corpus "$ENV{MITCAD_IAM_CORPUS}")
if(corpus STREQUAL "")
  set(corpus "$ENV{MITCAD_IPT_CORPUS}")
endif()
if(NOT CMAKE_HOST_WIN32)
  string(REPLACE ":" ";" corpus "${corpus}")
endif()

# The .iam file with this content id, or "".
function(find_assembly id out)
  set(found "")
  foreach(dir IN LISTS corpus)
    foreach(index INDEX.tsv index.tsv)
      if(NOT found AND EXISTS "${dir}/${index}")
        file(STRINGS "${dir}/${index}" rows ENCODING UTF-8 REGEX "\t${id}")
        foreach(row IN LISTS rows)
          string(REGEX MATCH "^[^\t]*" path "${row}")
          if(NOT found AND EXISTS "${dir}/${path}")
            set(found "${dir}/${path}")
          endif()
        endforeach()
      endif()
    endforeach()
  endforeach()
  if(NOT found)
    foreach(dir IN LISTS corpus)
      if(NOT found AND IS_DIRECTORY "${dir}")
        file(GLOB_RECURSE files "${dir}/*.iam" "${dir}/*.IAM")
        foreach(file IN LISTS files)
          if(NOT found)
            file(SHA256 "${file}" hash)
            if(hash MATCHES "^${id}")
              set(found "${file}")
            endif()
          endif()
        endforeach()
      endif()
    endforeach()
  endif()
  set(${out} "${found}" PARENT_SCOPE)
endfunction()

find_assembly(c71542a1 bench)
find_assembly(1dc60c2a drawer)
if(NOT bench AND NOT drawer)
  message("reorient corpus: skipped (set MITCAD_IAM_CORPUS or MITCAD_IPT_CORPUS to the test files' folders)")
  return()
endif()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(failures 0)

# Imports an assembly into <WORK>/<name>.mitcad.
function(import file name)
  execute_process(COMMAND "${CLI}" import-iam "${file}" ${ARGN} --save "${WORK}/${name}.mitcad"
    RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT EXISTS "${WORK}/${name}.mitcad")
    message(FATAL_ERROR "${name}: the import failed (${status}): ${err}")
  endif()
endfunction()

# Runs one turn on a design (optionally saving the turned one) and prints
# its line.
function(turn design label step)
  set(script "${WORK}/turn-${label}.json")
  file(WRITE "${script}" "[${step}]\n")
  set(save)
  if(ARGN)
    set(save --save "${ARGN}")
  endif()
  execute_process(COMMAND "${CLI}" run "${script}" --open "${design}" ${save}
    RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(status EQUAL 0)
    message("ok     ${label}")
  else()
    string(STRIP "${err}" err)
    message("FAILED ${label}: ${err}")
    math(EXPR count "${failures} + 1")
    set(failures ${count} PARENT_SCOPE)
  endif()
endfunction()

set(turns)
foreach(axis x y z)
  foreach(degrees 90 180 270)
    list(APPEND turns "${axis}${degrees}")
  endforeach()
endforeach()
list(APPEND turns xz yz)

function(step_of turn out)
  if(turn MATCHES "^([xyz])([0-9]+)$")
    set(${out} "{\"cmd\": \"reorient_design\", \"axis\": \"${CMAKE_MATCH_1}\", \"degrees\": ${CMAKE_MATCH_2}}" PARENT_SCOPE)
  else()
    set(${out} "{\"cmd\": \"reorient_design\", \"plane\": \"${turn}\"}" PARENT_SCOPE)
  endif()
endfunction()

if(bench)
  import("${bench}" bench-history --history)
  import("${bench}" bench-bodies)
  foreach(kind history bodies)
    foreach(t IN LISTS turns)
      step_of(${t} step)
      turn("${WORK}/bench-${kind}.mitcad" "c71542a1 ${kind} ${t}" "${step}")
    endforeach()
  endforeach()
else()
  message("skip   c71542a1: not under MITCAD_IAM_CORPUS or MITCAD_IPT_CORPUS")
endif()

if(drawer)
  import("${drawer}" drawer --history)
  step_of(x270 step)
  turn("${WORK}/drawer.mitcad" "1dc60c2a history x270" "${step}" "${WORK}/drawer-x270.mitcad")
  foreach(t z90 z270)
    step_of(${t} step)
    turn("${WORK}/drawer-x270.mitcad" "1dc60c2a history x270 then ${t}" "${step}")
  endforeach()
else()
  message("skip   1dc60c2a: not under MITCAD_IAM_CORPUS or MITCAD_IPT_CORPUS")
endif()

if(failures GREATER 0)
  message(FATAL_ERROR "reorient corpus: ${failures} turns refused")
endif()
