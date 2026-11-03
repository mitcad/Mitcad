# SPDX-License-Identifier: MIT
# The timber parts of Mitcad's generated parts library (mitcad#478)
# through mitcad-cli. The library is a folder made by
# tools/libraries/make-profile-library.py (or make-parts-library.py; a
# git repository with mitcad-library.json) named by
# MITCAD_PROFILES_LIBRARY; it is not in the repository, and the test is
# skipped without it. Nothing goes over the network.
#
# 1. The three timber families (sawn, planed, dimensioned), each in a
#    category of its own: every size of the tables a row, the less
#    common ones marked, every row's section its size (re-sawn 2 mm under nominal), its species' density, 20 %
#    moisture, the stock lengths 2.7 to 5.4 m; no supplier.
# 2. Their designs at two sizes each: the body W x T x L and its mass
#    (the density times the volume).
# 3. The wood fasteners' sizes and axis frames, the connectors' fits,
#    mount frames and holes, the sheets.
# 4. The pickers: the Profile picker lists no timber, the Timber picker
#    the three families, sheets only on a face.
# 5. The profiled boards (mitcad#477): tongue and groove floorboards and
#    panel boards, shiplap and feather-edge cladding, deck boards smooth
#    and grooved: their rows' profiles, cover widths and areas (the
#    designs' volume per length at two rows each), their tongue and groove
#    frames, and the pickers.
# 6. Profiled boards placed: a floorboard on another's tongue, a shiplap
#    row stepping by its cover, the cut list's area.
# 7. The default framing fastener (mitcad#516): the countersunk wood screw
#    marked wood.default, the wood picker's first; a frame with it seated
#    flush.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P build-timber-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "build-timber-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(library "$ENV{MITCAD_PROFILES_LIBRARY}")
if(library STREQUAL "")
  message("build timber: skipped (set MITCAD_PROFILES_LIBRARY to the generated parts library's folder)")
  return()
endif()
if(NOT EXISTS "${library}/mitcad-library.json")
  message(FATAL_ERROR "MITCAD_PROFILES_LIBRARY is ${library}, which has no mitcad-library.json")
endif()
set(ENV{MITCAD_LIBRARIES_DIR} "${WORK}/cache")
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")

function(cli)
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  endif()
  set(out "${out}${err}" PARENT_SCOPE)
endfunction()

function(same actual wanted what)
  if(NOT "${actual}" STREQUAL "${wanted}")
    message(FATAL_ERROR "${what}: '${actual}', not '${wanted}'")
  endif()
endfunction()

file(READ "${library}/mitcad-library.json" manifest)
string(JSON components LENGTH "${manifest}" components)
math(EXPR last_component "${components} - 1")
set(component_ids "")
foreach(i RANGE ${last_component})
  string(JSON id GET "${manifest}" components ${i} id)
  list(APPEND component_ids "${id}")
endforeach()

# The index of a component in the manifest.
function(component_index wanted out_var)
  list(FIND component_ids "${wanted}" i)
  if(i LESS 0)
    message(FATAL_ERROR "the library has no ${wanted}")
  endif()
  set(${out_var} ${i} PARENT_SCOPE)
endfunction()

# A field of a component's build block, or "(none)".
function(build_field id out_var)
  component_index("${id}" index)
  string(JSON value ERROR_VARIABLE missing GET "${manifest}" components ${index} build ${ARGN})
  if(missing)
    set(value "(none)")
  endif()
  set(${out_var} "${value}" PARENT_SCOPE)
endfunction()

function(expect_field id wanted)
  build_field("${id}" value ${ARGN})
  same("${value}" "${wanted}" "${id}: build.${ARGN}")
endfunction()

# 1. The families: rows, standard and less common, per family.
set(densities "spruce=450;pine=520")
foreach(entry IN ITEMS "timber-sawn|sawn|66|44" "timber-planed|planed|57|12" "timber-dimensioned|dimensioned|20|8")
  string(REPLACE "|" ";" fields "${entry}")
  list(GET fields 0 id)
  list(GET fields 1 finish)
  list(GET fields 2 count)
  list(GET fields 3 less)
  component_index("${id}" index)
  # Each family a group of the pickers of its own (mitcad#484).
  string(JSON category GET "${manifest}" components ${index} category)
  same("${category}" "${finish}-timber" "${id}: its category")
  foreach(key supplier part_number supplier_url)
    string(JSON value ERROR_VARIABLE missing GET "${manifest}" components ${index} ${key})
    if(NOT missing)
      message(FATAL_ERROR "${id} names a ${key}: ${value}")
    endif()
  endforeach()
  expect_field("${id}" "timber" kind)
  expect_field("${id}" "profile" role)
  expect_field("${id}" "${finish}" material finish)
  expect_field("${id}" "" material grade)
  expect_field("${id}" "20" material moisture)
  expect_field("${id}" "(none)" system)
  expect_field("${id}" "(none)" mass_per_metre)
  string(JSON stock GET "${manifest}" components ${index} build length stock)
  string(REGEX REPLACE "[][ \n]" "" stock "${stock}")
  same("${stock}" "2700,3000,3300,3600,3900,4200,4500,4800,5100,5400" "${id}: the stock lengths")
  string(JSON rows LENGTH "${manifest}" components ${index} build rows)
  same("${rows}" "${count}" "${id}: its sizes")
  math(EXPR last "${rows} - 1")
  set(marked 0)
  foreach(i RANGE ${last})
    string(JSON name MEMBER "${manifest}" components ${index} build rows ${i})
    string(JSON row GET "${manifest}" components ${index} build rows ${name})
    string(JSON availability GET "${row}" availability)
    if(availability STREQUAL "less common")
      math(EXPR marked "${marked} + 1")
    elseif(NOT availability STREQUAL "standard")
      message(FATAL_ERROR "${id} ${name}: availability '${availability}'")
    endif()
    # The section: the thickness of the name, the width the nominal's or
    # 2 mm under it (re-sawn).
    string(REGEX MATCH "^([0-9]+)x([0-9]+)$" match "${name}")
    set(thickness "${CMAKE_MATCH_1}")
    set(width "${CMAKE_MATCH_2}")
    string(JSON actual_t GET "${row}" section height)
    string(JSON actual_w GET "${row}" section width)
    string(JSON nominal_w GET "${row}" nominal width)
    same("${actual_t}" "${thickness} mm" "${id} ${name}: its thickness")
    same("${nominal_w}" "${width} mm" "${id} ${name}: its nominal width")
    math(EXPR resawn "${width} - 2")
    if(NOT actual_w STREQUAL "${width} mm" AND NOT actual_w STREQUAL "${resawn} mm")
      message(FATAL_ERROR "${id} ${name}: its width ${actual_w}")
    endif()
    string(JSON species GET "${row}" material species)
    string(JSON density GET "${row}" material density)
    if(NOT "${species}=${density}" IN_LIST densities)
      message(FATAL_ERROR "${id} ${name}: ${species} of ${density} kg/m3")
    endif()
    string(JSON tolerance GET "${row}" tolerance width)
  endforeach()
  same("${marked}" "${less}" "${id}: its less common sizes")
  message("${id}: ${rows} sizes, ${marked} less common")
endforeach()
# The footnotes: re-sawn sizes, the species by thickness, the marking.
expect_field(timber-sawn "48 mm" rows 22x50 section width)
expect_field(timber-sawn "73 mm" rows 50x75 section width)
expect_field(timber-sawn "73 mm" rows 75x75 section width)
expect_field(timber-sawn "less common" rows 75x75 availability)
expect_field(timber-sawn "standard" rows 75x200 availability)
expect_field(timber-sawn "pine" rows 19x100 material species)
expect_field(timber-sawn "pine" rows 25x225 material species)
expect_field(timber-sawn "spruce" rows 44x250 material species)
expect_field(timber-sawn "-1" rows 22x100 tolerance width 0)
expect_field(timber-sawn "4" rows 50x150 tolerance width 1)
expect_field(timber-planed "pine" rows 15x15 material species)
expect_field(timber-planed "pine" rows 21x195 material species)
expect_field(timber-planed "spruce" rows 18x45 material species)
expect_field(timber-planed "less common" rows 33x21 availability)
expect_field(timber-planed "0.5" rows 18x45 tolerance thickness 1)
expect_field(timber-dimensioned "pine" rows 20x95 material species)
expect_field(timber-dimensioned "less common" rows 48x248 availability)
expect_field(timber-dimensioned "standard" rows 42x66 availability)

# 2. The designs at two sizes: the body and its mass (density x volume,
#    in grams).
function(member id row width thickness density)
  math(EXPR volume "${width} * ${thickness} * 3000")
  math(EXPR grams "${volume} * ${density} / 1000000")
  file(WRITE "${WORK}/${id}-${row}.json" "[
  {\"cmd\": \"apply_configuration\", \"name\": \"${row}\"},
  {\"cmd\": \"set_parameter\", \"name\": \"L\", \"expression\": \"3000 mm\"},
  {\"expect\": {\"bodies\": 1}},
  {\"expect\": {\"total_volume\": ${volume}, \"tolerance\": 1e-6}}
]")
  component_index("${id}" index)
  string(JSON path GET "${manifest}" components ${index} path)
  cli(run "${WORK}/${id}-${row}.json" --open "${library}/${path}")
  build_field("${id}" listed rows ${row} material density)
  same("${listed}" "${density}" "${id} ${row}: its density")
  message("${id} ${row}: ${width} x ${thickness} x 3000 mm, ${grams} g")
endfunction()
member(timber-sawn 22x100 100 22 450)
member(timber-sawn 22x50 48 22 450)
member(timber-planed 21x95 95 21 520)
member(timber-planed 70x195 195 70 450)
member(timber-dimensioned 48x148 148 48 450)
member(timber-dimensioned 20x145 145 20 520)

# 3. The wood fasteners: sizes and the axis frame; the connectors: fits,
#    frames and fasteners; the sheets.
foreach(entry IN ITEMS "wood-screw|26|screw" "connector-screw|5|screw" "ring-nail|7|nail" "structural-screw|25|screw")
  string(REPLACE "|" ";" fields "${entry}")
  list(GET fields 0 id)
  list(GET fields 1 count)
  list(GET fields 2 kind)
  component_index("${id}" index)
  string(JSON rows LENGTH "${manifest}" components ${index} build rows)
  same("${rows}" "${count}" "${id}: its sizes")
  expect_field("${id}" "fastener" role)
  expect_field("${id}" "${kind}" wood kind)
  expect_field("${id}" "axis" frames 0 name)
  expect_field("${id}" "shaft" frames 0 kind)
  # Its axis is a mount frame too (mitcad#1197): Insert from Library
  # places it into a hole by Place Component.
  foreach(pair "name|axis" "kind|shaft" "joint|rigid")
    string(REPLACE "|" ";" pair "${pair}")
    list(GET pair 0 key)
    list(GET pair 1 wanted)
    string(JSON value ERROR_VARIABLE missing GET "${manifest}" components ${index} mounts 0 ${key})
    same("${value}" "${wanted}" "${id}: mounts 0 ${key}")
  endforeach()
endforeach()
expect_field(wood-screw "40 mm" rows 4.5x40 wood length)
expect_field(wood-screw "4.5 mm" rows 4.5x40 wood diameter)
foreach(height 98 123 148 173 198 223)
  expect_field("joist-hanger-48x${height}" "hanger" connector fit kind)
  expect_field("joist-hanger-48x${height}" "${height}" connector fit section 1)
  expect_field("joist-hanger-48x${height}" "outer" connector placement)
  expect_field("joist-hanger-48x${height}" "mount" connector mates 0 frame)
  expect_field("joist-hanger-48x${height}" "mount" frames 0 name)
  expect_field("joist-hanger-48x${height}" "connector-screw" connector fasteners 0 screw component)
  expect_field("joist-hanger-48x${height}" "4" connector fasteners 0 count)
endforeach()
foreach(size 40 60 90)
  component_index("angle-bracket-${size}" index)
  expect_field("angle-bracket-${size}" "angle" connector fit kind)
endforeach()
foreach(size 98 123)
  expect_field("post-base-${size}" "post_base" connector fit kind)
  expect_field("post-base-${size}" "${size}" connector fit section 0)
  expect_field("post-base-${size}" "base" frames 1 name)
endforeach()
foreach(id plywood-spruce plywood-birch osb-3 chipboard-p5)
  expect_field("${id}" "face" panel mount)
  expect_field("${id}" "T" panel thickness)
endforeach()

#    The hanger 48x148 in its mount frame's coordinates: round the
#    joist's end at the origin, x up; one solid with its holes.
file(WRITE "${WORK}/hanger.json" "[
  {\"expect\": {\"bodies\": 1}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-76, -56, -70], \"max\": [56, 56, 0]}}]}}
]")
cli(run "${WORK}/hanger.json" --open "${library}/timber-connectors/joist-hanger-48x148.mitcad")

# 4. The pickers.
cli(library fetch "${library}")
cli(library candidates --role profile)
if(out MATCHES "timber-")
  message(FATAL_ERROR "the Profile picker lists timber:\n${out}")
endif()
cli(library candidates --role profile --kind timber)
foreach(id timber-sawn timber-planed timber-dimensioned)
  if(NOT out MATCHES "mitcad-parts/${id} ")
    message(FATAL_ERROR "the Timber picker has no ${id}:\n${out}")
  endif()
endforeach()
if(out MATCHES "p30x30")
  message(FATAL_ERROR "the Timber picker lists slot profiles:\n${out}")
endif()
cli(library candidates --role panel --mount face)
foreach(id plywood-spruce plywood-birch osb-3 chipboard-p5)
  if(NOT out MATCHES "mitcad-parts/${id} ")
    message(FATAL_ERROR "the sheets have no ${id}:\n${out}")
  endif()
endforeach()
cli(library candidates --role panel)
if(out MATCHES "plywood|osb-3|chipboard")
  message(FATAL_ERROR "the slot panels' picker lists sheets:\n${out}")
endif()

# 5. The profiled boards (mitcad#477): each family a part of its
#    category, every row a profile, a cover width not over its width and
#    an area; no supplier. Their designs at two rows: the body's volume at
#    a length of 1 mm is the row's area (so the mass, the density times
#    it, is the cut list's); a board that joins the next has its tongue
#    and groove the cover width apart.
foreach(entry IN ITEMS "floorboard-tg|tongue-and-groove|tongue_and_groove|8|28x120|21x95"
                       "panel-board-tg|tongue-and-groove|tongue_and_groove|8|15x95|20x120"
                       "cladding-shiplap|cladding|shiplap|5|22x145|28x170"
                       "cladding-feather-edge|cladding|feather_edge|4|22x150|22x100"
                       "deck-board|decking|deck|5|28x120|34x145"
                       "deck-board-grooved|decking|deck|3|28x120|34x145")
  string(REPLACE "|" ";" fields "${entry}")
  list(GET fields 0 id)
  list(GET fields 1 category)
  list(GET fields 2 profile)
  list(GET fields 3 count)
  list(GET fields 4 first_row)
  list(GET fields 5 second_row)
  component_index("${id}" index)
  string(JSON value GET "${manifest}" components ${index} category)
  same("${value}" "${category}" "${id}: its category")
  foreach(key supplier part_number supplier_url)
    string(JSON value ERROR_VARIABLE missing GET "${manifest}" components ${index} ${key})
    if(NOT missing)
      message(FATAL_ERROR "${id} names a ${key}: ${value}")
    endif()
  endforeach()
  expect_field("${id}" "timber" kind)
  expect_field("${id}" "${profile}" board profile)
  string(JSON rows LENGTH "${manifest}" components ${index} build rows)
  same("${rows}" "${count}" "${id}: its sizes")
  math(EXPR last "${rows} - 1")
  foreach(i RANGE ${last})
    string(JSON name MEMBER "${manifest}" components ${index} build rows ${i})
    string(JSON row GET "${manifest}" components ${index} build rows ${name})
    string(JSON row_profile GET "${row}" board profile)
    same("${row_profile}" "${profile}" "${id} ${name}: its profile")
    string(JSON cover GET "${row}" board cover)
    string(JSON width GET "${row}" section width)
    string(REGEX REPLACE " mm$" "" cover "${cover}")
    string(REGEX REPLACE " mm$" "" width "${width}")
    if(cover GREATER width)
      message(FATAL_ERROR "${id} ${name}: its cover ${cover} is over its width ${width}")
    endif()
    if(profile STREQUAL "deck" AND NOT cover STREQUAL width)
      message(FATAL_ERROR "${id} ${name}: a deck board covers its width, not ${cover}")
    endif()
    string(JSON area GET "${row}" board area)
  endforeach()
  component_index("${id}" index)
  string(JSON path GET "${manifest}" components ${index} path)
  foreach(row ${first_row} ${second_row})
    build_field("${id}" area rows ${row} board area)
    file(WRITE "${WORK}/${id}-${row}.json" "[
  {\"cmd\": \"apply_configuration\", \"name\": \"${row}\"},
  {\"cmd\": \"set_parameter\", \"name\": \"L\", \"expression\": \"1 mm\"},
  {\"expect\": {\"bodies\": 1}},
  {\"expect\": {\"total_volume\": ${area}, \"tolerance\": 1e-5}}
]")
    cli(run "${WORK}/${id}-${row}.json" --open "${library}/${path}")
    build_field("${id}" cover rows ${row} board cover)
    message("${id} ${row}: ${area} mm2, covers ${cover}")
  endforeach()
  if(NOT profile STREQUAL "deck")
    expect_field("${id}" "groove" frames 6 kind)
    expect_field("${id}" "tongue" frames 7 kind)
  endif()
endforeach()
# The floorboard 28x120: its tongue (mount:tongue, F17) 112 from its groove
# (F15).
file(WRITE "${WORK}/floorboard-frames.json" "[
  {\"cmd\": \"apply_configuration\", \"name\": \"28x120\"},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F15\"}, \"result\": {\"origin\": [-60, 0, 0], \"normal\": [-1, 0, 0]}}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F17\"}, \"result\": {\"origin\": [52, 0, 0], \"normal\": [1, 0, 0]}}}
]")
component_index(floorboard-tg index)
string(JSON path GET "${manifest}" components ${index} path)
cli(run "${WORK}/floorboard-frames.json" --open "${library}/${path}")
cli(library candidates --role profile --kind timber)
foreach(id floorboard-tg panel-board-tg cladding-shiplap cladding-feather-edge deck-board deck-board-grooved)
  if(NOT out MATCHES "mitcad-parts/${id} ")
    message(FATAL_ERROR "the Timber picker has no ${id}:\n${out}")
  endif()
endforeach()
cli(library candidates --role profile)
if(out MATCHES "floorboard|cladding-|deck-board")
  message(FATAL_ERROR "the Profile picker lists profiled boards:\n${out}")
endif()

# 6. Placed (with the kernel): a floorboard 28x120 on another's tongue
#    lies 112 on, its groove on that tongue; a row of shiplap 22x145 over
#    1000 steps by its cover, 133 (eight boards, the last ripped to 69);
#    the cut list's area: 2 x 0.112 + 1.0 m2.
file(TO_CMAKE_PATH "${library}" url)
set(floorboard "{\"library\": \"mitcad-parts\", \"url\": \"${url}\", \"rev\": \"HEAD\", \"component\": \"floorboard-tg\", \"config\": \"28x120\"}")
set(shiplap "{\"library\": \"mitcad-parts\", \"url\": \"${url}\", \"rev\": \"HEAD\", \"component\": \"cladding-shiplap\", \"config\": \"22x145\"}")
file(WRITE "${WORK}/profiled-placed.json" "[
  {\"cmd\": \"build_place\", \"part\": ${floorboard}, \"at\": {\"point\": [0, 0, 0]}, \"length\": 1000},
  {\"cmd\": \"build_place\", \"part\": {\"source\": \"S1\", \"config\": \"28x120\"}, \"length\": 1000,
   \"at\": {\"occurrence\": \"O1\", \"geometry\": {\"mount\": \"F1:mount(tongue)\"}}},
  {\"expect\": {\"query\": {\"query\": \"joint_frame\", \"occurrence\": \"O2\", \"geometry\": {\"mount\": \"F2:mount(groove)\"}},
               \"result\": {\"origin\": [52, 0, 0], \"z_axis\": [-1, 0, 0]}}},
  {\"expect\": {\"query\": {\"query\": \"joint_frame\", \"occurrence\": \"O2\", \"geometry\": {\"mount\": \"F2:mount(tongue)\"}},
               \"result\": {\"origin\": [164, 0, 0], \"z_axis\": [1, 0, 0]}}},
  {\"cmd\": \"build_place\", \"part\": ${shiplap}, \"at\": {\"point\": [0, 500, 0]}, \"to\": {\"point\": [1000, 500, 0]},
   \"spacing\": {\"mode\": \"gap\", \"along\": [0, 1, 0], \"length\": 1000}},
  {\"expect\": {\"query\": {\"query\": \"build_cut_list\"}, \"result\": {\"profiled\": true, \"totals\": {\"area\": 1.224}}}}
]")
cli(run "${WORK}/profiled-placed.json")
if(NOT out MATCHES "Placed bodies: 10\n")
  message(FATAL_ERROR "the shiplap row is not of eight boards:\n${out}")
endif()

# 7. The default framing fastener (mitcad#516): the library marks the
#    countersunk wood screw (wood.default), seated by its head's top, not
#    the pan-head connector screw; the wood picker lists it first. A frame
#    with it, as Timber sends it before the user chooses a fastener (sized
#    from the members), has every screw's head top flush with the wood.
# string(JSON ... GET) gives a JSON true as ON: compared as a CMake boolean.
build_field(wood-screw marked wood default)
if(NOT marked MATCHES "^(ON|TRUE|true|1)$")
  message(FATAL_ERROR "wood-screw: build.wood;default: '${marked}', not true")
endif()
expect_field(wood-screw "head_top" wood seat)
expect_field(connector-screw "(none)" wood default)
expect_field(connector-screw "head_underside" wood seat)
cli(library candidates --role fastener --kind wood)
string(REGEX MATCH "mitcad-parts/[a-z0-9-]+ " first "${out}")
same("${first}" "mitcad-parts/wood-screw " "the wood picker's first fastener")
set(members "")
foreach(i RANGE 1 5)
  list(APPEND members "{\"kind\": \"timber\"}")
endforeach()
set(seats "")
foreach(i RANGE 1 12)
  list(APPEND seats "{\"role\": \"fastener\", \"seating\": {\"head_top_distance\": 0}}")
endforeach()
string(JOIN "," members ${members})
string(JOIN "," seats ${seats})
file(WRITE "${WORK}/default-fastener.json" "[
  {\"cmd\": \"build_frame\", \"kind\": \"wall\",
   \"part\": {\"library\": \"mitcad-parts\", \"url\": \"${url}\", \"rev\": \"HEAD\", \"component\": \"timber-dimensioned\", \"config\": \"48x98\"},
   \"start\": [0, 0, 0], \"end\": [1200, 0, 0], \"height\": 600, \"spacing\": 600,
   \"fastener\": {\"library\": \"mitcad-parts\", \"url\": \"${url}\", \"rev\": \"HEAD\", \"component\": \"wood-screw\"}},
  {\"expect\": {\"query\": {\"query\": \"build_parts\"}, \"tolerance\": 1e-6,
               \"result\": {\"parts\": [${members},${seats}]}}}
]")
cli(run "${WORK}/default-fastener.json")
message("the default framing fastener: wood-screw first, a frame's 12 screws flush")

message("the timber parts: their sizes, designs, fasteners, connectors, sheets, profiled boards and pickers")
