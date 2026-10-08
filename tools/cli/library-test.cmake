# SPDX-License-Identifier: MIT
# Component libraries (mitcad#64, mitcad#63) through mitcad-cli: a small
# metric fastener library made by the generator (tools/libraries) in a
# folder of the build tree and recorded as v1.0.0, a head diameter changed
# for v1.1.0, the library fetched from its folder into a cache, shown and
# searched; parts inserted at v1.0.0 in sizes (linked and copied), the
# parts list, the comparison of the versions, the design reopened at its
# recorded version, then updated to v1.1.0 and to another size; the
# library's index entry. Nothing goes over the network.
#
# cmake -DCLI=<mitcad-cli> -DGIT=<git> -DPYTHON=<python3> -DGENERATOR=<make-fastener-library.py>
#       -DWORK=<folder> -P library-test.cmake

foreach(var CLI GIT PYTHON GENERATOR WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "library-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(library "${WORK}/mitcad-fasteners")
set(ENV{MITCAD_LIBRARIES_DIR} "${WORK}/cache")
set(ENV{GIT_CONFIG_NOSYSTEM} 1)
set(ENV{GIT_CONFIG_GLOBAL} "${WORK}/gitconfig")

function(cli expect)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(expect STREQUAL "ok" AND NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  elseif(expect STREQUAL "fail" AND NOT status EQUAL 1)
    message(FATAL_ERROR "mitcad-cli ${ARGN} should fail with 1 (${status}):\n${out}${err}")
  endif()
  set(out "${out}${err}" PARENT_SCOPE)
endfunction()

function(git)
  execute_process(COMMAND "${GIT}" -C "${library}" -c "user.name=Library Test" -c "user.email=test@example.invalid"
                          ${ARGN}
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err OUTPUT_STRIP_TRAILING_WHITESPACE)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "git ${ARGN} failed (${status}):\n${out}${err}")
  endif()
  set(out "${out}" PARENT_SCOPE)
endfunction()

function(expect pattern what)
  if(NOT out MATCHES "${pattern}")
    message(FATAL_ERROR "${what}:\n${out}")
  endif()
endfunction()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(WRITE "${WORK}/gitconfig" "")

# The library, v1.0.0: two sizes of three standards.
execute_process(COMMAND "${PYTHON}" "${GENERATOR}" --cli "${CLI}" --out "${library}" --sizes M5,M6
                        --standards iso4762,iso4032,iso7089 --version 1.0.0 --commit
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "the generator failed (${status}):\n${out}${err}")
endif()
expect("mitcad-fasteners 1.0.0: 3 components" "the generator's summary")
cli(ok library check "${library}")
expect("mitcad-fasteners: 3 components, no errors" "the library's check")
foreach(file mitcad-library.json LICENSE SOURCES.md README.md previews/iso4762.png .mitcad/project.json)
  if(NOT EXISTS "${library}/${file}")
    message(FATAL_ERROR "the generator wrote no ${file}")
  endif()
endforeach()
file(READ "${library}/mitcad-library.json" manifest)
string(JSON license GET "${manifest}" license)
if(NOT license STREQUAL "CC0-1.0")
  message(FATAL_ERROR "the library's licence: ${license}")
endif()

# v1.1.0: the M5 screws' heads 8.7 mm wide.
file(READ "${library}/screws/iso4762.mitcad" design)
string(REPLACE "\"dk\": \"8.5 mm\"" "\"dk\": \"8.7 mm\"" changed "${design}")
if(changed STREQUAL design)
  message(FATAL_ERROR "no M5 head diameter to change in iso4762.mitcad")
endif()
file(WRITE "${library}/screws/iso4762.mitcad" "${changed}")
git(commit -q -am "Wider M5 heads")
git(tag v1.1.0)

# Fetched from the folder, shown and searched.
cli(ok library fetch "${library}")
expect("Fetched library Mitcad metric fasteners \\(mitcad-fasteners\\)" "the fetch")
expect("Versions: v1.1.0 \\([0-9a-f]+\\), v1.0.0" "the versions after the fetch")
cli(ok library show mitcad-fasteners --rev v1.0.0)
expect("iso4762 Hexagon socket head cap screw \\(ISO 4762\\): screws/iso4762.mitcad, [0-9]+ sizes \\(default M5x16\\)"
       "the library's components")
cli(ok library search "din 912" --license CC0-1.0)
expect("mitcad-fasteners/iso4762" "the search for DIN 912")
cli(ok library search washer --license CC-BY-4.0)
if(out MATCHES "iso7089")
  message(FATAL_ERROR "the licence filter let a CC0 part through:\n${out}")
endif()
cli(ok library list --json)
expect("\"id\":\"mitcad-fasteners\"" "the list of libraries")

# Parts inserted at v1.0.0: a screw M5x16 and a washer M6 linked, a nut M5
# copied.
file(TO_CMAKE_PATH "${library}" url)
set(part "\"id\": \"mitcad-fasteners\", \"url\": \"${url}\", \"rev\": \"v1.0.0\"")
file(WRITE "${WORK}/insert.json" "[
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4762\", \"config\": \"M5x16\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4032\", \"config\": \"M5\"},
   \"link\": false, \"transform\": {\"translation\": [0, 0, -12]}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso7089\", \"config\": \"M6\"},
   \"transform\": {\"translation\": [30, 0, 0]}},
  {\"expect\": {\"body\": \"ISO 4762 M5x16\", \"volume\": 563.2433357349299, \"tolerance\": 1e-6}},
  {\"expect\": {\"bodies\": 3}}
]")
cli(ok run "${WORK}/insert.json" --save "${WORK}/assembly.mitcad")
cli(ok parts "${WORK}/assembly.mitcad")
expect("1  ISO 4762 M5x16  \\(mitcad-fasteners v1\\.0\\.0 \\([0-9a-f]+\\), CC0-1\\.0, linked\\)" "the parts list's screw")
expect("1  ISO 4032 M5  \\(mitcad-fasteners v1\\.0\\.0 \\([0-9a-f]+\\), CC0-1\\.0, copy\\)" "the parts list's nut")
expect("1  ISO 7089 M6  " "the parts list's washer")

# What v1.1.0 changes for the screw.
cli(ok library diff mitcad-fasteners v1.0.0 v1.1.0 --url "${url}" --part iso4762:M5x16 --part iso7089:M6)
expect("iso4762 M5x16: M5x16: dk 8\\.5 mm -> 8\\.7 mm" "the comparison of the versions")
expect("iso7089 M6: no changes" "the comparison of an unchanged part")

# Opened again, the screw stays at v1.0.0.
cli(ok info "${WORK}/assembly.mitcad")
expect("volume 563\\.243 mm3" "the screw at its recorded version")

# Updated to v1.1.0, then to another size.
file(WRITE "${WORK}/update.json" "[
  {\"cmd\": \"update_library_parts\", \"changes\": [{\"component\": \"ISO 4762 M5x16\", \"rev\": \"v1.1.0\"}]},
  {\"expect\": {\"body\": \"ISO 4762 M5x16\", \"volume\": 576.752184145366, \"tolerance\": 1e-6}},
  {\"cmd\": \"update_library_parts\", \"changes\": [{\"component\": \"ISO 4762 M5x16\", \"config\": \"M6x20\"}]},
  {\"expect\": {\"body\": \"ISO 4762 M6x20\", \"volume\": 971.7736704007988, \"tolerance\": 1e-6}}
]")
cli(ok run "${WORK}/update.json" --open "${WORK}/assembly.mitcad" --save "${WORK}/updated.mitcad")
cli(ok parts "${WORK}/updated.mitcad")
expect("1  ISO 4762 M6x20  \\(mitcad-fasteners v1\\.1\\.0" "the updated parts list")
# A copied part does not follow the library.
file(WRITE "${WORK}/copy.json" "[
  {\"cmd\": \"update_library_parts\", \"changes\": [{\"component\": \"ISO 4032 M5\", \"rev\": \"v1.1.0\"}]}
]")
cli(fail run "${WORK}/copy.json" --open "${WORK}/assembly.mitcad")
expect("is a copy of a library part" "an update of a copy")

# The entry a community index takes for the library.
cli(ok library index-entry "${library}" --url "https://example.org/mitcad-fasteners.git")
expect("\"id\": \"mitcad-fasteners\"" "the index entry")
expect("\"url\": \"https://example.org/mitcad-fasteners.git\"" "the index entry's URL")
