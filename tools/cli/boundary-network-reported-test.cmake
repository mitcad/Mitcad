# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Boundary Surface's Network whose curves run on past the outer crossing
# curves (mitcad#2333, GitHub #609), on the reported network rebuilt as a
# design (never committed): stored as it is in `github-609/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits of
# its SHA-256) and copied into WORK. Opened by mitcad-cli run with
# boundary_network_reported.json: the Network builds one face through all
# five curves, between the outer crossings. Skipped when the folder or the
# design is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P boundary-network-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "boundary-network-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "7f05aa35") # the rebuilt design

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "boundary network reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB stored LIST_DIRECTORIES false "${reported}/github-609/*.mitcad")
set(found "")
foreach(candidate IN LISTS stored)
  file(SHA256 "${candidate}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL design_id)
    set(found "${candidate}")
    break()
  endif()
endforeach()
if(found STREQUAL "")
  message(STATUS "boundary network reported: skipped (no design ${design_id} in ${reported}/github-609)")
  return()
endif()

# Copied into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
get_filename_component(name "${found}" NAME)
file(COPY_FILE "${found}" "${WORK}/${name}")

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/boundary_network_reported.json" --open "${WORK}/${name}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "boundary_network_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the reported network builds one face between its outer crossings")
