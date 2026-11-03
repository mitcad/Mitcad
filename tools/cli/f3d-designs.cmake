# SPDX-License-Identifier: MIT
# Single .f3d designs checked item by item (mitcad#311): each found by its
# content id (the first 8 hex digits of its SHA-256) under the folders of
# MITCAD_F3D_CORPUS and MITCAD_F3D_BACKUP (each one folder or several
# separated by ':' or ';') and the folders beside them whose names end in
# `-reported` (files reported with import faults, kept apart; mitcad#1175),
# imported with mitcad-cli import-f3d with the import trace on. Per design:
#
# - the items named parametric come in parametric, and only the items
#   named as falling back fall back (the others of the file's own base
#   features and of placing occurrences aside);
# - no parametric item went through a retry on the file's exact state,
#   and every modelling input was read, none inferred from the history
#   (but those of the items a design names as inferred, a fourth field);
# - reading the file and applying its appearances take less time than
#   replaying its items (the appearances made a design of hundreds of face
#   appearances 16 times slower to open: 25 s reading them and 159 s
#   applying them against 8 s of replay);
# - where a design gives a time limit (a fifth field, seconds in a release
#   build, SLOWER times that in a debug build), the import takes no longer
#   (mitcad#642): about ten times its usual time, so that only a large
#   regression fails, as the recompute speed tests do;
# - where a design gives a volume tolerance (a sixth field, relative), every
#   body stored in the file has a body in the import whose volume is within
#   it, and every body the file hides is hidden (mitcad#1175).
#
# With SLOWER the imports also try an item from its own start, the route
# tried first, SLOWER times as long (mitcad-cli import-f3d
# --own-start-time-scale, mitcad#847): an item that gives its state that
# way hangs on that time (30 s, half of the item's), and a debug build's
# path pattern that takes 18 s by itself ran past it when the designs
# started at once beside the rest of the corpus step on a busy machine.
# The item's other routes keep their times: the items that fall back by
# design spend each of them in full, and with all of them scaled a sweep
# that passes over itself took 362 s instead of 95 s.
#
# Skipped when none of the designs is found.
#
# With RUNNER (mitcad_run_parallel) each design is checked in a child
# process of its own, several at once (MITCAD_CORPUS_JOBS, not 1; the
# longest first by the designs' own times when the test runner gives them,
# mitcad#832); each child is this script with DESIGN and DESIGN_FILE. A
# child may commit the larger of MITCAD_CORPUS_MEMORY and 4G (a design's
# import has taken over 2G) and has no time limit of its own (the designs'
# limits above and ctest's TIMEOUT hold); each child gets SLOWER, for
# its time limit and its own-start time. (With the designs at once, a
# thread's sizing offset ran past its 2 s of wall time beside the others
# and the thread took another route, on whose faces db73863f's Fillet4
# failed. Since mitcad#874 the import's time limits are on a clock that
# leaves the load out, the offset's 2 s SLOWER times as long, and a
# definition cut short at its share goes on on its worker.)
#
#   cmake -DCLI=<mitcad-cli> -DWORK=<dir> [-DSLOWER=<factor>] [-DRUNNER=<mitcad_run_parallel>]
#         -P f3d-designs.cmake

cmake_minimum_required(VERSION 3.24)

# <id>|<items parametric, comma separated>|<items that fall back>[|<items
# whose inputs may be inferred>[|<time limit, seconds in a release build>[|<volume
# tolerance of the final bodies, relative>]]]; an
# item may be named by its place on the timeline, `#<index>` (in every list
# since mitcad#719: names a user gave items stay out of this file).
set(designs
    # A circuit board with a pole round it (150 items, 78 occurrences):
    # its fillet between the pole's chamfer and a band 0.3 mm high where
    # flats cut the pole runs over the flats: built with them filled and cut
    # out again (mitcad#311). Nothing falls back.
    "e3c34193|Revolve1,Revolve2,Revolve3,Fillet5,Extrude8,Extrude11,Sketch12,Plane1,Fillet9,Fillet10|"
    # An assembly of a housing, a plug screwed into it, a nut and a tool
    # (145 items, 7 components; mitcad#379): the plug's thread found its
    # angle in the plug's own bodies, not in the housing's internal thread
    # round it. The revolution cut into the threaded bore is parametric: the
    # bore's cylinders, which the file keeps as cones of 1e-13 rad, are read
    # as cylinders (mitcad#389). A rounding along the thread (item 109) is
    # parametric since the walk past a seam of mitcad#235. The housing's
    # corner rounded with 20 (item 74) and the rounding at the thread's
    # chamfer (item 142) are parametric since the port's patches 0720 and
    # 0721 (mitcad#343). The foot of a post under a block (item 120)
    # takes its circle's arcs from the one edge the file names, its tangent
    # chain no longer running on into the block's convex long edges where
    # they touch the circle (mitcad#439). The thread's rounding up to the
    # groove's floor (item 27) is parametric with port patch 0803 and the
    # faster measurement against the flanks' splines (mitcad#442). The
    # rounding of a key's two lens ends (item 137) is parametric since the
    # port's patches 0821 and 0822 (mitcad#343); its inputs count as
    # inferred, as the file lost one of its four edge references, which is
    # left out. The rounding of the thread's chamfer cone down to the root
    # (item 143) is parametric since the port's patches 0860 and 0861
    # (mitcad#441). The rounding along the tetracore's closed chain of 36
    # edges (item 88) is parametric since the port's patch 0870 (mitcad#343).
    # The rounding of the thread's crest on the tetracore (item 133) is
    # parametric since the thread lies on screw surfaces and a rounding
    # walks its pieces as one face; item 143 keeps its corner with port
    # patch 0910 (mitcad#343). Falling back: a sweep that passes over
    # itself, and an extrusion whose profile's curve the sketch no longer
    # has.
    "db73863f|Thread1,Revolve4,Fillet6|Sweep1,Extrude3|Axis1,C-Pattern1,Extrude3,Extrude6,Fillet4,Fillet5,GeometricRelationship1,Plane1,Plane2,Plane3,Thread1,Thread2"
    # A grid-unit chute (27 items; mitcad#641): a base, a frame, a chute of
    # stacked rings and a catch, every size a count of grid cells. The
    # pastes and the derive keep their bodies in base features of their
    # own, the frame's slots are patterned from a paste, an align places
    # the derived slot, the rings' count is a parameter's and their pattern
    # runs against its axis, and an extrusion starts on a face. Falling
    # back: only the derive (item 2) of a design the file does not hold,
    # static in the source too. Inferred: the align's motion (its geometry
    # is not decoded), the slot's thin cut (its profile's sketch is not
    # decoded: the last sketch before it) and a cut of faces up to an
    # object (the side the history settles).
    "b35176e6|Extrude3,Align1,CopyPasteBodies1,R-Pattern4,Combine2,Fillet7,Extrude4,Extrude5,Chamfer2,Extrude8,Sweep4,R-Pattern3,Extrude9,Extrude10,Extrude11,CopyPasteBodies2,Move5,Extrude14,Extrude15,Chamfer3|#2|Align1,Extrude4,Extrude11"
    # A bin with a pattern of 372 ribs along a path (21 items, mitcad#642):
    # a stored body was healed from its base, the states of all the
    # pattern's copies built in a chain (4 359 s to open); the face merge of
    # its cut by an extrusion going down was invalid (port patch 1040, then
    # Extrude9 and the rounding Fillet4 after it come in parametric); healing
    # its large stored bodies searched their long wires' edges by number
    # (port patch 1050). The extrusion before the pattern looks ahead no
    # further than its own state (it built two of the copies' states), the
    # faces' measures skip the faces far from a change, a cut down the wall
    # that lies on its state by its faces tries no other definitions, and
    # the pattern, which does not translate, is not tried again from its
    # last copy but one (mitcad#658). The last cut keeps the combined body's
    # 18 lumps together and is parametric. Reading it converts only the
    # bodies of the states the replay asks for, not those of the copies'
    # 372 states (mitcad#675). It opens in about 44 s in a release build
    # (37 s while the pattern fell back);
    # the limit catches the chain of states coming back.
    # Combine2 omits 31 stale selections of copies the file no longer has,
    # with inferred inputs; the other 17 are kept as its tools and their
    # consumed history bodies are retired (mitcad#674). The pattern is
    # parametric (mitcad#673): its path decoded at the right place, the
    # original placed on the closed outline by its ribs' centre, the copies
    # turned along it and joined to the body as the file keeps one.
    # Nothing falls back.
    "b832916a|Extrude8,Fillet3,PathPattern3,Extrude9,Extrude10,Extrude11,Extrude12,Chamfer1,Fillet4,Extrude18,Extrude19,Fillet5,Chamfer6,R-Pattern3,Combine2,Extrude20||Extrude7,Combine2|375"
    # A drum of six components written by a 2026 writer (68 items,
    # mitcad#719): its parameters (class version 10) give the sizes, the
    # tapped holes' modelled threads come in on their own states and are
    # repeated by the pattern of the holes, holes are placed at their
    # distances from two edges or on a shaft at a distance from its end, a
    # pattern of faces (#24) takes the rounding added after its faces were
    # selected (inferred inputs), and
    # patterns about arcs and edges go the stored axis' way round. The
    # shaft's and the lid's holes tapped with taper pipe threads (NPT, ISO
    # 7-1 Rc) come in with their tapered bores, and the patterns of the
    # shaft's are parametric: the row pattern (#32) copies plain bores, as
    # the file does, the holes' threads left out (inferred), the circular
    # pattern (C-Pattern8) tapered copies (mitcad#733). The modelled
    # threads take the angles their states show, and the patterns' copies
    # turn with them. Nothing falls back.
    # The holes whose inputs are not all read (unknown) are named with the
    # inferred ones; the items the user named are given by their places.
    "3282222f|Extrude1,Extrude2,Extrude3,Extrude4,Extrude5,Extrude9,Extrude10,Hole1,Hole2,Hole3,Hole5,Hole6,Hole7,Hole8,C-Pattern2,C-Pattern6,#24,#32,C-Pattern8,#38,#39,#41,#42,#43,#44,#46,#48,#49,#50,#51,#53,#54,#55,#56,#57||Plane2,Plane3,Hole2,Hole3,Hole5,Hole6,Hole7,Hole8,#24,#32,#41,#43,#49"
    # A reported drill index (27 items, mitcad#1175): a blank cut and pasted
    # into its own component, which the file keeps as the blank (hidden)
    # and a copy of it, then 15 holes in the copy's flange, a boss with
    # embossed sizes and holes. The paste comes in as a copy in place (no
    # base feature before the holes), the holes parametric, the hidden blank
    # hidden, and both bodies have the stored volumes. The holes' places
    # are where the history removes material (inferred). The sizes run
    # round a circle below it (the text-path constraint's placements,
    # mitcad#1177): the sketch (#6) gives the text along its path, and the
    # emboss (#24) selects the text's 59 letters (read), with no base
    # feature after it.
    "528e5029|#5,#6,#7,#8,#9,#10,#11,#12,#13,#14,#15,#16,#17,#18,#19,#20,#21,#24,#25,#26||#7,#8,#9,#10,#11,#12,#13,#14,#15,#16,#17,#18,#19,#20,#21,#25,#26||1e-6")

# Seconds as the trace gives them (two decimals) in hundredths.
function(hundredths seconds out)
  string(REPLACE "." "" digits "${seconds}")
  string(REGEX REPLACE "^0+([0-9])" "\\1" digits "${digits}")
  set(${out} "${digits}" PARENT_SCOPE)
endfunction()

# A size as MITCAD_CORPUS_MEMORY gives it (bytes, or with K, M or G) in
# MiB; 0 for none.
function(size_mib text out)
  string(TOUPPER "${text}" text)
  if(text MATCHES "^([0-9]+)([KMG]?)I?B?$")
    set(value "${CMAKE_MATCH_1}")
    if(CMAKE_MATCH_2 STREQUAL "G")
      math(EXPR value "${value} * 1024")
    elseif(CMAKE_MATCH_2 STREQUAL "K")
      math(EXPR value "${value} / 1024")
    elseif(CMAKE_MATCH_2 STREQUAL "")
      math(EXPR value "${value} / 1048576")
    endif()
    set(${out} "${value}" PARENT_SCOPE)
  else()
    set(${out} 0 PARENT_SCOPE)
  endif()
endfunction()

set(folders "")
if(DEFINED DESIGN)
  # A child of the parallel run: the one design, its file found already.
  set(found_ids "${DESIGN}")
  set(path_${DESIGN} "${DESIGN_FILE}")
endif()
foreach(variable MITCAD_F3D_CORPUS MITCAD_F3D_BACKUP)
  if(DEFINED ENV{${variable}})
    string(REPLACE ";" ":" value "$ENV{${variable}}")
    string(REPLACE ":" ";" value "${value}")
    foreach(folder IN LISTS value)
      if(IS_DIRECTORY "${folder}")
        list(APPEND folders "${folder}")
        # The reported files beside it (mitcad#1175).
        get_filename_component(parent "${folder}" DIRECTORY)
        file(GLOB reported LIST_DIRECTORIES true "${parent}/*-reported")
        foreach(r IN LISTS reported)
          if(IS_DIRECTORY "${r}")
            list(APPEND folders "${r}")
          endif()
        endforeach()
      endif()
    endforeach()
  endif()
endforeach()
if(folders)
  list(REMOVE_DUPLICATES folders)
endif()
if(NOT folders AND NOT DEFINED DESIGN)
  message("f3d designs: skipped (MITCAD_F3D_CORPUS and MITCAD_F3D_BACKUP name no folder)")
  return()
endif()

# The designs' ids, then each file's id until all are found.
set(wanted "")
foreach(design IN LISTS designs)
  string(REPLACE "|" ";" fields "${design}")
  list(GET fields 0 id)
  list(APPEND wanted "${id}")
endforeach()
if(DEFINED DESIGN)
  set(folders "")
else()
  set(found_ids "")
endif()
foreach(folder IN LISTS folders)
  file(GLOB_RECURSE files "${folder}/*.f3d")
  list(SORT files)
  foreach(f IN LISTS files)
    # (A name with a semicolon falls apart in the list: not looked at.)
    if(NOT EXISTS "${f}" OR IS_DIRECTORY "${f}")
      continue()
    endif()
    file(SHA256 "${f}" sum)
    string(SUBSTRING "${sum}" 0 8 id)
    list(FIND wanted "${id}" at)
    list(FIND found_ids "${id}" seen)
    if(at GREATER -1 AND seen EQUAL -1)
      list(APPEND found_ids "${id}")
      set(path_${id} "${f}")
    endif()
  endforeach()
endforeach()
if(NOT found_ids)
  message("f3d designs: skipped (none of ${wanted} under ${folders})")
  return()
endif()

file(MAKE_DIRECTORY "${WORK}")
set(jobs "$ENV{MITCAD_CORPUS_JOBS}")
if(DEFINED RUNNER AND NOT DEFINED DESIGN AND NOT jobs STREQUAL "1")
  # Each design in a child process of its own; the runner prints their
  # lines in this order and "FAILED <id>: ..." for a child that failed.
  set(memory "$ENV{MITCAD_CORPUS_MEMORY}")
  if(NOT memory STREQUAL "none" AND NOT memory STREQUAL "0")
    size_mib("${memory}" memory)
    if(memory LESS 4096)
      set(memory 4096)
    endif()
    set(memory "${memory}M")
  endif()
  set(list "${WORK}/designs.txt")
  file(WRITE "${list}" "")
  set(not_found "")
  foreach(design IN LISTS designs)
    string(REPLACE "|" ";" fields "${design}")
    list(GET fields 0 id)
    list(FIND found_ids "${id}" at)
    if(at EQUAL -1)
      list(APPEND not_found "${id}")
      continue()
    endif()
    set(slower "")
    if(DEFINED SLOWER)
      set(slower "\t-DSLOWER=${SLOWER}")
    endif()
    file(APPEND "${list}" "${id}\t${CMAKE_COMMAND}\t-DCLI=${CLI}\t-DWORK=${WORK}${slower}\t-DDESIGN=${id}\t"
                          "-DDESIGN_FILE=${path_${id}}\t-P\t${CMAKE_CURRENT_LIST_FILE}\n")
  endforeach()
  foreach(id IN LISTS not_found)
    message("f3d designs: ${id} not found, not checked")
  endforeach()
  execute_process(COMMAND "${RUNNER}" --memory "${memory}" --file-timeout 0 "${list}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE out)
  string(REGEX REPLACE "\n$" "" shown "${out}")
  message("${shown}")
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "f3d designs: a design failed (FAILED lines above), or ${RUNNER} could not run them "
                        "(exit ${status})")
  endif()
  return()
endif()
set(problems "")
foreach(design IN LISTS designs)
  string(REPLACE "|" ";" fields "${design}")
  list(GET fields 0 id)
  list(GET fields 1 parametric)
  list(GET fields 2 falling)
  set(inferred "")
  set(limit "")
  set(volumes "")
  list(LENGTH fields field_count)
  if(field_count GREATER 3)
    list(GET fields 3 inferred)
  endif()
  if(field_count GREATER 4)
    list(GET fields 4 limit)
    if(limit AND DEFINED SLOWER)
      math(EXPR limit "${limit} * ${SLOWER}")
    endif()
  endif()
  if(field_count GREATER 5)
    list(GET fields 5 volumes)
  endif()
  string(REPLACE "," ";" parametric "${parametric}")
  string(REPLACE "," ";" falling "${falling}")
  string(REPLACE "," ";" inferred "${inferred}")
  list(FIND found_ids "${id}" at)
  if(at EQUAL -1)
    if(NOT DEFINED DESIGN)
      message("f3d designs: ${id} not found, not checked")
    endif()
    continue()
  endif()
  set(own_start "")
  if(DEFINED SLOWER)
    set(own_start --own-start-time-scale "${SLOWER}")
  endif()
  string(TIMESTAMP started "%s")
  execute_process(
    COMMAND "${CMAKE_COMMAND}" -E env MITCAD_IMPORT_TRACE=1
            "${CLI}" import-f3d "${path_${id}}" --report "${WORK}/${id}.json" ${own_start}
    OUTPUT_FILE "${WORK}/${id}.txt" ERROR_FILE "${WORK}/${id}.trace" RESULT_VARIABLE status)
  string(TIMESTAMP ended "%s")
  math(EXPR took "${ended} - ${started}")
  if(limit AND took GREATER limit)
    list(APPEND problems "${id}: the import took ${took} s, more than its limit of ${limit} s")
  endif()
  if(NOT status EQUAL 0)
    list(APPEND problems "${id}: import-f3d failed (${status})")
    continue()
  endif()
  file(READ "${WORK}/${id}.json" report)
  string(JSON count LENGTH "${report}" designs 0 items)
  math(EXPR last "${count} - 1")
  set(fell "")
  foreach(i RANGE ${last})
    string(JSON name GET "${report}" designs 0 items ${i} name)
    string(JSON type GET "${report}" designs 0 items ${i} type)
    string(JSON outcome GET "${report}" designs 0 items ${i} outcome)
    string(JSON retry ERROR_VARIABLE retry_missing GET "${report}" designs 0 items ${i} exact_retry)
    string(JSON provenance ERROR_VARIABLE provenance_missing GET "${report}" designs 0 items ${i}
           input_provenance)
    string(JSON index GET "${report}" designs 0 items ${i} index)
    list(FIND parametric "${name}" named)
    if(named EQUAL -1)
      list(FIND parametric "#${index}" named)
    endif()
    if(named GREATER -1 AND NOT outcome STREQUAL "parametric")
      list(APPEND problems "${id}: ${name} ${outcome}, not parametric")
    endif()
    if(outcome STREQUAL "fallback" AND NOT type STREQUAL "BaseFeature")
      # An item whose name is not for the list (a comma in it, or the name
      # of another design) is named by its place on the timeline, `#<i>`.
      list(FIND falling "#${index}" by_index)
      if(by_index GREATER -1)
        list(APPEND fell "#${index}")
      else()
        list(APPEND fell "${name}")
      endif()
    endif()
    if(outcome STREQUAL "parametric" AND NOT retry_missing)
      list(APPEND problems "${id}: ${name} is parametric after a retry on the file's exact state")
    endif()
    list(FIND inferred "${name}" may_infer)
    if(may_infer EQUAL -1)
      list(FIND inferred "#${index}" may_infer)
    endif()
    if(outcome STREQUAL "parametric" AND NOT provenance_missing AND NOT provenance STREQUAL "read"
       AND may_infer EQUAL -1)
      list(APPEND problems "${id}: ${name}'s inputs ${provenance}, not read")
    endif()
  endforeach()
  if(NOT fell STREQUAL falling)
    list(APPEND problems "${id}: falling back: '${fell}', expected '${falling}'")
  endif()
  # The final bodies against those stored in the file, and the bodies the
  # file hides (mitcad#1175).
  if(volumes)
    string(JSON body_count LENGTH "${report}" designs 0 bodies)
    if(body_count EQUAL 0)
      list(APPEND problems "${id}: no stored body compared")
    else()
      math(EXPR last_body "${body_count} - 1")
      foreach(b RANGE ${last_body})
        string(JSON file GET "${report}" designs 0 bodies ${b} file)
        string(JSON dv ERROR_VARIABLE dv_missing GET "${report}" designs 0 bodies ${b} volume_difference)
        if(dv_missing)
          list(APPEND problems "${id}: the stored body ${file} has no body in the import")
          continue()
        endif()
        string(REGEX REPLACE "^-" "" dv_abs "${dv}")
        if(dv_abs GREATER volumes)
          list(APPEND problems "${id}: the body of ${file} differs in volume by ${dv} (relative), more than ${volumes}")
        endif()
      endforeach()
    endif()
    string(JSON hidden ERROR_VARIABLE hidden_missing GET "${report}" designs 0 appearances hidden)
    string(JSON hidden_set ERROR_VARIABLE hidden_set_missing GET "${report}" designs 0 appearances hidden_set)
    if(hidden_missing)
      set(hidden 0)
    endif()
    if(hidden_set_missing)
      set(hidden_set 0)
    endif()
    if(NOT hidden EQUAL hidden_set)
      list(APPEND problems "${id}: ${hidden_set} of the file's ${hidden} hidden bodies hidden")
    endif()
  endif()
  # The phases' times from the trace.
  file(STRINGS "${WORK}/${id}.trace" reading REGEX "file read: inputs found in [0-9.]+ s")
  file(STRINGS "${WORK}/${id}.trace" items REGEX "^import: \\[[0-9.]+\\] item [0-9]+:")
  file(STRINGS "${WORK}/${id}.trace" faces REGEX "^import: appearance of face .* in [0-9.]+ s$")
  if(NOT reading OR NOT items)
    list(APPEND problems "${id}: the trace has no reading time or no items")
    continue()
  endif()
  list(GET reading 0 reading)
  string(REGEX REPLACE ".* in ([0-9.]+) s.*" "\\1" reading "${reading}")
  list(GET items -1 replay)
  string(REGEX REPLACE "^import: \\[([0-9.]+)\\].*" "\\1" replay "${replay}")
  set(applying 0)
  foreach(line IN LISTS faces)
    string(REGEX REPLACE ".* in ([0-9.]+) s$" "\\1" s "${line}")
    hundredths("${s}" s)
    math(EXPR applying "${applying} + ${s}")
  endforeach()
  hundredths("${reading}" reading_cs)
  hundredths("${replay}" replay_cs)
  math(EXPR beside "${reading_cs} + ${applying}")
  message("f3d designs: ${id}: ${took} s in all, reading ${reading} s, replay ${replay} s, appearances "
          "${applying} cs; falling back: ${fell}")
  if(beside GREATER replay_cs)
    list(APPEND problems "${id}: reading (${reading} s) and the appearances (${applying} cs) took longer than the replay (${replay} s)")
  endif()
endforeach()
if(problems)
  string(REPLACE ";" "\n  " problems "${problems}")
  message(FATAL_ERROR "f3d designs:\n  ${problems}")
endif()
