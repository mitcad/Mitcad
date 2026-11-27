# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# GitHub #686's drawing of a filleted tube (mitcad#2447; never committed): the
# reporter's archive in `github-686/` under MITCAD_MITCAD_REPORTED, found by
# its content id, the design in it by its own (a circle swept along a line
# and an arc, mirrored and joined into a Y of two bent tubes, the crotch
# where their tori meet rounded by a free-form fillet, shelled), unpacked
# into WORK. Its drawing Drawing1 (orthographic views and an isometric one,
# with Hidden Edges and Tangent Edges as the reporter left them) is
# written as SVG by export-drawing with the drawing trace on (one
# `drawing-trace:` line per view: the algorithm each view took), its bodies
# as a .brep file, whose edges the drawing test (test_geometry_surfaces
# drawing --edges) lists (the faces' surfaces, the continuity stored on
# each edge and found) and which it projects in seven views
# (test_geometry_surfaces drawing --tangent-edges): with Tangent Edges
# off no line along a tangent edge, seen or hidden; with it on none along a
# seam (the tube's seams, which its sweep stores as smooth edges, were
# drawn as tangent edges) and every other edge drawn over its whole length;
# exact, on the quick path as the sheet's 1:1 views take it, and polygonal.
# Before the fix half of the fillet's boundary (the halves on which the
# fillet stored no G1, which OCCT's own test took for edges at an angle)
# was drawn as a sharp edge in every view. Skipped when the folder or the
# archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DTEST=<test_geometry_surfaces> -DTEST_NAME=drawing -DWORK=<folder>
#   -P drawing-tube-reported-test.cmake

foreach(var CLI TEST TEST_NAME WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "drawing-tube-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "da176d16") # the reporter's archive
set(design_id "ddf7f403") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-686")
  message(STATUS "drawing tube reported: skipped (MITCAD_MITCAD_REPORTED/github-686 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-686/*.zip")
set(archive "")
foreach(candidate IN LISTS archives)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL archive_id)
    set(archive "${candidate}")
    break()
  endif()
endforeach()
if(archive STREQUAL "")
  message(STATUS "drawing tube reported: skipped (no archive ${archive_id} in ${reported}/github-686)")
  return()
endif()

# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/unpacked")
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/unpacked/*.mitcad")
set(design "")
foreach(candidate IN LISTS designs)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL design_id)
    set(design "${candidate}")
    break()
  endif()
endforeach()
if(design STREQUAL "")
  message(FATAL_ERROR "the archive ${archive_id} holds no design ${design_id}")
endif()

function(run)
  execute_process(COMMAND ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "${ARGN} failed (${status}):\n${out}${err}")
  endif()
  set(output "${out}" PARENT_SCOPE)
  set(errors "${err}" PARENT_SCOPE)
endfunction()

set(ENV{MITCAD_DRAWING_TRACE} "1")
run("${CLI}" export-drawing "${design}" "${WORK}/reported.svg" --drawing Drawing1)
unset(ENV{MITCAD_DRAWING_TRACE})
string(REGEX MATCHALL "drawing-trace: view[^\n]*" traces "${errors}")
foreach(trace IN LISTS traces)
  message(STATUS "${trace}")
endforeach()
file(READ "${WORK}/reported.svg" text)
string(REGEX MATCHALL "<g class=\"view\"" views "${text}")
list(LENGTH views n)
if(n EQUAL 0)
  message(FATAL_ERROR "no views in the SVG")
endif()
message(STATUS "ok   drawing Drawing1 written: ${n} views")

run("${CLI}" export "${design}" "${WORK}/reported.brep")
run("${TEST}" "${TEST_NAME}" --edges "${WORK}/reported.brep")
message(STATUS "${output}")
run("${TEST}" "${TEST_NAME}" --tangent-edges "${WORK}/reported.brep")
message(STATUS "${output}")
message(STATUS "ok   tangent edges, seams and hidden edges drawn as the view's settings ask")
