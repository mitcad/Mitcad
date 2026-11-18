# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Edits of an imported .f3d design recompute (mitcad#1904, GitHub #371) on
# a reported design (never committed): its archive in `github-371/` under
# MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits
# of its SHA-256), the design in it by its own, unpacked into WORK.
# Imported with mitcad-cli import-f3d: the cut that starts on a
# construction plane (timeline item 35) must come in parametric and
# verified against the file's history. Then mitcad-cli run with
# f3d_edit_reported.json edits the tower's height and two other
# dimensions: every feature after them recomputes and the tower's top
# moves; then Fillet1's rounding of 3 along the fin's foot and of 0.5
# along its top's edges, each added alone before the fillet, build
# (mitcad#1926). The whole of Fillet1 (timeline item 37) builds too, but
# its result is not the file's (a few cubic millimetres at the fin's arm
# ends and the tower slot's corners): the import lists it as a deviating
# result, and the test keeps it as the user would (--accept-deviations
# 37), so that Fillet1 comes in parametric and recomputes on the edited
# tower. Chamfer3 (timeline item 38) builds on Fillet1's result (OCCT port
# patch 1730) and comes in parametric and verified, so that the final
# body's tower height follows the edit. After the support's height edit
# (d49 = 6 mm) Fillet1 builds too, its legs' feet rounded on the result of
# their tops' rounding (OCCT port patch 1780, mitcad#1987), and Chamfer3
# after it. Skipped when the folder or the archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P f3d-edit-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "f3d-edit-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(archive_id "671857f7") # the reporter's archive
set(design_id "4852c1b2") # the design in it

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "f3d edit reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB archives LIST_DIRECTORIES false "${reported}/github-371/*.zip")
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
  message(STATUS "f3d edit reported: skipped (no archive ${archive_id} in ${reported}/github-371)")
  return()
endif()

# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/unpacked")
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/unpacked/*.f3d")
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

execute_process(COMMAND "${CLI}" import-f3d "${design}" --accept-deviations 37 --save "${WORK}/imported.mitcad"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "mitcad-cli import-f3d failed (${status}):\n${out}${err}")
endif()
string(REGEX MATCH "\n +35 [^\n]*" row "${out}")
string(STRIP "${row}" row)
if(NOT row MATCHES " parametric +-> F33 [^\n]*\\(verified\\)")
  message(FATAL_ERROR "the cut that starts on a construction plane did not come in parametric: ${row}")
endif()
message(STATUS "ok   ${row}")
# Fillet1 builds its rounding of all its sets (OCCT port patches up to
# 1729), kept parametric with its deviation from the file's result.
string(REGEX MATCH "\n +37 [^\n]*" row "${out}")
string(STRIP "${row}" row)
if(NOT row MATCHES " parametric +-> F35 [^\n]*kept parametric at the user's choice")
  message(FATAL_ERROR "Fillet1 did not come in parametric with its deviating result kept: ${row}")
endif()
message(STATUS "ok   ${row}")
# Chamfer3 builds on the kernel's Fillet1 result (OCCT port patch 1730).
string(REGEX MATCH "\n +38 [^\n]*" row "${out}")
string(STRIP "${row}" row)
if(NOT row MATCHES " parametric +-> F36 [^\n]*\\(verified\\)")
  message(FATAL_ERROR "Chamfer3 did not come in parametric and verified: ${row}")
endif()
message(STATUS "ok   ${row}")

execute_process(COMMAND "${CLI}" run "${SCRIPTS}/f3d_edit_reported.json" --open "${WORK}/imported.mitcad"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "f3d_edit_reported failed (${status}):\n${out}${err}")
endif()
message(STATUS "ok   the edits of the tower's height and two other dimensions recompute every feature after them, Fillet1 and Chamfer3 too (also after the support's height edit), and the final tower height follows")
message(STATUS "ok   the fillet's rounding of 3 along the fin's foot and of 0.5 along its top's edges build")
