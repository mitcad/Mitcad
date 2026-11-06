# SPDX-License-Identifier: MIT
# The timber parts of Mitcad's parts library (mitcad#478) through
# mitcad-cli: the generator (tools/libraries/make-profile-library.py) on
# the test's group files (tools/cli/tests/timber-groups: a board family
# with a re-sawn and a less common size, a sheet, a wood screw and a nail,
# an angle bracket, a joist hanger and a post base, all modelled from
# their dimensions, no supplier's files); the library checked, its
# manifest's timber fields and the rows' own blocks, the designs (the
# board's section, volume and face frames following a row, its wood
# appearance; the fastener's head; the connectors' plates, holes and
# frames; the sheet), the profiled boards (mitcad#477: tongue and groove
# with and without a V-joint, shiplap, feather-edge and a grooved deck
# board; their board blocks, volumes and tongue and groove frames
# following rows and parameters), the steel tubes (mitcad#873: a round,
# a square and a furniture tube; their rows' corner radii, masses per
# metre and tolerances, their volumes against the exact areas and the
# standards' masses, the furniture tube's bores switched by its rows;
# mitcad#1328: inch tubes of ASTM A500 named and sized in inches, a tube of
# a typed size that takes a size typed and refuses a wall of a third of
# its side, both placed by BUILD with their cut-list masses), and the
# pickers: timber only where it is asked for, the steel tubes among the
# profiles, a junction of members offered the wood fasteners and the
# connectors that fit its section. Nothing goes over the network.
#
# cmake -DCLI=<mitcad-cli> -DGIT=<git> -DPYTHON=<python3> -DGENERATOR=<make-profile-library.py>
#       -DGROUPS=<tools/cli/tests/timber-groups> -DWORK=<folder> -P timber-library-test.cmake

foreach(var CLI GIT PYTHON GENERATOR GROUPS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "timber-library-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(library "${WORK}/mitcad-parts")
set(ENV{MITCAD_LIBRARIES_DIR} "${WORK}/cache")
set(ENV{GIT_CONFIG_NOSYSTEM} 1)
set(ENV{GIT_CONFIG_GLOBAL} "${WORK}/gitconfig")

function(cli expect)
  # UTF-8: the messages carry "×"; Windows would decode them in the console code page.
  execute_process(COMMAND "${CLI}" ${ARGN} RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err
                  ENCODING UTF8)
  if(expect STREQUAL "ok" AND NOT status EQUAL 0)
    message(FATAL_ERROR "mitcad-cli ${ARGN} failed (${status}):\n${out}${err}")
  endif()
  set(out "${out}${err}" PARENT_SCOPE)
endfunction()

function(expect pattern what)
  if(NOT out MATCHES "${pattern}")
    message(FATAL_ERROR "${what}:\n${out}")
  endif()
endfunction()

function(same actual wanted what)
  # A number may come back from string(JSON) with 17 digits
  # (0.14999999999999999 for 0.15): equal as numbers is the same.
  if(NOT "${actual}" STREQUAL "${wanted}" AND NOT ("${actual}" MATCHES "^-?[0-9.]+$" AND
     "${wanted}" MATCHES "^-?[0-9.]+$" AND "${actual}" EQUAL "${wanted}"))
    message(FATAL_ERROR "${what}: '${actual}', not '${wanted}'")
  endif()
endfunction()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}" "${WORK}/supplier")
file(WRITE "${WORK}/gitconfig" "")

# 1. The library, v1.0.0, from the test's group files.
execute_process(COMMAND "${PYTHON}" "${GENERATOR}" --cli "${CLI}" --source "${WORK}/supplier" --out "${library}"
                        --groups "${GROUPS}" --work "${WORK}" --version 1.0.0 --no-render --commit
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
set(out "${out}${err}")
if(NOT status EQUAL 0)
  message(FATAL_ERROR "the generator failed (${status}):\n${out}")
endif()
expect("mitcad-parts 1.0.0: 20 components" "the generator's summary")
if(out MATCHES "left out|warning")
  message(FATAL_ERROR "a part left out or warned of:\n${out}")
endif()
cli(ok library check "${library}")
expect("mitcad-parts: 20 components, no errors" "the library's check")
file(READ "${library}/SOURCES.md" sources)
if(NOT sources MATCHES "test-board \\(Test timber\\): sizes made up for the test"
   OR NOT sources MATCHES "test-shs \\(Test square tube\\): sizes of EN 10219-2 for the test"
   OR NOT sources MATCHES "test-furniture-tube \\(Test furniture tube\\): sizes of EN 10305-5 for the test"
   OR NOT sources MATCHES "test-hss \\(Test inch square tube\\): sizes of ASTM A500 for the test")
  message(FATAL_ERROR "SOURCES.md:\n${sources}")
endif()

# 2. The manifest: the parts' blocks, their rows' own and what the
#    generator filled in. Components in the group files' order.
file(READ "${library}/mitcad-library.json" manifest)
function(field wanted what)
  string(JSON value ERROR_VARIABLE missing GET "${manifest}" components ${ARGN})
  if(missing)
    set(value "(none)")
  endif()
  same("${value}" "${wanted}" "${what}")
endfunction()
function(frames index wanted)
  string(JSON count LENGTH "${manifest}" components ${index} build frames)
  set(names "")
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON name GET "${manifest}" components ${index} build frames ${i} name)
    list(APPEND names "${name}")
  endforeach()
  same("${names}" "${wanted}" "the frames of component ${index}")
endfunction()

field("test-board" "the first component" 0 id)
field("(none)" "the board's supplier" 0 supplier)
field("timber" "the board's kind" 0 build kind)
field("(none)" "the board's system" 0 build system)
field("(none)" "the board's mass per metre" 0 build mass_per_metre)
# The part's fields are its default row's (22x100).
field("100 mm" "the board's width" 0 build section width)
field("22 mm" "the board's thickness" 0 build section height)
field("standard" "the board's availability" 0 build availability)
field("spruce" "the board's species" 0 build material species)
field("450" "the board's density" 0 build material density)
field("sawn" "the board's finish" 0 build material finish)
# The rows: re-sawn 2 mm under its nominal width; a less common size of
# pine with the tolerance of its size.
field("48 mm" "22x50's actual width" 0 build rows 22x50 section width)
field("50 mm" "22x50's nominal width" 0 build rows 22x50 nominal width)
field("less common" "48x148's availability" 0 build rows 48x148 availability)
field("pine" "48x148's species" 0 build rows 48x148 material species)
field("520" "48x148's density" 0 build rows 48x148 material density)
field("-2" "48x148's width tolerance" 0 build rows 48x148 tolerance width 0)
field("3" "48x148's thickness tolerance" 0 build rows 48x148 tolerance thickness 1)
frames(0 "end_a;end_b;face_xp;face_xn;face_yp;face_yn")
field("face" "the sheet's mount" 1 build panel mount)
field("2500" "the sheet's second size" 1 build panel stock 1 1)
field("less common" "the 18 mm sheet" 1 build rows 18 availability)
frames(1 "face")
field("40 mm" "the screw 5.0x40's length" 2 build rows 5.0x40 wood length)
field("50 mm" "the screw's default length" 2 build wood length)
field("5 mm" "the screw's diameter" 2 build wood diameter)
field("7850" "the screw's density (its group file's)" 2 build material density)
frames(2 "axis")
field("nail" "the nail's kind" 3 build wood kind)
frames(4 "mount;hole_b_1;hole_b_2;hole_a_1")
field("hole_a_1" "the bracket's screws into a" 4 build connector fasteners 0 frames 0)
field("1" "the bracket's screw count" 4 build connector fasteners 0 count)
field("2" "the bracket's nail count" 4 build connector fasteners 1 count)
frames(5 "mount;hole_b_1;hole_b_2;hole_b_3;hole_b_4;hole_b_5;hole_b_6;hole_a_1;hole_a_2;hole_a_3;hole_a_4")
field("4" "the hanger's screws into the joist" 5 build connector fasteners 0 count)
field("6" "the hanger's screws into the carrier" 5 build connector fasteners 1 count)
field("5 mm" "the hanger's hole" 5 build frames 1 diameter)
frames(6 "mount;base;hole_a_1;hole_a_2;hole_a_3;hole_a_4")

# 3. The designs. The board: its section on XY extruded by L, 100 x 22 x
#    3000 by default; in the row 22x50 (48 wide) its body and the face
#    frames follow (mount:face_xp is F7 after its plane, mount:face_yn
#    F13); its bodies take the wood appearance, whose texture is embedded.
file(WRITE "${WORK}/board.json" "[
  {\"expect\": {\"total_volume\": 6600000, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F7\"}, \"result\": {\"origin\": [50, 0, 0], \"normal\": [1, 0, 0], \"x_axis\": [0, 0, 1]}}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F13\"}, \"result\": {\"origin\": [0, -11, 0], \"normal\": [0, -1, 0]}}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"appearance\": \"test-wood\", \"bbox\": {\"min\": [-50, -11, 0], \"max\": [50, 11, 3000]}}]}},
  {\"cmd\": \"apply_configuration\", \"name\": \"22x50\"},
  {\"expect\": {\"total_volume\": 3168000, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F7\"}, \"result\": {\"origin\": [24, 0, 0], \"normal\": [1, 0, 0]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"48x148\"},
  {\"expect\": {\"total_volume\": 21312000, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F13\"}, \"result\": {\"origin\": [0, -24, 0], \"normal\": [0, -1, 0]}}},
  {\"cmd\": \"set_parameter\", \"name\": \"L\", \"expression\": \"2000 mm\"},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F5\"}, \"result\": {\"origin\": [0, 0, 2000], \"normal\": [0, 0, 1]}}}
]")
cli(ok run "${WORK}/board.json" --open "${library}/timber/test-board.mitcad")
file(READ "${library}/timber/test-board.mitcad" design)
string(JSON appearance GET "${design}" appearances 0 id)
same("${appearance}" "test-wood" "the board's appearance")
string(JSON projection GET "${design}" appearances 0 texture projection)
same("${projection}" "box" "the texture's projection")

#    The sheet: a box from the origin, 1200 x 2400 x 12; 18 thick in its
#    row 18.
file(WRITE "${WORK}/sheet.json" "[
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [0, 0, 0], \"max\": [1200, 2400, 12]}}]}},
  {\"cmd\": \"apply_configuration\", \"name\": \"18\"},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [0, 0, 0], \"max\": [1200, 2400, 18]}}]}}
]")
cli(ok run "${WORK}/sheet.json" --open "${library}/timber/test-sheet.mitcad")

#    The screw: its countersunk head in z > 0, the tip at its length; the
#    nail's flat head on z < 0.
file(WRITE "${WORK}/screw.json" "[
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"appearance\": \"steel_satin\", \"bbox\": {\"min\": [-5, -5, 0], \"max\": [5, 5, 50]}}]}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F3\"}, \"result\": {\"origin\": [0, 0, 0], \"normal\": [0, 0, 1]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"5.0x40\"},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-5, -5, 0], \"max\": [5, 5, 40]}}]}}
]")
cli(ok run "${WORK}/screw.json" --open "${library}/wood/test-screw.mitcad")
file(WRITE "${WORK}/nail.json" "[
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-4, -4, -1.5], \"max\": [4, 4, 40]}}]}}
]")
cli(ok run "${WORK}/nail.json" --open "${library}/wood/test-nail.mitcad")

#    The connectors in their mount frame's coordinates: the bracket's arms
#    on z = 0 (to +x) and x = 0 (to -z); the hanger round a 48 x 148 joist
#    whose end is at the origin, x up; the post base under the post's end.
#    Each is one solid: its plates joined, less its holes (a hole through a
#    plate of thickness t is pi r^2 t).
function(connector id bbox_min bbox_max volume)
  file(WRITE "${WORK}/${id}.json" "[
  {\"expect\": {\"bodies\": 1}},
  {\"expect\": {\"total_volume\": ${volume}, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [${bbox_min}], \"max\": [${bbox_max}]}}]}}
]")
  cli(ok run "${WORK}/${id}.json" --open "${library}/wood/${id}.mitcad")
endfunction()
# The bracket: arms 60 x 40 x 2 and 2 x 40 x 50 overlapping by 2 x 40 x 2
# (8640), three 5 mm holes through 2 mm.
connector(test-bracket "0, -20, -50" "60, 20, 0" 8522.19027549)
# The hanger: a seat 2 x 52 x 70, sides 132 x 2 x 70, flanges 132 x 32 x 2
# (59520 joined), ten 5 mm holes through 2 mm.
connector(test-hanger "-76, -56, -70" "56, 56, 0" 59127.30091830)
# The post base: a plate 160 x 160 x 5 and sides 4 x 60 x 155 (200000
# joined), four 5.5 mm holes through 4 mm.
connector(test-post-base "-80, -80, -5" "80, 80, 150" 199619.86727)

#    Profiled boards (mitcad#477, c-profiled.json): their board blocks
#    (profile, cover width, area, joint), per row; their frames, a board
#    that joins the next with its groove and tongue the cover width apart.
field("tongue_and_groove" "the floorboard's profile" 7 build board profile)
field("112 mm" "the floorboard's cover" 7 build board cover)
field("3123.5" "the floorboard's area" 7 build board area)
field("8 mm" "the floorboard's tongue" 7 build board tongue length)
field("137 mm" "28x145's cover" 7 build rows 28x145 board cover)
field("3823.5" "28x145's area" 7 build rows 28x145 board area)
frames(7 "end_a;end_b;face_xp;face_xn;face_yp;face_yn;groove;tongue")
field("groove" "the floorboard's groove frame" 7 build frames 6 kind)
field("86 mm" "the panel board's cover" 8 build board cover)
field("shiplap" "the shiplap's profile" 9 build board profile)
field("133 mm" "the shiplap 22x145's cover" 9 build rows 22x145 board cover)
field("12 mm" "the shiplap's overlap" 9 build board overlap)
field("feather_edge" "the feather-edge's profile" 10 build board profile)
field("120.3521 mm" "the feather-edge's cover" 10 build board cover)
field("100.4223 mm" "the feather-edge 22x125's cover (overlap 25)" 10 build rows 22x125 board cover)
field("25 mm" "the feather-edge 22x125's overlap" 10 build rows 22x125 board overlap)
field("deck" "the deck board's profile" 11 build board profile)
field("120 mm" "the deck board's cover (its width)" 11 build board cover)
field("4" "the deck board's grooves" 11 build board grooves)
field("5 mm" "the deck board's edge radius" 11 build board edge_radius)
frames(11 "end_a;end_b;face_xp;face_xn;face_yp;face_yn")

#    Their designs: the volume the area times the length, in every row and
#    when a parameter changes; the frames follow (mount:groove F15,
#    mount:tongue F17). The floorboard: 112 x 28 + a tongue 8 x 8 less a
#    groove 9 x 8.5.
file(WRITE "${WORK}/floorboard.json" "[
  {\"expect\": {\"total_volume\": 9370500, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F15\"}, \"result\": {\"origin\": [-60, 0, 0], \"normal\": [-1, 0, 0], \"x_axis\": [0, 0, 1]}}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F17\"}, \"result\": {\"origin\": [52, 0, 0], \"normal\": [1, 0, 0], \"x_axis\": [0, 0, 1]}}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"appearance\": \"test-floor\", \"bbox\": {\"min\": [-60, -14, 0], \"max\": [60, 14, 3000]}}]}},
  {\"cmd\": \"apply_configuration\", \"name\": \"28x145\"},
  {\"expect\": {\"total_volume\": 11470500, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F17\"}, \"result\": {\"origin\": [64.5, 0, 0]}}},
  {\"cmd\": \"set_parameter\", \"name\": \"TL\", \"expression\": \"10 mm\"},
  {\"expect\": {\"total_volume\": 11299500, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F17\"}, \"result\": {\"origin\": [62.5, 0, 0]}}}
]")
cli(ok run "${WORK}/floorboard.json" --open "${library}/profiled/test-floorboard.mitcad")
#    The panel board with its V-joint: 86 x 15 + 9 x 5 - 10 x 5.5 less two
#    chamfers 2 x 2 at 45 degrees; at 30 degrees they are 2 x 2 tan(30).
file(WRITE "${WORK}/panel.json" "[
  {\"expect\": {\"total_volume\": 3828000, \"tolerance\": 1e-6}},
  {\"cmd\": \"set_parameter\", \"name\": \"VA\", \"expression\": \"30 deg\"},
  {\"expect\": {\"total_volume\": 3833071.797, \"tolerance\": 1e-6}}
]")
cli(ok run "${WORK}/panel.json" --open "${library}/profiled/test-panel.mitcad")
#    The shiplap: 120 x 22 less 12 x 13 and 12 x 9; its tongue (the lip)
#    the cover width from its groove.
file(WRITE "${WORK}/shiplap.json" "[
  {\"expect\": {\"total_volume\": 7128000, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F17\"}, \"result\": {\"origin\": [48, 0, 0]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"22x145\"},
  {\"expect\": {\"total_volume\": 8778000, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F15\"}, \"result\": {\"origin\": [-72.5, 0, 0]}}}
]")
cli(ok run "${WORK}/shiplap.json" --open "${library}/profiled/test-shiplap.mitcad")
#    The feather-edge: (22 + 6) / 2 x 150, turned as laid (by atan2(9.2,
#    120)); the cover from the overlap; its box centred.
file(WRITE "${WORK}/feather.json" "[
  {\"expect\": {\"total_volume\": 6300000, \"tolerance\": 1e-6}},
  {\"expect\": {\"parameter\": \"COV\", \"value\": 120.35214996002357}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-75.00987729, -16.70098956, 0], \"max\": [75.00987729, 16.70098956, 3000]}}]}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F17\"}, \"result\": {\"origin\": [45.34227267, 0, 0]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"22x125\"},
  {\"expect\": {\"total_volume\": 5250000, \"tolerance\": 1e-6}},
  {\"expect\": {\"parameter\": \"COV\", \"value\": 100.42230827858917}}
]")
cli(ok run "${WORK}/feather.json" --open "${library}/profiled/test-feather-edge.mitcad")
#    The deck board: 120 x 28 less four corners of radius 5 and four
#    grooves 5 x 3.
file(WRITE "${WORK}/deck.json" "[
  {\"expect\": {\"total_volume\": 9835619.448, \"tolerance\": 1e-6}},
  {\"cmd\": \"apply_configuration\", \"name\": \"28x145\"},
  {\"expect\": {\"total_volume\": 11935619.448, \"tolerance\": 1e-6}},
  {\"cmd\": \"set_parameter\", \"name\": \"R\", \"expression\": \"8 mm\"},
  {\"expect\": {\"total_volume\": 11835185.787, \"tolerance\": 1e-6}}
]")
cli(ok run "${WORK}/deck.json" --open "${library}/profiled/test-deck.mitcad")

#    Steel tubes (mitcad#873, d-tubes.json): profiles of no slot system and
#    no supplier, their rows sorted by size and wall; each row's corner
#    radii by the rules of its sizes (EN 10219-2: 2 T and T up to 6 mm,
#    2.5 T and 1.5 T up to 10 mm; EN 10305-5: 0.5 T and a sharp inner
#    corner up to 2.5 mm, 1.75 T and 0.75 T over it), its mass per metre the
#    exact area times 7850 kg/m3, the furniture tube's tolerances.
field("test-chs" "the round tube" 12 id)
field("(none)" "the round tube's supplier" 12 supplier)
field("(none)" "the round tube's kind" 12 build kind)
field("(none)" "the round tube's system" 12 build system)
field("48.3 mm" "the round tube's default section" 12 build section width)
field("3.559 kg" "CHS 48.3x3.2's mass per metre" 12 build mass_per_metre)
field("13.478 kg" "CHS 114.3x5's mass per metre" 12 build rows 114.3x5 mass_per_metre)
field("5 mm" "CHS 114.3x5's wall" 12 build rows 114.3x5 tube thickness)
field("7850" "the round tube's density" 12 build material density)
frames(12 "end_a;end_b;axis")
field("shaft" "the round tube's axis" 12 build frames 2 kind)
field("3.303 kg" "SHS 40x40x3's mass per metre" 13 build mass_per_metre)
field("6 mm" "SHS 40x40x3's outer radius (2 T)" 13 build tube outer_radius)
field("3 mm" "SHS 40x40x3's inner radius (T)" 13 build tube inner_radius)
field("25 mm" "SHS 100x100x10's outer radius (2.5 T)" 13 build rows 100x100x10 tube outer_radius)
field("15 mm" "SHS 100x100x10's inner radius (1.5 T)" 13 build rows 100x100x10 tube inner_radius)
field("25.565 kg" "SHS 100x100x10's mass per metre" 13 build rows 100x100x10 mass_per_metre)
field("100 mm" "SHS 100x100x10's section" 13 build rows 100x100x10 section height)
frames(13 "end_a;end_b;face_xp;face_xn;face_yp;face_yn")
field("1.339 kg" "30x30x1.5's mass per metre" 14 build mass_per_metre)
field("0.75 mm" "30x30x1.5's outer radius (0.5 T)" 14 build tube outer_radius)
field("0 mm" "30x30x1.5's sharp inner corner" 14 build tube inner_radius)
field("-0.25" "30x30x1.5's side tolerance" 14 build tolerance size 0)
field("0.15" "30x30x1.5's wall tolerance" 14 build tolerance thickness 1)
field("5.25 mm" "35x35x3's outer radius (1.75 T)" 14 build rows 35x35x3 tube outer_radius)
field("2.25 mm" "35x35x3's inner radius (0.75 T)" 14 build rows 35x35x3 tube inner_radius)
field("2.863 kg" "35x35x3's mass per metre" 14 build rows 35x35x3 mass_per_metre)
field("0.3" "35x35x3's wall tolerance (0.1 T)" 14 build rows 35x35x3 tolerance thickness 1)
field("4.252 kg" "40x40x4's mass per metre" 14 build rows 40x40x4 mass_per_metre)
field("0.35" "40x40x4's wall tolerance (at most 0.35)" 14 build rows 40x40x4 tolerance thickness 1)
field("2000" "the furniture tube's shorter stock length" 14 build length stock 0)
# The rectangular tubes (mitcad#892): the square ones' rules on a width and
# a height, the rows `<W>x<H>x<T>`, the face frames on both sides.
field("test-rhs" "the rectangular tube" 15 id)
field("8.594 kg" "RHS 100x50x4's mass per metre" 15 build mass_per_metre)
field("100 mm" "RHS 100x50x4's width" 15 build section width)
field("50 mm" "RHS 100x50x4's height" 15 build section height)
field("rectangular" "RHS 100x50x4's shape" 15 build tube shape)
field("8 mm" "RHS 100x50x4's outer radius (2 T)" 15 build tube outer_radius)
field("20 mm" "RHS 200x100x8's outer radius (2.5 T)" 15 build rows 200x100x8 tube outer_radius)
field("12 mm" "RHS 200x100x8's inner radius (1.5 T)" 15 build rows 200x100x8 tube inner_radius)
field("33.945 kg" "RHS 200x100x8's mass per metre" 15 build rows 200x100x8 mass_per_metre)
frames(15 "end_a;end_b;face_xp;face_xn;face_yp;face_yn")
field("1.752 kg" "40x20x2's mass per metre" 16 build mass_per_metre)
field("0 mm" "40x20x2's sharp inner corner" 16 build tube inner_radius)
field("-0.3" "40x20x2's width tolerance" 16 build tolerance width 0)
field("0.3" "40x20x2's height tolerance" 16 build tolerance height 1)
field("0.2" "40x20x2's wall tolerance (0.1 T)" 16 build tolerance thickness 1)
field("2.392 kg" "35x25x3's mass per metre" 16 build rows 35x25x3 mass_per_metre)
# The inch tubes (mitcad#1328): rows named in inches as the tables write
# them, sized in mm (HSS 3x3x3/16: 76.2 mm outside, a 4.7625 mm wall, the
# corners 2 t and t), sorted by size; their masses per metre the exact
# area times 7850 kg/m3, within 0.5 % of the tables' nominal weights
# (6.87 lb/ft = 10.22 kg/m, 0.98 lb/ft = 1.458, 2.13 lb/ft = 3.170, 8.81
# lb/ft = 13.11).
field("test-hss" "the inch square tube" 17 id)
field("76.2 mm" "HSS 3x3x3/16's side" 17 build section width)
field("4.7625 mm" "HSS 3x3x3/16's wall" 17 build tube thickness)
field("9.525 mm" "HSS 3x3x3/16's outer radius (2 t)" 17 build tube outer_radius)
field("4.7625 mm" "HSS 3x3x3/16's inner radius (t)" 17 build tube inner_radius)
field("10.224 kg" "HSS 3x3x3/16's mass per metre" 17 build mass_per_metre)
field("25.4 mm" "HSS 1x1x0.083's side" 17 build rows 1x1x0.083 section width)
field("2.1082 mm" "HSS 1x1x0.083's wall" 17 build rows 1x1x0.083 tube thickness)
field("1.452 kg" "HSS 1x1x0.083's mass per metre" 17 build rows 1x1x0.083 mass_per_metre)
field("3.167 kg" "HSS 1-1/2x1-1/2x0.120's mass per metre" 17 build rows 1-1/2x1-1/2x0.120 mass_per_metre)
field("6096" "the inch tube's stock length (20 ft)" 17 build length stock 0)
field("test-hss-rect" "the inch rectangular tube" 18 id)
field("101.6 mm" "HSS 4x2x1/4's width" 18 build section width)
field("50.8 mm" "HSS 4x2x1/4's height" 18 build section height)
field("12.7 mm" "HSS 4x2x1/4's outer radius" 18 build tube outer_radius)
field("13.112 kg" "HSS 4x2x1/4's mass per metre" 18 build mass_per_metre)
field("3.167 kg" "HSS 2x1x0.120's mass per metre" 18 build rows 2x1x0.120 mass_per_metre)
# The typed size: its default row's block, its rows' own, and the area
# parameter the cut list reads.
field("test-typed-shs" "the tube of a typed size" 19 id)
field("A" "the typed tube's area parameter" 19 build area_parameter)
field("3.303 kg" "the typed tube's default (40x40x3) mass per metre" 19 build mass_per_metre)
field("20 mm" "100x100x8's outer radius (2.5 T)" 19 build rows 100x100x8 tube outer_radius)
field("12 mm" "100x100x8's inner radius (1.5 T)" 19 build rows 100x100x8 tube inner_radius)
field("21.385 kg" "100x100x8's mass per metre" 19 build rows 100x100x8 mass_per_metre)
frames(19 "end_a;end_b;face_xp;face_xn;face_yp;face_yn")

#    Their designs: the volume the exact area times the length, 1000 mm by
#    default, in every row; and within 1 % of the standards' masses (CHS
#    48.3x3.2 3.56 kg/m, 114.3x5 13.5; SHS 40x40x3 3.30, 100x100x10 25.6;
#    EN 10305-5 30x30x1.5 1.34, 35x35x3 2.86, 40x40x4 4.25) at 7850 kg/m3.
#    The round tube's axis (mount:axis F8) on Z; the square tube's face
#    frames follow its size (mount:face_xp F9). The furniture tube's rows
#    switch its bores: the rounded one (F4) suppressed at a sharp inner
#    corner, the sharp one (F6) at a rounded one; its face_xp is F11.
file(WRITE "${WORK}/chs.json" "[
  {\"expect\": {\"total_volume\": 453394.65177, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 453503.18471, \"tolerance\": 0.01}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"appearance\": \"steel_satin\", \"bbox\": {\"min\": [-24.15, -24.15, 0], \"max\": [24.15, 24.15, 1000]}}]}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F8\"}, \"result\": {\"origin\": [0, 0, 0], \"normal\": [0, 0, 1]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"114.3x5\"},
  {\"expect\": {\"total_volume\": 1716880.38519, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 1719745.22293, \"tolerance\": 0.01}},
  {\"cmd\": \"set_parameter\", \"name\": \"L\", \"expression\": \"500 mm\"},
  {\"expect\": {\"total_volume\": 858440.19259, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F7\"}, \"result\": {\"origin\": [0, 0, 500], \"normal\": [0, 0, 1]}}}
]")
cli(ok run "${WORK}/chs.json" --open "${library}/steel-tubes/test-chs.mitcad")
file(WRITE "${WORK}/shs.json" "[
  {\"expect\": {\"total_volume\": 420823.00165, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 420382.16561, \"tolerance\": 0.01}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-20, -20, 0], \"max\": [20, 20, 1000]}}]}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F9\"}, \"result\": {\"origin\": [20, 0, 0], \"normal\": [1, 0, 0], \"x_axis\": [0, 0, 1]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"100x100x10\"},
  {\"expect\": {\"total_volume\": 3256637.06143, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 3261146.49682, \"tolerance\": 0.01}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F9\"}, \"result\": {\"origin\": [50, 0, 0], \"normal\": [1, 0, 0]}}}
]")
cli(ok run "${WORK}/shs.json" --open "${library}/steel-tubes/test-shs.mitcad")
file(WRITE "${WORK}/furniture.json" "[
  {\"expect\": {\"total_volume\": 170517.14587, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 170700.63694, \"tolerance\": 0.01}},
  {\"expect\": {\"feature\": \"F4\", \"status\": \"suppressed\"}},
  {\"expect\": {\"feature\": \"F6\", \"status\": \"ok\"}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-15, -15, 0], \"max\": [15, 15, 1000]}}]}},
  {\"cmd\": \"apply_configuration\", \"name\": \"35x35x3\"},
  {\"expect\": {\"total_volume\": 364685.83471, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 364331.21019, \"tolerance\": 0.01}},
  {\"expect\": {\"feature\": \"F4\", \"status\": \"ok\"}},
  {\"expect\": {\"feature\": \"F6\", \"status\": \"suppressed\"}},
  {\"cmd\": \"apply_configuration\", \"name\": \"40x40x4\"},
  {\"expect\": {\"total_volume\": 541663.70615, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 541401.27389, \"tolerance\": 0.01}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F11\"}, \"result\": {\"origin\": [20, 0, 0], \"normal\": [1, 0, 0]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"30x30x1.5\"},
  {\"expect\": {\"total_volume\": 170517.14587, \"tolerance\": 1e-6}},
  {\"expect\": {\"feature\": \"F4\", \"status\": \"suppressed\"}}
]")
cli(ok run "${WORK}/furniture.json" --open "${library}/steel-tubes/test-furniture-tube.mitcad")
#    The rectangular tubes (mitcad#892): the volume the exact area times
#    the length (RHS 100x50x4 8.59 kg/m, 200x100x8 33.9; EN 10305-5 40x20x2
#    1.75, 35x25x3 2.39), the box of the width and height, the face frames
#    on half the width (face_xp F9) and half the height (face_yp F13); the
#    furniture tube's bores switched as the square one's, its face_yp F15.
file(WRITE "${WORK}/rhs.json" "[
  {\"expect\": {\"total_volume\": 1094796.44737, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 1094267.51592, \"tolerance\": 0.01}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-50, -25, 0], \"max\": [50, 25, 1000]}}]}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F9\"}, \"result\": {\"origin\": [50, 0, 0], \"normal\": [1, 0, 0]}}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F13\"}, \"result\": {\"origin\": [0, 25, 0], \"normal\": [0, 1, 0], \"x_axis\": [0, 0, 1]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"200x100x8\"},
  {\"expect\": {\"total_volume\": 4324247.71932, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 4318471.33758, \"tolerance\": 0.01}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F9\"}, \"result\": {\"origin\": [100, 0, 0], \"normal\": [1, 0, 0]}}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F13\"}, \"result\": {\"origin\": [0, 50, 0], \"normal\": [0, 1, 0]}}}
]")
cli(ok run "${WORK}/rhs.json" --open "${library}/steel-tubes/test-rhs.mitcad")
file(WRITE "${WORK}/rect-furniture.json" "[
  {\"expect\": {\"total_volume\": 223141.59265, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 222929.93631, \"tolerance\": 0.01}},
  {\"expect\": {\"feature\": \"F4\", \"status\": \"suppressed\"}},
  {\"expect\": {\"feature\": \"F6\", \"status\": \"ok\"}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-20, -10, 0], \"max\": [20, 10, 1000]}}]}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F15\"}, \"result\": {\"origin\": [0, 10, 0], \"normal\": [0, 1, 0]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"35x25x3\"},
  {\"expect\": {\"total_volume\": 304685.83471, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 304458.59873, \"tolerance\": 0.01}},
  {\"expect\": {\"feature\": \"F4\", \"status\": \"ok\"}},
  {\"expect\": {\"feature\": \"F6\", \"status\": \"suppressed\"}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F15\"}, \"result\": {\"origin\": [0, 12.5, 0], \"normal\": [0, 1, 0]}}}
]")
cli(ok run "${WORK}/rect-furniture.json" --open "${library}/steel-tubes/test-rect-furniture-tube.mitcad")
#    The inch tubes (mitcad#1328): the volume the exact area times the
#    length, within 1 % of the tables' nominal weights at 7850 kg/m3; the
#    box and the face frames in mm (face_xp F9, face_yp F13).
file(WRITE "${WORK}/hss.json" "[
  {\"expect\": {\"total_volume\": 1302474.71774, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 1302380.41940, \"tolerance\": 0.01}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-38.1, -38.1, 0], \"max\": [38.1, 38.1, 1000]}}]}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F9\"}, \"result\": {\"origin\": [38.1, 0, 0], \"normal\": [1, 0, 0]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"1x1x0.083\"},
  {\"expect\": {\"total_volume\": 184969.49804, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 185783.52417, \"tolerance\": 0.01}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F9\"}, \"result\": {\"origin\": [12.7, 0, 0]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"3x3x3/16\"},
  {\"expect\": {\"parameter\": \"T\", \"value\": 4.7625}}
]")
cli(ok run "${WORK}/hss.json" --open "${library}/steel-tubes/test-hss.mitcad")
file(WRITE "${WORK}/hss-rect.json" "[
  {\"expect\": {\"total_volume\": 1670350.60932, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 1670155.96724, \"tolerance\": 0.01}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-50.8, -25.4, 0], \"max\": [50.8, 25.4, 1000]}}]}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F13\"}, \"result\": {\"origin\": [0, 25.4, 0], \"normal\": [0, 1, 0]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"2x1x0.120\"},
  {\"expect\": {\"total_volume\": 403429.38839, \"tolerance\": 1e-6}},
  {\"expect\": {\"total_volume\": 403794.80252, \"tolerance\": 0.01}}
]")
cli(ok run "${WORK}/hss-rect.json" --open "${library}/steel-tubes/test-hss-rect.mitcad")
#    The typed size: an open table of B and T; a size typed (76.2 x 76.2 x
#    4.8, no row) gives the exact area with EN 10219-2's corners (2 T and
#    T up to 6 mm) and its area parameter; a preset over 6 mm the wider
#    corners (2.5 T and 1.5 T); a wall of a third of the side or more is
#    refused and nothing changes.
file(WRITE "${WORK}/typed.json" "[
  {\"expect\": {\"query\": {\"query\": \"configurations\"},
               \"result\": {\"configurations\": {\"options\": [\"B\", \"T\"], \"default\": \"40x40x3\"}}}},
  {\"expect\": {\"total_volume\": 420823.00165, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F9\"}, \"result\": {\"origin\": [20, 0, 0], \"normal\": [1, 0, 0]}}},
  {\"cmd\": \"configure\", \"values\": {\"B\": 76.2, \"T\": 4.8}},
  {\"expect\": {\"parameter\": \"B\", \"value\": 76.2}},
  {\"expect\": {\"parameter\": \"RO\", \"value\": 9.6}},
  {\"expect\": {\"parameter\": \"A\", \"value\": 1311.5468842161254}},
  {\"expect\": {\"total_volume\": 1311546.88422, \"tolerance\": 1e-6}},
  {\"expect\": {\"query\": {\"query\": \"bodies\", \"properties\": true},
               \"result\": [{\"bbox\": {\"min\": [-38.1, -38.1, 0], \"max\": [38.1, 38.1, 1000]}}]}},
  {\"expect\": {\"query\": {\"query\": \"datum\", \"uid\": \"F9\"}, \"result\": {\"origin\": [38.1, 0, 0]}}},
  {\"cmd\": \"apply_configuration\", \"name\": \"100x100x8\"},
  {\"expect\": {\"parameter\": \"RO\", \"value\": 20}},
  {\"expect\": {\"parameter\": \"RI\", \"value\": 12}},
  {\"expect\": {\"total_volume\": 2724247.71932, \"tolerance\": 1e-6}},
  {\"cmd\": \"configure\", \"values\": {\"B\": 40, \"T\": 3}},
  {\"expect\": {\"total_volume\": 420823.00165, \"tolerance\": 1e-6}},
  {\"cmd\": \"configure\", \"values\": {\"T\": 14}},
  {\"expect\": {\"parameter\": \"T\", \"value\": 3}},
  {\"expect\": {\"total_volume\": 420823.00165, \"tolerance\": 1e-6}}
]")
cli(ok run "${WORK}/typed.json" --open "${library}/steel-tubes/test-typed-shs.mitcad")

# 4. The pickers: fetched, timber only where asked for.
cli(ok library fetch "${library}")
cli(ok library candidates --role profile --kind timber)
expect("mitcad-parts/test-board" "the Timber picker")
foreach(id test-floorboard test-panel test-shiplap test-feather-edge test-deck)
  expect("mitcad-parts/${id}" "the Timber picker's profiled boards")
endforeach()
if(out MATCHES "test-chs|test-shs|test-furniture-tube|test-rhs|test-rect-furniture-tube|test-hss|test-typed-shs")
  message(FATAL_ERROR "the Timber picker lists steel tubes:\n${out}")
endif()
cli(ok library candidates --role profile)
if(out MATCHES "test-board|test-floorboard|test-shiplap|test-deck")
  message(FATAL_ERROR "the Profile picker lists timber:\n${out}")
endif()
# The steel tubes are profiles of no slot system (mitcad#873).
foreach(id test-chs test-shs test-furniture-tube test-rhs test-rect-furniture-tube test-hss test-hss-rect
           test-typed-shs)
  expect("mitcad-parts/${id}" "the Profile picker's steel tubes")
endforeach()
# BUILD places an inch tube as it places an EN tube (mitcad#1328): its
# face frame, its cut list's designation in inches and its mass, half
# the table's 6.87 lb/ft at 500 mm within 1 %. A tube of a typed size
# placed in a size typed and in a preset: each its own row and stock,
# its mass its exact area times 7850 kg/m3.
set(tube "{\"library\": \"mitcad-parts\", \"url\": \"${library}\", \"rev\": \"HEAD\"")
file(WRITE "${WORK}/place-hss.json" "[
  {\"cmd\": \"build_place\", \"part\": ${tube}, \"component\": \"test-hss\", \"config\": \"3x3x3/16\"},
   \"at\": {\"point\": [0, 0, 0]}, \"length\": 500},
  {\"expect\": {\"feature\": \"F1\", \"status\": \"ok\"}},
  {\"expect\": {\"query\": {\"query\": \"joint_frame\", \"occurrence\": \"O1\",
                          \"geometry\": {\"mount\": \"F1:mount(face_xp)\"}},
               \"result\": {\"component\": \"C1\"}}},
  {\"expect\": {\"query\": {\"query\": \"build_cut_list\"},
               \"result\": {\"rows\": [{\"designation\": \"HSS 3x3x3/16 L=500\", \"config\": \"3x3x3/16\",
                                       \"length\": 500, \"mass\": 5.1118}]},
               \"tolerance\": 0.01}}
]")
cli(ok run "${WORK}/place-hss.json")
file(WRITE "${WORK}/place-typed.json" "[
  {\"cmd\": \"build_place\", \"part\": ${tube}, \"component\": \"test-typed-shs\",
                                     \"parameters\": {\"B\": \"76.2 mm\", \"T\": \"4.8 mm\"}},
   \"at\": {\"point\": [0, 0, 0]}, \"length\": 500},
  {\"expect\": {\"feature\": \"F1\", \"status\": \"ok\"}},
  {\"cmd\": \"build_place\", \"part\": {\"source\": \"S1\"}, \"at\": {\"point\": [300, 0, 0]}, \"length\": 500},
  {\"expect\": {\"query\": {\"query\": \"build_cut_list\"},
               \"result\": {\"rows\": [{\"designation\": \"Square tube 40x40x3 L=500\", \"size\": \"B 40, T 3\",
                                       \"quantity\": 1, \"mass\": 1.651730},
                                      {\"designation\": \"Square tube 76.2x76.2x4.8 L=500\", \"size\": \"B 76.2, T 4.8\",
                                       \"quantity\": 1, \"mass\": 5.147822}],
                          \"stock\": [{\"needed\": 1}, {\"needed\": 1}]},
               \"tolerance\": 1e-5}}
]")
cli(ok run "${WORK}/place-typed.json")
cli(ok library candidates --role panel --mount face)
expect("mitcad-parts/test-sheet" "the sheets on a face")
cli(ok library candidates --role panel)
if(out MATCHES "test-sheet")
  message(FATAL_ERROR "the slot panels' picker lists sheets:\n${out}")
endif()
cli(ok library candidates --role fastener)
if(out MATCHES "test-screw|test-nail")
  message(FATAL_ERROR "the slot nuts' picker lists wood fasteners:\n${out}")
endif()
cli(ok library candidates --role fastener --kind wood)
expect("mitcad-parts/test-nail" "the wood fasteners (the nail)")
expect("mitcad-parts/test-screw" "the wood fasteners (the screw)")
# A 48 x 148 joist on a beam: the fasteners, the hanger and the bracket;
# the post base goes on no junction. A 48 x 123 joist: the hanger greyed.
file(WRITE "${WORK}/joist.json"
     "{\"id\": \"J1\", \"kind\": \"T\", \"wood\": {\"sections\": {\"a\": [148, 48], \"b\": [148, 48]}}}")
cli(ok library candidates --junction "${WORK}/joist.json")
foreach(id test-hanger test-bracket test-screw test-nail)
  expect("(^|\n)mitcad-parts/${id} " "${id} for the joist")
endforeach()
expect("not mitcad-parts/test-post-base" "the post base greyed at a junction")
file(WRITE "${WORK}/joist-123.json"
     "{\"id\": \"J1\", \"kind\": \"T\", \"wood\": {\"sections\": {\"a\": [123, 48], \"b\": [148, 48]}}}")
cli(ok library candidates --junction "${WORK}/joist-123.json")
expect("not mitcad-parts/test-hanger  Test joist hanger 48x148: it fits 48 × 148, the member is 123 × 48"
       "the hanger greyed for a 48 x 123 joist")

# 5. The production generator's real geometry seated in a frame (mitcad#512),
#    independently of its metadata: countersunk screw top z=0, tip L=50;
#    flat nail top z=-1.5, tip L=40, driven 1.5 deeper. No board machining.
set(members "")
foreach(i RANGE 1 5)
  list(APPEND members "{\"kind\": \"timber\", \"cuts\": [], \"machining\": []}")
endforeach()
string(JOIN "," members ${members})
foreach(spec IN ITEMS "test-screw|50" "test-nail|41.5")
  string(REPLACE "|" ";" fields "${spec}")
  list(GET fields 0 id)
  list(GET fields 1 tip)
  set(seats "")
  foreach(i RANGE 1 12)
    list(APPEND seats "{\"role\": \"fastener\", \"seating\": {\"head_top_distance\": 0, \"tip_distance\": ${tip}}}")
  endforeach()
  string(JOIN "," seats ${seats})
  file(WRITE "${WORK}/seat-${id}.json" "[
    {\"cmd\": \"build_frame\", \"kind\": \"wall\",
     \"part\": {\"library\": \"mitcad-parts\", \"url\": \"${library}\", \"rev\": \"HEAD\", \"component\": \"test-board\"},
     \"start\": [0, 0, 0], \"end\": [1200, 0, 0], \"height\": 600, \"spacing\": 600,
     \"fastener\": {\"library\": \"mitcad-parts\", \"url\": \"${library}\", \"rev\": \"HEAD\", \"component\": \"${id}\"}},
    {\"expect\": {\"query\": {\"query\": \"build_parts\"}, \"tolerance\": 1e-6,
                 \"result\": {\"parts\": [${members},${seats}]}}}
  ]")
  cli(ok run "${WORK}/seat-${id}.json")
endforeach()
