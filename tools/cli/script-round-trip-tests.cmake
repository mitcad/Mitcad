# SPDX-License-Identifier: MIT
# Every CLI script's design exported as a script and replayed (mitcad#2245):
# cli.script_round_trip.<script> runs `mitcad-cli export-script <script>
# --check`, which builds the design with the script, exports its timeline
# (export_script), replays the export on a new document and compares the
# two: the features (uids, names, types, suppression, status), the
# parameters, the bodies (uids, names, volumes within 1e-6 relative) and
# every body's face names. A design that cannot be replayed by itself (the
# export leaves imported bodies out or refuses the design in words, or the
# script needs a file or an option its own test gives it) is skipped with
# exit status 5, and the output says why.
#
# Included from tools/cli/CMakeLists.txt inside its BUILD_TESTING block.

# The command itself: a file written and replayed, the script printed,
# --upto, usage errors.
add_test(NAME cli.export_script
  COMMAND "${CMAKE_COMMAND}" "-DCLI=$<TARGET_FILE:mitcad-cli>"
          "-DSCRIPT=${CMAKE_CURRENT_SOURCE_DIR}/tests/block_fillet_chamfer.json" "-DUPTO=Fillet1"
          "-DWORK=${CMAKE_CURRENT_BINARY_DIR}/export-script" -P "${CMAKE_CURRENT_SOURCE_DIR}/export-script-test.cmake")
set_tests_properties(cli.export_script PROPERTIES TIMEOUT 300)

file(GLOB round_trip_scripts CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/tests/*.json")
foreach(path IN LISTS round_trip_scripts)
  get_filename_component(script "${path}" NAME_WE)
  add_test(NAME cli.script_round_trip.${script} COMMAND mitcad-cli export-script "${path}" --check)
  set_tests_properties(cli.script_round_trip.${script} PROPERTIES
    SKIP_RETURN_CODE 5 TIMEOUT 900 LABELS script_round_trip)
endforeach()
