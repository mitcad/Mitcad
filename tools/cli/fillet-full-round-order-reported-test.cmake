# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# GitHub #608's design (mitcad#2330; never committed): a rounding wider than
# the riser beside its edge (Fillet2, mitcad#2208) and a rounding of a
# corner of the bottom it touches (Fillet1, at 11 and as a Full Round) in
# both orders. The archive in `github-608/` under MITCAD_MITCAD_REPORTED,
# found by its content id (the first 8 hex digits of its SHA-256), the design
# in it by its own, unpacked into WORK and opened by mitcad-cli run with
# three scripts: fillet_full_round_order_reported_radius.json (the design's
# own order, Fillet1 at 11), _moved.json (Fillet1 at 11 moved before
# Fillet2) and _full_round.json (Fillet1 a Full Round, both orders, its
# exact volume). Each: every fillet ok without a note, one body, no sliver
# face, no plane facing up at the height of the step's bottom left (no lip
# of Fillet2's rounding standing out past Fillet1's, a step at its foot). The two orders at 11 report the same volume
# within 0.005 mm3 (the lip was 0.4). Skipped when the folder or the archive
# is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-full-round-order-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-full-round-order-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "fe46f952") # the reporter's archive
set(design_id "047bb4ee") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/github-608")
  message(STATUS "fillet full round order reported: skipped (MITCAD_MITCAD_REPORTED/github-608 is not there)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-608/*.zip")
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
  message(STATUS "fillet full round order reported: skipped (no archive ${archive_id} in ${reported}/github-608)")
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

# Runs a script on the design; the volume of Body1 (F2.b0) the report
# prints after it, in thousandths of a cubic millimetre, into `out_var`.
function(run_script script out_var)
  execute_process(COMMAND "${CLI}" run "${SCRIPTS}/${script}" --open "${design}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "${script} failed (${status}):\n${out}${err}")
  endif()
  if(NOT out MATCHES "\\(F2\\.b0\\): volume ([0-9]+)\\.([0-9][0-9][0-9]) mm3")
    message(FATAL_ERROR "${script}: no volume of F2.b0 in the report:\n${out}")
  endif()
  message(STATUS "     ${script}: volume ${CMAKE_MATCH_1}.${CMAKE_MATCH_2} mm3")
  set(${out_var} "${CMAKE_MATCH_1}${CMAKE_MATCH_2}" PARENT_SCOPE)
endfunction()

run_script(fillet_full_round_order_reported_radius.json own_order)
run_script(fillet_full_round_order_reported_moved.json moved)
run_script(fillet_full_round_order_reported_full_round.json full_round)

math(EXPR difference "${own_order} - ${moved}")
if(difference LESS -5 OR difference GREATER 5)
  message(FATAL_ERROR "Fillet1 at 11 gives another body before Fillet2 than after it: the volumes differ by "
                      "${difference} thousandths of a mm3")
endif()
message(STATUS "ok   Fillet1 at 11 and as a Full Round gives the same body before and after Fillet2, no step")
