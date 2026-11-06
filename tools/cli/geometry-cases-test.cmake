# SPDX-License-Identifier: MIT
# Designs of geometry cases reported with a false warning (never
# committed; MITCAD_GEOMETRY_CASES, a folder of cases each in a folder of
# its own): each is opened and recomputed with mitcad-cli info, and the
# features listed must build without a warning (mitcad#1327). Skipped when
# none of the designs is there.
#
# cmake -DCLI=<mitcad-cli> -P geometry-cases-test.cmake

if(NOT DEFINED CLI)
  message(FATAL_ERROR "geometry-cases-test.cmake needs -DCLI=...")
endif()

# "<case folder>/<design> <feature>" each.
set(expected
  # mitcad#1327: a fillet of a tangent chain of concave edges (R3) whose
  # corner meets two earlier roundings (R5, R1) differs from the faces
  # beside it by 0.057 mm, which no one can see; and a rounding of a hole's
  # rim built as a ring that runs 0.65 mm onto a neighbouring rounded face
  # and is smooth there. Neither warns.
  "fillet-corner-overhang/fillet-corner.mitcad Fillet3"
  "fillet-corner-overhang/fillet-corner-ring.mitcad Fillet4"
)

set(cases "$ENV{MITCAD_GEOMETRY_CASES}")
set(failed 0)
set(ran 0)
foreach(line IN LISTS expected)
  if(NOT line MATCHES "^([^ ]+) ([^ ]+)$")
    message(FATAL_ERROR "bad line: ${line}")
  endif()
  set(design "${cases}/${CMAKE_MATCH_1}")
  set(feature "${CMAKE_MATCH_2}")
  if(cases STREQUAL "" OR NOT EXISTS "${design}")
    continue()
  endif()
  string(MAKE_C_IDENTIFIER "${design}" key)
  if(NOT DEFINED report_${key})
    get_filename_component(dir "${design}" DIRECTORY)
    execute_process(COMMAND "${CLI}" info "${design}"
      WORKING_DIRECTORY "${dir}"
      RESULT_VARIABLE code OUTPUT_VARIABLE out ERROR_VARIABLE err)
    if(NOT code EQUAL 0)
      message("FAIL ${design}: mitcad-cli info exited with ${code}\n${err}")
      math(EXPR failed "${failed} + 1")
      set(out "")
    endif()
    set(report_${key} "${out}")
  endif()
  math(EXPR ran "${ran} + 1")
  set(report "${report_${key}}")
  string(REGEX MATCH "[^\n]* ${feature} \\([a-z_]+\\): [^\n]*" row "${report}")
  if(row STREQUAL "")
    message("FAIL ${line}: no such feature in the timeline")
    math(EXPR failed "${failed} + 1")
  elseif(NOT row MATCHES "\\): ok$")
    message("FAIL ${line}: ${row}")
    math(EXPR failed "${failed} + 1")
  else()
    message("ok   ${line}")
  endif()
endforeach()
if(ran EQUAL 0)
  message("geometry cases: skipped (MITCAD_GEOMETRY_CASES has none of the designs)")
  return()
endif()
if(failed GREATER 0)
  message(FATAL_ERROR "geometry cases: ${failed} check(s) failed")
endif()
