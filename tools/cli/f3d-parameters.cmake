# SPDX-License-Identifier: MIT
# .f3d designs driven by their user parameters (mitcad#641): each found by
# its content id (the first 8 hex digits of its SHA-256) under the folders
# of MITCAD_F3D_CORPUS and MITCAD_F3D_BACKUP, imported with mitcad-cli
# import-f3d, its parameters changed and the design recomputed (mitcad-cli
# run --open). Per design:
#
# - every feature recomputes without an error, and without a warning it
#   did not have as imported (mitcad#891: projections that no longer
#   followed their source kept their last geometry);
# - the design has the bodies it had as imported, no more (mitcad#890: a
#   cut that left rings of a body as bodies of their own);
# - each body named has the bounding box given, within 0.001 mm, where
#   the expected box is derived from the design's own parameter
#   expressions; a body whose box grew has a larger volume than in the
#   design as imported, one whose box stayed the same volume (within 1e-6
#   of it and 1 mm3 of rounding);
# - where MITCAD_F3D_DESIGN_REF holds the source application's STEP export
#   of the change (`<id>/<change>.step`), every body is compared with it
#   too: volumes within 0.01 % and surfaces within 0.01 mm
#   (mitcad-cli compare-step).
#
# Skipped when none of the designs is found.
#
#   cmake -DCLI=<mitcad-cli> -DWORK=<dir> -P f3d-parameters.cmake

cmake_minimum_required(VERSION 3.24)

# <id>|<change name>|<parameter>=<value>,...|<body>=<min x>:<min y>:<min z>:
# <max x>:<max y>:<max z>,... where <body> is a body's name or #<n>, the
# n-th body of the design as imported (from 1), which keeps the names a
# design gives its bodies out of this file (mitcad#1802).
set(designs
    # A grid-unit chute of three bodies: a frame, a chute of stacked rings
    # and a catch (mitcad#641), by their order (the import names them as
    # the file does, mitcad#1802). Each grid cell (GridSize, 28 mm) more of
    # BaseX, DepthY and BaseZ makes the frame and the catch one cell wider
    # and deeper, and the chute one cell taller with one ring more per half
    # cell (ChuteElements = round(ChuteExtent / ChuteElementHeight)). The
    # catch stays where its free move puts it, 56 mm below the frame. The
    # boxes are derived from the expressions: no reference export of the
    # changes exists yet. Each parameter a cell more, then all three.
    "b35176e6|base-x|BaseX=5|#1=0:0:0:140:84:28,#2=1.8:1.8:21:138.2:82.2:119,#3=0:0:-56:140:84:-28"
    "b35176e6|base-z|BaseZ=6|#1=0:0:0:112:84:28,#2=1.8:1.8:21:110.2:82.2:147,#3=0:0:-56:112:84:-28"
    "b35176e6|depth-y|DepthY=4|#1=0:0:0:112:112:28,#2=1.8:1.8:21:110.2:110.2:119,#3=0:0:-56:112:112:-28"
    "b35176e6|one-cell-more|BaseX=5,BaseZ=6,DepthY=4|#1=0:0:0:140:112:28,#2=1.8:1.8:21:138.2:110.2:147,#3=0:0:-56:140:112:-28"
    # Three cells wider (mitcad#890, mitcad#891): the frame's offset
    # outlines had their tangent corners 2e-5 mm off where the lines touch
    # the arcs, the chamfers lost their edges and the projections of the
    # slot their source; on a square chute (four cells deep and high) the
    # sweep cut along those corners left four rings as bodies of their own.
    # Then back: the design as imported.
    "b35176e6|base-x-three-cells|BaseX=7|#1=0:0:0:196:84:28,#2=1.8:1.8:21:194.2:82.2:119,#3=0:0:-56:196:84:-28"
    "b35176e6|base-x-three-cells-square|BaseZ=4,DepthY=4,BaseX=7|#1=0:0:0:196:112:28,#2=1.8:1.8:21:194.2:110.2:91,#3=0:0:-56:196:112:-28"
    "b35176e6|base-x-three-cells-and-back|BaseX=7,BaseX=4|#1=0:0:0:112:84:28,#2=1.8:1.8:21:110.2:82.2:119,#3=0:0:-56:112:84:-28")

set(folders "")
foreach(variable MITCAD_F3D_CORPUS MITCAD_F3D_BACKUP)
  if(DEFINED ENV{${variable}})
    string(REPLACE ";" ":" value "$ENV{${variable}}")
    string(REPLACE ":" ";" value "${value}")
    foreach(folder IN LISTS value)
      if(IS_DIRECTORY "${folder}")
        list(APPEND folders "${folder}")
      endif()
    endforeach()
  endif()
endforeach()
if(NOT folders)
  message("f3d parameters: skipped (MITCAD_F3D_CORPUS and MITCAD_F3D_BACKUP name no folder)")
  return()
endif()

set(wanted "")
foreach(design IN LISTS designs)
  string(REPLACE "|" ";" fields "${design}")
  list(GET fields 0 id)
  list(APPEND wanted "${id}")
endforeach()
set(found_ids "")
foreach(folder IN LISTS folders)
  file(GLOB_RECURSE files "${folder}/*.f3d")
  list(SORT files)
  foreach(f IN LISTS files)
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
  message("f3d parameters: skipped (none of ${wanted} under ${folders})")
  return()
endif()

# The bodies of a design report (`run`/`info --json`): `<name>_volume`,
# `<name>_min`, `<name>_max` in the caller's scope, and `names`.
function(read_bodies json prefix)
  string(JSON count LENGTH "${json}" bodies)
  set(names "")
  if(count GREATER 0)
    math(EXPR last "${count} - 1")
    foreach(i RANGE ${last})
      string(JSON name GET "${json}" bodies ${i} name)
      string(JSON volume GET "${json}" bodies ${i} volume)
      list(APPEND names "${name}")
      set(${prefix}_${name}_volume "${volume}" PARENT_SCOPE)
      foreach(end min max)
        set(v "")
        foreach(k RANGE 2)
          string(JSON c GET "${json}" bodies ${i} bbox ${end} ${k})
          list(APPEND v "${c}")
        endforeach()
        set(${prefix}_${name}_${end} "${v}" PARENT_SCOPE)
      endforeach()
    endforeach()
  endif()
  set(${prefix}_names "${names}" PARENT_SCOPE)
endfunction()

# |a - b| <= tolerance, with CMake's integer arithmetic on nanometres.
function(near a b out)
  foreach(side a b)
    set(x "${${side}}")
    # Scientific notation (1E-15: below a nanometre) is zero.
    if(x MATCHES "[eE]")
      set(x 0)
    endif()
    set(sign 1)
    if(x MATCHES "^-")
      set(sign -1)
      string(SUBSTRING "${x}" 1 -1 x)
    endif()
    if(NOT x MATCHES "\\.")
      set(x "${x}.0")
    endif()
    string(REGEX REPLACE "^([0-9]*)\\.([0-9]*)$" "\\1" whole "${x}")
    string(REGEX REPLACE "^([0-9]*)\\.([0-9]*)$" "\\2" fraction "${x}")
    string(SUBSTRING "${fraction}000000" 0 6 fraction)
    if(whole STREQUAL "")
      set(whole 0)
    endif()
    string(REGEX REPLACE "^0+([0-9])" "\\1" fraction "${fraction}")
    math(EXPR um_${side} "${sign} * (${whole} * 1000000 + ${fraction})")
  endforeach()
  math(EXPR d "${um_a} - ${um_b}")
  if(d LESS 0)
    math(EXPR d "-${d}")
  endif()
  # The tolerance, 0.001 mm, in nanometres (1e-6 mm; the values are cut to
  # whole nanometres). The recomputed chute's side lies 0.00007 mm off its
  # derived place with all three parameters changed.
  if(d LESS_EQUAL 1000)
    set(${out} TRUE PARENT_SCOPE)
  else()
    set(${out} FALSE PARENT_SCOPE)
  endif()
endfunction()

file(MAKE_DIRECTORY "${WORK}")
set(problems "")
foreach(design IN LISTS designs)
  string(REPLACE "|" ";" fields "${design}")
  list(GET fields 0 id)
  list(GET fields 1 change)
  list(GET fields 2 sets)
  list(GET fields 3 expected)
  list(FIND found_ids "${id}" at)
  if(at EQUAL -1)
    message("f3d parameters: ${id} not found, not checked")
    continue()
  endif()
  # Each design is imported once, for all its changes.
  set(imported "${WORK}/${id}.mitcad")
  list(FIND imported_ids "${id}" done)
  if(done EQUAL -1)
    list(APPEND imported_ids "${id}")
    execute_process(COMMAND "${CLI}" import-f3d "${path_${id}}" --save "${imported}" --json
                    OUTPUT_FILE "${WORK}/${id}-import.json" ERROR_FILE "${WORK}/${id}-import.err"
                    RESULT_VARIABLE status)
    if(NOT status EQUAL 0)
      list(APPEND problems "${id}: import-f3d failed (${status})")
      list(APPEND failed_ids "${id}")
      continue()
    endif()
  endif()
  list(FIND failed_ids "${id}" failed)
  if(failed GREATER -1)
    continue()
  endif()
  execute_process(COMMAND "${CLI}" info "${imported}" --json OUTPUT_VARIABLE before RESULT_VARIABLE status)
  if(NOT status EQUAL 0)
    list(APPEND problems "${id}: info failed (${status})")
    continue()
  endif()
  read_bodies("${before}" before)
  # The change as a script of set_parameter commands.
  string(REPLACE "," ";" sets "${sets}")
  set(script "[")
  set(sep "")
  foreach(s IN LISTS sets)
    string(REGEX REPLACE "^([^=]+)=(.*)$" "\\1" name "${s}")
    string(REGEX REPLACE "^([^=]+)=(.*)$" "\\2" value "${s}")
    string(APPEND script "${sep}{\"cmd\": \"set_parameter\", \"name\": \"${name}\", \"value\": \"${value}\"}")
    set(sep ", ")
  endforeach()
  string(APPEND script "]")
  file(WRITE "${WORK}/${id}-${change}.script.json" "${script}")
  set(changed "${WORK}/${id}-${change}.mitcad")
  execute_process(COMMAND "${CLI}" run "${WORK}/${id}-${change}.script.json" --open "${imported}"
                          --save "${changed}" --json
                  OUTPUT_VARIABLE after ERROR_VARIABLE err RESULT_VARIABLE status)
  file(WRITE "${WORK}/${id}-${change}.json" "${after}")
  if(NOT status EQUAL 0)
    list(APPEND problems "${id} ${change}: run failed (${status}): ${err}")
    continue()
  endif()
  # Every feature recomputes without an error, and with no warning it did
  # not have as imported (a projection that no longer follows its source).
  set(warned "")
  string(JSON count LENGTH "${before}" timeline features)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON s GET "${before}" timeline features ${i} status)
    if(s STREQUAL "warning")
      string(JSON uid GET "${before}" timeline features ${i} uid)
      list(APPEND warned "${uid}")
    endif()
  endforeach()
  string(JSON count LENGTH "${after}" timeline features)
  math(EXPR last "${count} - 1")
  foreach(i RANGE ${last})
    string(JSON s GET "${after}" timeline features ${i} status)
    string(JSON uid GET "${after}" timeline features ${i} uid)
    list(FIND warned "${uid}" known)
    if(s STREQUAL "error" OR (s STREQUAL "warning" AND known EQUAL -1))
      string(JSON name GET "${after}" timeline features ${i} name)
      string(JSON e GET "${after}" timeline features ${i} error)
      list(APPEND problems "${id} ${change}: ${name} (${s}): ${e}")
    endif()
  endforeach()
  read_bodies("${after}" after)
  # The design's bodies, no more: a cut that leaves pieces of a body as
  # bodies of their own (mitcad#890) is no change of size.
  if(NOT after_names STREQUAL before_names)
    string(REPLACE ";" ", " got "${after_names}")
    string(REPLACE ";" ", " was "${before_names}")
    list(APPEND problems "${id} ${change}: the bodies are ${got}, not ${was}")
  endif()
  string(REPLACE "," ";" expected "${expected}")
  foreach(body IN LISTS expected)
    string(REGEX REPLACE "^([^=]+)=(.*)$" "\\1" name "${body}")
    string(REGEX REPLACE "^([^=]+)=(.*)$" "\\2" box "${body}")
    string(REPLACE ":" ";" box "${box}")
    # #<n>: the n-th body as imported, by its name there.
    if(name MATCHES "^#([0-9]+)$")
      math(EXPR k "${CMAKE_MATCH_1} - 1")
      list(LENGTH before_names n)
      if(k LESS 0 OR NOT k LESS n)
        list(APPEND problems "${id} ${change}: no body ${name} (${n} bodies)")
        continue()
      endif()
      list(GET before_names ${k} name)
    endif()
    list(FIND after_names "${name}" at)
    if(at EQUAL -1)
      list(APPEND problems "${id} ${change}: no body ${name} (bodies: ${after_names})")
      continue()
    endif()
    set(got ${after_${name}_min} ${after_${name}_max})
    set(was ${before_${name}_min} ${before_${name}_max})
    set(grew FALSE)
    foreach(k RANGE 5)
      list(GET box ${k} want)
      list(GET got ${k} have)
      near("${want}" "${have}" ok)
      if(NOT ok)
        list(APPEND problems "${id} ${change}: ${name}'s box is ${got}, expected ${box}")
        break()
      endif()
      list(GET was ${k} old)
      near("${want}" "${old}" same)
      if(NOT same)
        set(grew TRUE)
      endif()
    endforeach()
    set(v1 "${before_${name}_volume}")
    set(v2 "${after_${name}_volume}")
    if(grew AND NOT v2 GREATER v1)
      list(APPEND problems "${id} ${change}: ${name}'s volume ${v2} is not larger than ${v1}")
    elseif(NOT grew)
      if(v2 GREATER v1)
        set(hi "${v2}")
        set(lo "${v1}")
      else()
        set(hi "${v1}")
        set(lo "${v2}")
      endif()
      string(REGEX REPLACE "\\..*" "" hi_whole "${hi}")
      string(REGEX REPLACE "\\..*" "" lo_whole "${lo}")
      # In whole cubic millimetres (CMake's integers): at most 1e-6 of the
      # volume and the rounding apart.
      math(EXPR apart "${hi_whole} - ${lo_whole}")
      math(EXPR allowed "${hi_whole} / 1000000 + 1")
      if(apart GREATER allowed)
        list(APPEND problems "${id} ${change}: ${name}'s volume ${v2} is not ${v1}")
      endif()
    endif()
  endforeach()
  # The reference export of the change, where there is one.
  if(DEFINED ENV{MITCAD_F3D_DESIGN_REF} AND EXISTS "$ENV{MITCAD_F3D_DESIGN_REF}/${id}/${change}.step")
    execute_process(COMMAND "${CLI}" compare-step "${changed}" "$ENV{MITCAD_F3D_DESIGN_REF}/${id}/${change}.step"
                            --max-relative 0.0001 --max-deviation 0.01
                    OUTPUT_VARIABLE out ERROR_VARIABLE err RESULT_VARIABLE status)
    if(NOT status EQUAL 0)
      list(APPEND problems "${id} ${change}: differs from the reference export:\n${out}${err}")
    endif()
    message("f3d parameters: ${id} ${change}: compared with the reference export")
  endif()
  message("f3d parameters: ${id} ${change}: bodies ${after_names}")
endforeach()
if(problems)
  string(REPLACE ";" "\n  " problems "${problems}")
  message(FATAL_ERROR "f3d parameters:\n  ${problems}")
endif()
