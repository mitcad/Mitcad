# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Annotations rolled back with the timeline (mitcad#1756) on a reported
# design (never committed): the project subset `sketch-mode-overlays/` under
# MITCAD_MITCAD_REPORTED, its design found by its content id (the first 8
# hex digits of its SHA-256), copied into WORK with the designs it refers
# to. Opened by mitcad-cli run with annotations_rolled_back_reported.json:
# with the marker right after the sketch the owner edited (as editing it
# puts the marker), its two material labels on bodies of later features
# are rolled back and no annotation is a warning; with the marker at the
# end again none is rolled back and none is a warning. Skipped when the
# folder or the design is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P annotations-rolled-back-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "annotations-rolled-back-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "4dcd969c") # the edited design

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}/sketch-mode-overlays")
  message(STATUS "annotations rolled back reported: skipped (no sketch-mode-overlays under MITCAD_MITCAD_REPORTED)")
  return()
endif()

# Copied into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(COPY "${reported}/sketch-mode-overlays" DESTINATION "${WORK}")
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/sketch-mode-overlays/*.mitcad")
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
  message(STATUS "annotations rolled back reported: skipped (no design ${design_id} in sketch-mode-overlays)")
  return()
endif()

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/annotations_rolled_back_reported.json" --open "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "annotations_rolled_back_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the material labels are rolled back with the edited sketch's later features, no warning; back at the end")
