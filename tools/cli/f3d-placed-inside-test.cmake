# SPDX-License-Identifier: MIT
# An inserted design moved by a joint on a part inside it (mitcad#1892;
# never committed): a design of MITCAD_F3D_BACKUP found by its content id
# (the first 8 hex digits of its SHA-256), with the design it inserts in
# the reference folder beside it (`<design> vN.refs/`). A rigid joint ties
# a part inside the inserted design to the design's own body, and the file
# keeps the placements of the paths to the parts inside only: the import
# must take the inserted design's own placement from them, not one path's
# (it landed turned and away from the body it is jointed to).
#
# The design is imported with mitcad-cli import-f3d and saved; the inserted
# design's occurrence (the only one in the root component) must be where
# the file has it (the path to the part inside it that sits at the inserted
# design's origin: position within 0.01 mm, rotation within 1e-6), and
# every body of it must lie on the design's own side of the plane the joint
# holds it on. Skipped when the design is not there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<dir> -P f3d-placed-inside-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "f3d-placed-inside-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(design_id "7e0036a0")
# The inserted design's placement in the root component (rows, mm).
set(expected_rows
  "1 0 0 25.5"
  "0 0 1 0"
  "0 -1 0 27.6"
)

# The design under the folders of MITCAD_F3D_BACKUP (one folder or several
# separated by ':' or ';').
set(design "")
string(REPLACE ";" ":" folders "$ENV{MITCAD_F3D_BACKUP}")
string(REPLACE ":" ";" folders "${folders}")
foreach(folder IN LISTS folders)
  if(NOT IS_DIRECTORY "${folder}" OR NOT design STREQUAL "")
    continue()
  endif()
  file(GLOB_RECURSE files LIST_DIRECTORIES false "${folder}/*.f3d")
  list(SORT files)
  foreach(f IN LISTS files)
    file(SHA256 "${f}" sum)
    string(SUBSTRING "${sum}" 0 8 id)
    if(id STREQUAL design_id)
      set(design "${f}")
      break()
    endif()
  endforeach()
endforeach()
if(design STREQUAL "")
  message("f3d placed inside: skipped (no design ${design_id} in MITCAD_F3D_BACKUP)")
  return()
endif()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
execute_process(
  COMMAND "${CLI}" import-f3d "${design}" --save "${WORK}/design.mitcad"
  RESULT_VARIABLE rc OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT rc EQUAL 0)
  message(FATAL_ERROR "f3d placed inside: the import of ${design_id} failed (${rc}):\n${out}\n${err}")
endif()
# The design it inserts was read from the reference folder.
if(NOT out MATCHES "Placed bodies: ([0-9]+)" OR CMAKE_MATCH_1 LESS 3)
  message(FATAL_ERROR "f3d placed inside: ${design_id}: the inserted design's bodies are not placed:\n${out}")
endif()

# The occurrence in the root component.
file(READ "${WORK}/design.mitcad" text)
string(JSON count LENGTH "${text}" occurrences)
math(EXPR last "${count} - 1")
set(found "")
foreach(i RANGE ${last})
  string(JSON parent GET "${text}" occurrences ${i} parent)
  if(parent STREQUAL "C0")
    if(NOT found STREQUAL "")
      message(FATAL_ERROR "f3d placed inside: ${design_id}: more than one occurrence in the root component")
    endif()
    set(found ${i})
  endif()
endforeach()
if(found STREQUAL "")
  message(FATAL_ERROR "f3d placed inside: ${design_id}: no occurrence in the root component")
endif()

# A JSON number's text (as string(JSON) gives it: 25.5, -0.9999999999999998,
# 1.2246467991473532e-16) as an integer of billionths, cut towards zero.
function(billionths text out_var)
  if(NOT text MATCHES "^(-?)([0-9]+)(\.([0-9]*))?([eE]([-+]?)0*([0-9]+))?$")
    message(FATAL_ERROR "not a number: '${text}'")
  endif()
  set(sign "${CMAKE_MATCH_1}")
  set(digits "${CMAKE_MATCH_2}${CMAKE_MATCH_4}")
  string(LENGTH "${CMAKE_MATCH_4}" places)
  set(exponent 0)
  if("${CMAKE_MATCH_6}" STREQUAL "-")
    set(exponent "-${CMAKE_MATCH_7}")
  elseif(NOT "${CMAKE_MATCH_7}" STREQUAL "")
    set(exponent "${CMAKE_MATCH_7}")
  endif()
  math(EXPR shift "(${exponent}) - ${places} + 9")
  if(shift GREATER 12)
    message(FATAL_ERROR "too large a number: '${text}'")
  elseif(shift GREATER_EQUAL 0)
    string(REPEAT "0" ${shift} zeros)
    string(APPEND digits "${zeros}")
  else()
    math(EXPR drop "0 - (${shift})")
    string(LENGTH "${digits}" length)
    if(drop GREATER_EQUAL length)
      set(digits 0)
    else()
      math(EXPR keep "${length} - ${drop}")
      string(SUBSTRING "${digits}" 0 ${keep} digits)
    endif()
  endif()
  string(REGEX REPLACE "^0+([0-9])" "\\1" digits "${digits}")
  if(digits STREQUAL "0")
    set(sign "")
  endif()
  set(${out_var} "${sign}${digits}" PARENT_SCOPE)
endfunction()

# Each entry against the file's: the rotation within 1e-6, the position
# within 0.01 mm.
set(failed "")
foreach(r RANGE 2)
  list(GET expected_rows ${r} row)
  string(REPLACE " " ";" row "${row}")
  foreach(c RANGE 3)
    list(GET row ${c} want)
    string(JSON got GET "${text}" occurrences ${found} transform ${r} ${c})
    billionths("${got}" g)
    billionths("${want}" w)
    math(EXPR d "${g} - (${w})")
    if(d LESS 0)
      math(EXPR d "0 - (${d})")
    endif()
    if(c EQUAL 3)
      set(tolerance 10000000)
    else()
      set(tolerance 1000)
    endif()
    if(d GREATER tolerance)
      string(APPEND failed "\n  row ${r} column ${c}: ${got}, not ${want}")
    endif()
  endforeach()
endforeach()
if(NOT failed STREQUAL "")
  message(FATAL_ERROR "f3d placed inside: ${design_id}: the inserted design is not where the file places it:${failed}")
endif()

# The joint holds the inserted design on the plane 18 mm up, facing down
# (the design's own body below it): every body of the inserted design
# lies above that plane, the lowest of them on it (within 0.01 mm).
string(REGEX MATCH "Placed bodies:.*" placed "${out}")
string(REPLACE ";" "," placed "${placed}")
string(REPLACE "\n" ";" lines "${placed}")
set(inside_body FALSE)
set(lowest "")
foreach(line IN LISTS lines)
  if(line MATCHES "^  [^ /][^/]*:[0-9]+/")
    set(inside_body TRUE)
  elseif(line MATCHES "^  [^ ]")
    set(inside_body FALSE)
  elseif(inside_body AND line MATCHES "bounding box \\[[-0-9.]+, [-0-9.]+, ([-0-9.]+)\\]")
    billionths("${CMAKE_MATCH_1}" z)
    if(lowest STREQUAL "" OR z LESS lowest)
      set(lowest ${z})
    endif()
  endif()
endforeach()
if(lowest STREQUAL "")
  message(FATAL_ERROR "f3d placed inside: ${design_id}: no body of the inserted design placed:\n${out}")
endif()
math(EXPR d "${lowest} - 18000000000")
if(d LESS -10000000 OR d GREATER 10000000)
  message(FATAL_ERROR "f3d placed inside: ${design_id}: the inserted design's lowest point is not on the plane 18 mm up (${lowest} nm)")
endif()
message("f3d placed inside: ${design_id} where the file places it")
