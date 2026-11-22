# SPDX-License-Identifier: MIT
# mitcad-cli export to .glb and .gltf (mitcad#2210): the design of
# tests/gltf_export.json (Plate with its Pin placed three times: two
# bodies, six placements) saved, then written to a .glb and a .gltf file.
# Each run prints its counts; test_gltf_export checks each file (one mesh
# per body shared by its placements, a node per placement with its matrix,
# the component tree, a .gltf file's .bin beside it) and compares it with
# the file File > Export's writer makes of the design. Options of other
# formats are refused with their message and write no file; --bodies,
# --refinement and --json as for the other formats.
#
# cmake -DCLI=<mitcad-cli> -DTEST=<test_gltf_export> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder>
#       -P gltf-export-test.cmake

foreach(var CLI TEST SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "gltf-export-test.cmake needs -D${var}=...")
  endif()
endforeach()
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}/cli" "${WORK}/app")

set(design "${WORK}/placed.mitcad")
execute_process(COMMAND "${CLI}" run "${SCRIPTS}/gltf_export.json" --save "${design}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "the design failed (${status}):\n${out}${err}")
endif()

foreach(format glb gltf)
  set(file "${WORK}/cli/placed.${format}")
  execute_process(COMMAND "${CLI}" export "${design}" "${file}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "export to .${format} failed (${status}):\n${out}${err}")
  endif()
  # The counts of the app's log: 6 body nodes, the triangles, the files
  # (a .gltf file's .bin beside it) and their bytes.
  if(format STREQUAL "glb")
    set(files "\\[\"[^\"]*placed\\.glb\"\\]")
  else()
    set(files "\\[\"[^\"]*placed\\.gltf\",\"[^\"]*placed\\.bin\"\\]")
  endif()
  if(NOT out MATCHES "Exported: {\"bodies\":6,\"bytes\":[1-9][0-9]*,\"files\":${files},\"format\":\"${format}\",\"path\":\"[^\"]*placed\\.${format}\",\"triangles\":[1-9][0-9]*,\"warnings\":\\[\\]}")
    message(FATAL_ERROR "export to .${format}: not the line with its counts:\n${out}${err}")
  endif()
  if(NOT EXISTS "${file}")
    message(FATAL_ERROR "export to .${format} wrote no ${file}")
  endif()
  execute_process(COMMAND "${TEST}" "${design}" "${file}" "${WORK}/app"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "the .${format} file is not as it should be (${status}):\n${out}${err}")
  endif()
  message(STATUS "${out}")
endforeach()
if(NOT EXISTS "${WORK}/cli/placed.bin")
  message(FATAL_ERROR "the .gltf file's buffer is not beside it")
endif()

# One body by name, every placement of it, as JSON.
execute_process(COMMAND "${CLI}" export "${design}" "${WORK}/cli/pins.glb" --bodies Body2 --refinement low --json
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0 OR NOT out MATCHES "(^|\n){\"bodies\":3,[^\n]*\"format\":\"glb\"" OR out MATCHES "Exported:")
  message(FATAL_ERROR "export of Body2 as JSON (${status}):\n${out}${err}")
endif()
execute_process(COMMAND "${CLI}" export "${design}" "${WORK}/cli/none.glb" --bodies NoSuchBody
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(status EQUAL 0 OR NOT err MATCHES "no body at the timeline marker is named NoSuchBody" OR EXISTS "${WORK}/cli/none.glb")
  message(FATAL_ERROR "an unknown body was not refused (${status}):\n${out}${err}")
endif()

# The options of other formats: refused with their message, exit status 2,
# no file.
foreach(option "--unit=in" "--schema=ap214" "--ascii" "--coordinates=component" "--occurrence=Plate:1")
  string(REPLACE "=" ";" arguments "${option}")
  list(GET arguments 0 name)
  set(refused "${WORK}/refused.gltf")
  execute_process(COMMAND "${CLI}" export "${design}" "${refused}" ${arguments}
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 2 OR NOT err MATCHES "mitcad-cli: ${name} does not apply to glTF files")
    message(FATAL_ERROR "${name} was not refused (${status}):\n${out}${err}")
  endif()
  if(EXISTS "${refused}" OR EXISTS "${WORK}/refused.bin")
    message(FATAL_ERROR "${name} was refused but a file was written")
  endif()
endforeach()
message(STATUS "glb and gltf as the application writes them; the other formats' options refused")
