# SPDX-License-Identifier: MIT
# mitcad-cli's exit after an .f3d import whose geometry kernel hung
# (mitcad#82): the threads the hang watchdog gave up still run when the
# program ends, and the shared libraries' static destructors must not run
# under them (they crashed in OCCT's shape healing: status 139 after the
# error). With MITCAD_IMPORT_STALL=busy every try keeps the kernel busy
# building the file's bodies without progress, so the import ends with the
# error that it hung: status 1. With MITCAD_IMPORT_STALL=compare only the
# first try hangs, comparing the final bodies, and the next one finishes:
# status 0 while the first still runs. A crash shows as a status that is
# not a number (CMake gives the signal's name).
#
# cmake -DCLI=<mitcad-cli> -DFILE=<bodies.f3d> -P hang-exit-test.cmake

foreach(var CLI FILE)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "hang-exit-test.cmake needs -D${var}=...")
  endif()
endforeach()

# Runs the import with the stall `stall`; `status` is the exit status
# expected, `pattern` what its output has.
function(import stall status pattern)
  set(ENV{MITCAD_IMPORT_STALL} "${stall}")
  execute_process(COMMAND "${CLI}" import-f3d "${FILE}" --hang-limit 0.5
                  RESULT_VARIABLE result OUTPUT_VARIABLE out ERROR_VARIABLE err)
  unset(ENV{MITCAD_IMPORT_STALL})
  if(NOT "${result}" STREQUAL "${status}")
    message(FATAL_ERROR "import-f3d with MITCAD_IMPORT_STALL=${stall}: exit status '${result}', "
                        "${status} expected:\n${out}${err}")
  endif()
  if(NOT "${out}${err}" MATCHES "${pattern}")
    message(FATAL_ERROR "import-f3d with MITCAD_IMPORT_STALL=${stall}: no '${pattern}' in:\n${out}${err}")
  endif()
endfunction()

# Several times: the crash depended on what the given-up threads did when
# the program ended.
foreach(round RANGE 1 3)
  import(busy 1 "the import of .*bodies\\.f3d hung in the geometry kernel")
  import(compare 0 "the geometry kernel did not return for 0\\.5 s on comparing the final bodies")
endforeach()
