# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# Shells with Sharp corners on reporters' designs (never committed): GitHub
# #642 (mitcad#2383), a revolved cone on a ring with a side boss and five
# notches cut up from the ring's top through the cone's wall, shelled 2 mm
# inside with Sharp corners and Tangent Chain on, the boss top and the foot
# removed, as the report's picture has it; before, the shell failed ("the
# offset faces do not close up") at any thickness (OCCT port patch 2383).
# The same report's second design (mitcad#2415): a disk with an arm whose
# flat sides are tangent to it, the disk's seam on one tangent line, a boss
# on the arm's end, shelled 2, 1 and 5 mm inside with Sharp corners and the
# boss top removed; it failed the same way (OCCT port patch 2415). GitHub
# #645 (mitcad#2415): a loft cut to a slot, its top rounded, shelled closed
# 2 mm inside, which failed at any thickness (patch 2415), and the report's
# shell with the top removed, 2 and 1 mm (mitcad#2462, patch 2462). GitHub
# #642's second request (mitcad#2443): the first design shelled the same way
# with the step's top at a wall of its own, 3 mm.
# Each report's archive is found in its folder under MITCAD_MITCAD_REPORTED
# by its content id (the first 8 hex digits of the SHA-256), the design in it
# by its own, unpacked into WORK so that nothing is written beside the
# reported files, and opened by mitcad-cli run with its script
# (shell_sharp_reported*.json): the Shell builds into one solid with walls of
# the thickness given. Skipped when the folder or every archive is not there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P shell-sharp-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "shell-sharp-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

# Each case: the report's folder, the reporter's archive, the design in it
# (content ids) and the script that opens it.
set(cases
  "github-642:40910cf1:980e559a:shell_sharp_reported642.json"   # 2 mm inside, Sharp, top and foot open
  "github-642:1bd3c43c:907e7160:shell_sharp_reported642b.json"  # 2, 1 and 5 mm inside, Sharp, boss top open
  "github-645:4b355bd0:18e67ffd:shell_sharp_reported645.json"   # closed 2 mm; top open 2 and 1 mm, Sharp and Rounded
  "github-642:40910cf1:980e559a:shell_own_thickness_reported642.json")  # the step's top at 3 mm (mitcad#2443)

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "shell sharp reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(ran 0)
foreach(case IN LISTS cases)
  string(REPLACE ":" ";" fields "${case}")
  list(GET fields 0 folder)
  list(GET fields 1 archive_id)
  list(GET fields 2 design_id)
  list(GET fields 3 script)
  file(GLOB archives LIST_DIRECTORIES false "${reported}/${folder}/*.zip")
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
    message(STATUS "shell sharp reported: no archive ${archive_id} in ${reported}/${folder}")
    continue()
  endif()
  if(NOT IS_DIRECTORY "${WORK}/${archive_id}")
    file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${WORK}/${archive_id}")
  endif()
  file(GLOB_RECURSE designs LIST_DIRECTORIES false "${WORK}/${archive_id}/*.mitcad")
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
  execute_process(COMMAND "${CLI}" run "${SCRIPTS}/${script}" --open "${design}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "${script} failed (${status}):\n${out}${err}")
  endif()
  message(STATUS "ok   ${script}")
  math(EXPR ran "${ran} + 1")
endforeach()
if(ran EQUAL 0)
  message(STATUS "shell sharp reported: skipped (no reported archive found)")
endif()
