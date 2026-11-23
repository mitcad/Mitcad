# SPDX-License-Identifier: MIT
# mitcad-cli export-script (mitcad#2245): a script's design written as a
# script file, which `run` replays with its expectations; printed without a
# file; stopped at a feature with --upto; usage errors with 2.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPT=<script.json> -DUPTO=<feature name> -DWORK=<folder> -P export-script-test.cmake

foreach(var CLI SCRIPT UPTO WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "export-script-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(MAKE_DIRECTORY "${WORK}")
set(out "${WORK}/exported.json")

function(cli expect)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE text ERROR_VARIABLE err)
  if(expect STREQUAL "ok" AND NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${text}${err}")
  elseif(expect STREQUAL "usage" AND NOT status EQUAL 2)
    message(FATAL_ERROR "mitcad-cli ${ARGN} should be a usage error (${status}):\n${text}${err}")
  endif()
  set(text "${text}${err}" PARENT_SCOPE)
endfunction()

cli(ok export-script "${SCRIPT}" "${out}")
if(NOT text MATCHES "^Exported [0-9]+ features \\([0-9]+ steps\\) to ")
  message(FATAL_ERROR "export-script did not say what it wrote:\n${text}")
endif()
file(READ "${out}" script)
foreach(wanted [[{"cmd":"add_feature"]] [[{"expect":{"bodies":]] [["uid":"F1"]])
  string(FIND "${script}" "${wanted}" at)
  if(at EQUAL -1)
    message(FATAL_ERROR "the exported script has no ${wanted}:\n${script}")
  endif()
endforeach()
# The exported script replays on an empty design, its expectations hold.
cli(ok run "${out}")

# Printed without a file.
cli(ok export-script "${SCRIPT}")
if(NOT text MATCHES "^\\[\n  {\"comment\":")
  message(FATAL_ERROR "export-script without a file should print the script:\n${text}")
endif()

# Stopped at a feature: it is the last one the script adds, and the script
# still replays.
cli(ok export-script "${SCRIPT}" "${WORK}/upto.json" --upto "${UPTO}")
file(READ "${WORK}/upto.json" upto)
string(FIND "${upto}" "\"name\":\"${UPTO}\"" at)
if(at EQUAL -1)
  message(FATAL_ERROR "the script --upto ${UPTO} lacks ${UPTO}:\n${upto}")
endif()
cli(ok run "${WORK}/upto.json")

cli(usage export-script)
cli(usage export-script "${SCRIPT}" "${out}" extra)
cli(usage export-script "${SCRIPT}" --check --upto marker)
