# SPDX-License-Identifier: MIT
# The reference sheet-metal hems (mitcad#1364, mitcad#1378): six designs of
# MITCAD_F3D_MODELS (never committed), each a base flange and one hem of a
# form, found by their content ids (the first 8 hex digits of the file's
# SHA-256). Each is imported with mitcad-cli import-f3d: the hem (the third
# item, `#2`) must come in parametric and verified against the file's
# history, and no item may fall back. Skipped when none of the designs is
# there.
#
# cmake -DCLI=<mitcad-cli> -P f3d-hems-test.cmake

if(NOT DEFINED CLI)
  message(FATAL_ERROR "f3d-hems-test.cmake needs -DCLI=...")
endif()

# "<content id> <form>" each.
set(expected
  "bb26e584 open"
  "aad6719d flat"
  "8fa42a30 rolled"
  "4cf060ff teardrop"
  "c2575479 rope"
  "08887779 double"
)

set(models "$ENV{MITCAD_F3D_MODELS}")
set(files "")
if(NOT models STREQUAL "" AND IS_DIRECTORY "${models}")
  file(GLOB_RECURSE files LIST_DIRECTORIES false "${models}/*.f3d")
endif()
foreach(file IN LISTS files)
  file(SHA256 "${file}" sha)
  string(SUBSTRING "${sha}" 0 8 id)
  set(path_${id} "${file}")
endforeach()

set(failed 0)
set(ran 0)
foreach(line IN LISTS expected)
  if(NOT line MATCHES "^([0-9a-f]+) ([a-z]+)$")
    message(FATAL_ERROR "bad line: ${line}")
  endif()
  set(id "${CMAKE_MATCH_1}")
  set(form "${CMAKE_MATCH_2}")
  if(NOT DEFINED path_${id})
    continue()
  endif()
  math(EXPR ran "${ran} + 1")
  execute_process(COMMAND "${CLI}" import-f3d "${path_${id}}"
    RESULT_VARIABLE code OUTPUT_VARIABLE out ERROR_VARIABLE err)
  string(REGEX MATCH "\n +2 [^\n]*" row "${out}")
  string(STRIP "${row}" row)
  if(NOT code EQUAL 0)
    message("FAIL ${id} (${form} hem): mitcad-cli import-f3d exited with ${code}\n${err}")
    math(EXPR failed "${failed} + 1")
  elseif(NOT row MATCHES " parametric +-> [^\n]*\\(verified\\)")
    message("FAIL ${id} (${form} hem): ${row}")
    math(EXPR failed "${failed} + 1")
  elseif(NOT out MATCHES "timeline items: [0-9]+ parametric, 0 partial, 0 fallback")
    string(REGEX MATCH "[^\n]*timeline items[^\n]*" summary "${out}")
    message("FAIL ${id} (${form} hem): ${summary}")
    math(EXPR failed "${failed} + 1")
  else()
    message("ok   ${id} (${form} hem): ${row}")
  endif()
endforeach()
if(ran EQUAL 0)
  message("reference hems: skipped (MITCAD_F3D_MODELS has none of the designs)")
  return()
endif()
if(failed GREATER 0)
  message(FATAL_ERROR "reference hems: ${failed} of ${ran} failed")
endif()
