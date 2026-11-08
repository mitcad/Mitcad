# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A drawing's views leave out the tangent edges of fillets while their
# Tangent Edges is off (mitcad#1438), on the reported design of GitHub #177
# (a bearing block with fillets; never committed): `github-177-*.mitcad`
# under MITCAD_MITCAD_REPORTED. Its drawing D1 (four views, none with
# tangent edges) is written as SVG by export-drawing, and its body as a
# .brep file, which test_drawing --tangent-edges projects in seven views:
# with Tangent Edges off no line along an edge between faces that meet
# tangentially (or along a seam), exact and polygonal; with it on, those
# lines thin; every other sharp line that OCCT's hidden-line removal
# draws of the body as it is still drawn; the body unchanged. Before the
# fix, 25 lines along tangent edges and one along a seam were drawn as
# sharp edges in those views. Skipped when the folder or the design is not
# there.
#
# cmake -DCLI=<mitcad-cli> -DTEST=<test_drawing> -DWORK=<folder> -P drawing-tangent-edges-test.cmake

foreach(var CLI TEST WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "drawing-tangent-edges-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "drawing tangent edges: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB designs LIST_DIRECTORIES false "${reported}/github-177-*.mitcad")
list(LENGTH designs n)
if(n EQUAL 0)
  message(STATUS "drawing tangent edges: skipped (no github-177 design in ${reported})")
  return()
endif()
list(GET designs 0 source)

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(design "${WORK}/reported.mitcad")
file(COPY_FILE "${source}" "${design}")

function(run)
  execute_process(COMMAND ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "${ARGN} failed (${status}):\n${out}${err}")
  endif()
  set(output "${out}" PARENT_SCOPE)
endfunction()

run("${CLI}" export-drawing "${design}" "${WORK}/reported.svg" --drawing D1)
file(READ "${WORK}/reported.svg" text)
string(REGEX MATCHALL "<g class=\"view\"" views "${text}")
list(LENGTH views n)
if(NOT n EQUAL 4)
  message(FATAL_ERROR "views in the SVG: ${n}, expected 4")
endif()
message(STATUS "ok   drawing D1 written: 4 views")

run("${CLI}" export "${design}" "${WORK}/reported.brep")
run("${TEST}" --tangent-edges "${WORK}/reported.brep")
message(STATUS "${output}")
message(STATUS "ok   no tangent edges drawn with Tangent Edges off")
