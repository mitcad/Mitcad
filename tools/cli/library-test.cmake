# SPDX-License-Identifier: MIT
# Component libraries (mitcad#64, mitcad#63) through mitcad-cli: the
# fasteners of a small parts library (mitcad-parts) made by the generator
# (tools/libraries) in a folder of the build tree and recorded as v1.0.0, a
# head diameter changed for v1.1.0, the library fetched from its folder
# into a cache, shown and searched; parts inserted at v1.0.0 in sizes (linked and copied), the
# parts list, the comparison of the versions, the design reopened at its
# recorded version, then updated to v1.1.0 and to another size; the
# library's index entry; then the library of every size, M1.6 to M24
# (mitcad#900), its smallest and largest rows inserted. Nothing goes over
# the network.
#
# cmake -DCLI=<mitcad-cli> -DGIT=<git> -DPYTHON=<python3> -DGENERATOR=<make-fastener-library.py>
#       -DWORK=<folder> -P library-test.cmake

foreach(var CLI GIT PYTHON GENERATOR WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "library-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(library "${WORK}/mitcad-parts")
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
expect("mitcad-parts 1.0.0: 3 components" "the generator's summary")
cli(ok library check "${library}")
expect("mitcad-parts: 3 components, no errors" "the library's check")
foreach(file mitcad-library.json LICENSE SOURCES.md README.md previews/iso4762.png .mitcad/project.json)
  if(NOT EXISTS "${library}/${file}")
    message(FATAL_ERROR "the generator wrote no ${file}")
  endif()
endforeach()
file(READ "${library}/mitcad-library.json" manifest)
string(JSON license GET "${manifest}" license)
if(NOT license STREQUAL "MIT")
  message(FATAL_ERROR "the library's licence: ${license}")
endif()
# The fasteners keep their own licence.
string(JSON license GET "${manifest}" components 0 license)
if(NOT license STREQUAL "CC0-1.0")
  message(FATAL_ERROR "the fasteners' licence: ${license}")
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
expect("Fetched library Mitcad parts \\(mitcad-parts\\)" "the fetch")
expect("Versions: v1.1.0 \\([0-9a-f]+\\), v1.0.0" "the versions after the fetch")
cli(ok library show mitcad-parts --rev v1.0.0)
expect("iso4762 Hexagon socket head cap screw \\(ISO 4762 / DIN 912\\): screws/iso4762.mitcad, [0-9]+ sizes \\(default M5x16\\)"
       "the library's components")
cli(ok library search "din 912" --license CC0-1.0)
expect("mitcad-parts/iso4762" "the search for DIN 912")
cli(ok library search washer --license CC-BY-4.0)
if(out MATCHES "iso7089")
  message(FATAL_ERROR "the licence filter let a CC0 part through:\n${out}")
endif()
cli(ok library list --json)
expect("\"id\":\"mitcad-parts\"" "the list of libraries")

# Parts inserted at v1.0.0: a screw M5x16 and a washer M6 linked, a nut M5
# copied. The screws' volumes are those of the head, shank and socket less
# the head's top edge rounded (radius 0.06 dk: 2 pi (dk/2 - 0.2234 r)
# r^2 (1 - pi/4)) and the thread's tip chamfer (45 degrees from the
# diameter d - 1.182532 P: the ring a cone takes off the shank's end):
# M5x16 563.2433 - 1.4506 - 1.6464; at v1.1.0 (dk 8.7 mm) 576.7522 -
# 1.5554 - 1.6464; M6x20 971.7737 - 2.3620 - 3.0784 (mitcad#526).
file(TO_CMAKE_PATH "${library}" url)
set(part "\"id\": \"mitcad-parts\", \"url\": \"${url}\", \"rev\": \"v1.0.0\"")
file(WRITE "${WORK}/insert.json" "[
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4762\", \"config\": \"M5x16\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4032\", \"config\": \"M5\"},
   \"link\": false, \"transform\": {\"translation\": [0, 0, -12]}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso7089\", \"config\": \"M6\"},
   \"transform\": {\"translation\": [30, 0, 0]}},
  {\"expect\": {\"body\": \"ISO 4762 M5x16\", \"volume\": 560.1463214427537, \"tolerance\": 1e-6}},
  {\"expect\": {\"bodies\": 3}}
]")
cli(ok run "${WORK}/insert.json" --save "${WORK}/assembly.mitcad")
cli(ok parts "${WORK}/assembly.mitcad")
expect("1  ISO 4762 M5x16  \\(mitcad-parts v1\\.0\\.0 \\([0-9a-f]+\\), CC0-1\\.0, linked\\)" "the parts list's screw")
expect("1  ISO 4032 M5  \\(mitcad-parts v1\\.0\\.0 \\([0-9a-f]+\\), CC0-1\\.0, copy\\)" "the parts list's nut")
expect("1  ISO 7089 M6  " "the parts list's washer")

# What v1.1.0 changes for the screw.
cli(ok library diff mitcad-parts v1.0.0 v1.1.0 --url "${url}" --part iso4762:M5x16 --part iso7089:M6)
expect("iso4762 M5x16: M5x16: dk 8\\.5 mm -> 8\\.7 mm" "the comparison of the versions")
expect("iso7089 M6: no changes" "the comparison of an unchanged part")

# Opened again, the screw stays at v1.0.0.
cli(ok info "${WORK}/assembly.mitcad")
expect("volume 560\\.146 mm3" "the screw at its recorded version")

# Updated to v1.1.0, then to another size.
file(WRITE "${WORK}/update.json" "[
  {\"cmd\": \"update_library_parts\", \"changes\": [{\"component\": \"ISO 4762 M5x16\", \"rev\": \"v1.1.0\"}]},
  {\"expect\": {\"body\": \"ISO 4762 M5x16\", \"volume\": 573.55034687, \"tolerance\": 1e-6}},
  {\"cmd\": \"update_library_parts\", \"changes\": [{\"component\": \"ISO 4762 M5x16\", \"config\": \"M6x20\"}]},
  {\"expect\": {\"body\": \"ISO 4762 M6x20\", \"volume\": 966.3332369294, \"tolerance\": 1e-6}}
]")
cli(ok run "${WORK}/update.json" --open "${WORK}/assembly.mitcad" --save "${WORK}/updated.mitcad")
cli(ok parts "${WORK}/updated.mitcad")
expect("1  ISO 4762 M6x20  \\(mitcad-parts v1\\.1\\.0" "the updated parts list")
# A copied part does not follow the library.
file(WRITE "${WORK}/copy.json" "[
  {\"cmd\": \"update_library_parts\", \"changes\": [{\"component\": \"ISO 4032 M5\", \"rev\": \"v1.1.0\"}]}
]")
cli(fail run "${WORK}/copy.json" --open "${WORK}/assembly.mitcad")
expect("is a copy of a library part" "an update of a copy")

# The entry a community index takes for the library.
cli(ok library index-entry "${library}" --url "https://example.org/mitcad-parts.git")
expect("\"id\": \"mitcad-parts\"" "the index entry")
expect("\"url\": \"https://example.org/mitcad-parts.git\"" "the index entry's URL")

# Every size (mitcad#900): the five standards M1.6 to M24 made, checked
# and fetched into a cache of their own; the smallest and largest new
# rows of each standard inserted. The screws' volumes as above (head,
# shank and socket less the head's rounding and the tip's chamfer; ISO
# 4017's hexagon less its top edges chamfered 0.06 s at 45 degrees:
# perimeter c^2 / 2 - 2 sqrt(3) c^3 / 3), the nuts' and washers' exact.
# A DIN-only row of each kind (mitcad#949) is named by its DIN standard:
# DIN 912 M6x8, DIN 933's own head M10x20, DIN 934's M10.
set(all "${WORK}/all-sizes/mitcad-parts")
set(ENV{MITCAD_LIBRARIES_DIR} "${WORK}/cache-all")
execute_process(COMMAND "${PYTHON}" "${GENERATOR}" --cli "${CLI}" --out "${all}" --version 1.0.0 --no-render
                        --commit
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "the generator failed with every size (${status}):\n${out}${err}")
endif()
expect("mitcad-parts 1.0.0: 5 components" "the generator's summary with every size")
cli(ok library check "${all}")
expect("mitcad-parts: 5 components, no errors" "the check of every size")
cli(ok library fetch "${all}")
cli(ok library show mitcad-parts --rev v1.0.0)
expect("iso4762 Hexagon socket head cap screw \\(ISO 4762 / DIN 912\\): screws/iso4762.mitcad, 448 sizes \\(default M5x16\\)"
       "the socket head screws' sizes")
expect("iso4017 Hexagon head screw, thread to the head \\(ISO 4017 / DIN 933\\): screws/iso4017.mitcad, 429 sizes \\(default M5x20\\)"
       "the hexagon head screws' sizes")
expect("iso10642 Hexagon socket countersunk head screw \\(ISO 10642 / DIN 7991\\): screws/iso10642.mitcad, 121 sizes \\(default M5x16\\)"
       "the countersunk screws' sizes")
expect("iso4032 Hexagon nut \\(ISO 4032 / DIN 934\\): nuts/iso4032.mitcad, 23 sizes \\(default M5\\)" "the nuts' sizes")
expect("iso7089 Plain washer \\(ISO 7089 / DIN 125\\): washers/iso7089.mitcad, 14 sizes \\(default M5\\)"
       "the washers' sizes")
file(TO_CMAKE_PATH "${all}" url)
set(part "\"id\": \"mitcad-parts\", \"url\": \"${url}\", \"rev\": \"v1.0.0\"")
file(WRITE "${WORK}/all-sizes.json" "[
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4762\", \"config\": \"M1.6x2.5\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4762\", \"config\": \"M24x200\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4017\", \"config\": \"M1.6x2\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4017\", \"config\": \"M24x150\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso10642\", \"config\": \"M14x25\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso10642\", \"config\": \"M20x100\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4032\", \"config\": \"M1.6\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4032\", \"config\": \"M24\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso7089\", \"config\": \"M1.6\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso7089\", \"config\": \"M24\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4762\", \"config\": \"M6x8\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4017\", \"config\": \"M10x20 DIN 933\"}},
  {\"cmd\": \"insert_component\", \"library\": {${part}, \"component\": \"iso4032\", \"config\": \"M10 DIN 934\"}},
  {\"expect\": {\"bodies\": 13}},
  {\"expect\": {\"body\": \"ISO 4762 M1.6x2.5\", \"volume\": 14.810165764, \"tolerance\": 1e-4}},
  {\"expect\": {\"body\": \"ISO 4762 M24x200\", \"volume\": 110932.29771, \"tolerance\": 1e-4}},
  {\"expect\": {\"body\": \"ISO 4017 M1.6x2\", \"volume\": 13.481649188, \"tolerance\": 1e-4}},
  {\"expect\": {\"body\": \"ISO 4017 M24x150\", \"volume\": 84301.883326, \"tolerance\": 1e-4}},
  {\"expect\": {\"body\": \"ISO 4032 M1.6\", \"volume\": 8.9147250874, \"tolerance\": 1e-6}},
  {\"expect\": {\"body\": \"ISO 4032 M24\", \"volume\": 14404.560996, \"tolerance\": 1e-6}},
  {\"expect\": {\"body\": \"ISO 7089 M1.6\", \"volume\": 3.0889709766, \"tolerance\": 1e-6}},
  {\"expect\": {\"body\": \"ISO 7089 M24\", \"volume\": 4118.6279689, \"tolerance\": 1e-6}}
]")
cli(ok run "${WORK}/all-sizes.json" --save "${WORK}/all-sizes.mitcad")
cli(ok parts "${WORK}/all-sizes.mitcad")
foreach(row "ISO 4762 M1\\.6x2\\.5" "ISO 4762 M24x200" "ISO 4017 M1\\.6x2" "ISO 4017 M24x150"
            "ISO 10642 M14x25" "ISO 10642 M20x100" "ISO 4032 M1\\.6" "ISO 4032 M24" "ISO 7089 M1\\.6"
            "ISO 7089 M24" "DIN 912 M6x8" "DIN 933 M10x20" "DIN 934 M10")
  expect("1  ${row}  \\(mitcad-parts v1\\.0\\.0" "the parts list's ${row}")
endforeach()
