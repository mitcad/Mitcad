# SPDX-License-Identifier: MIT
# A design whose BUILD connectors the first placing rule placed, when
# placing them again by the current rule is refused (mitcad#1151): the
# frame of build_corners.json saved, its records without their rule (as
# the first rule's design files are), opened with mitcad-cli info and the
# test hook MITCAD_TEST_BUILD_UPGRADE_REFUSED that refuses the upgrade of
# such a design. The design still opens: the report is printed, the exit
# status is 0, and a warning names the refusal.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P build-upgrade-refused-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "build-upgrade-refused-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")

# The frame with its four connectors, placed by the current rule.
set(current "${WORK}/corners.mitcad")
execute_process(COMMAND "${CLI}" run "${SCRIPTS}/build_corners.json" --open "${SCRIPTS}/build_parts.mitcad"
                        --save "${current}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "building the frame failed (${status}):\n${out}${err}")
endif()

# As the first rule saved it: no "rule" in the BUILD records (the last
# field of a record, after a comma).
file(READ "${current}" text)
string(REGEX MATCHALL "\"rule\": [0-9]+" rules "${text}")
list(LENGTH rules count)
if(count EQUAL 0)
  message(FATAL_ERROR "the saved frame has no BUILD record with a rule:\n${text}")
endif()
string(REGEX REPLACE ",[ \t\r\n]*\"rule\": [0-9]+" "" text "${text}")
if(text MATCHES "\"rule\": [0-9]")
  message(FATAL_ERROR "a rule is left in the records:\n${text}")
endif()
set(first "${WORK}/first-rule.mitcad")
file(WRITE "${first}" "${text}")

# Opened with the upgrade refused: the design as loaded, with the warning.
execute_process(COMMAND "${CMAKE_COMMAND}" -E env MITCAD_TEST_BUILD_UPGRADE_REFUSED=1 "${CLI}" info "${first}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "opening the design failed (${status}):\n${out}${err}")
endif()
if(NOT err MATCHES "warning: connectors of the first rule were not placed again: [^\n]*MITCAD_TEST_BUILD_UPGRADE_REFUSED")
  message(FATAL_ERROR "no warning about the refused upgrade:\n${err}")
endif()
if(NOT out MATCHES "Timeline \\(marker at the end\\):.*Components: ")
  message(FATAL_ERROR "no report of the design:\n${out}")
endif()
message(STATUS "${count} BUILD records of the first rule; opened with the warning:\n${err}")
