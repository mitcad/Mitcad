# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A rounding as large as the side faces beside its edges (GitHub #353,
# mitcad#1819), or whose rounded corners meet (GitHub #403, mitcad#1954), on
# the reporter's designs (never committed), and an edge of such a body
# rounded or bevelled past the flat faces at its ends (GitHub #566,
# mitcad#2229): their archives in `github-353/`, `github-375/`,
# `github-403/` and `github-566/` under MITCAD_MITCAD_REPORTED, found by their
# content ids (the first 8 hex digits of the SHA-256), each design in its
# archive by its own, unpacked into WORK. Each is opened by mitcad-cli run
# with its script (fillet_exact_reported*.json):
# the design's rounding is built at its size, with no warning or
# information, or refused in words where the script says so. GitHub #557's
# design (`github-557/`, mitcad#2279) has its rounding turned into a Full
# Round, whose radius is worked out from the faces. GitHub #536's
# (`github-536/`, mitcad#2312): the four upright edges of the stepped block
# rounded at 5 mm, by the reporter's two Fillet features and by the same
# edges in one feature (a design made from the reporter's, stored beside the
# archives as it is, its archive given as `-` and the design copied into
# WORK); both scripts check the same exact geometry, so the one feature
# builds what the two build. Skipped when the folder or every archive is not
# there.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPTS=<tools/cli/tests> -DWORK=<folder> -P fillet-exact-reported-test.cmake

foreach(var CLI SCRIPTS WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "fillet-exact-reported-test.cmake needs -D${var}=...")
  endif()
endforeach()

# Each case: the report's folder, the reporter's archive, the design in it
# (content ids) and the script that opens it.
set(cases
  "github-353:2cd84339:4bde7722:fillet_exact_reported.json" # four edges
  "github-353:b7522a40:39960088:fillet_exact_reported2.json" # eight edges
  "github-353:39896406:d2414f5a:fillet_exact_reported3.json" # a cube, 4.999
  "github-353:39896406:d2414f5a:fillet_exact_reported3_half.json" # the cube at 5
  "github-375:92354767:38e97e9a:fillet_exact_reported375.json" # then a chamfer
  "github-375:db2f0aa4:81de8088:fillet_exact_reported375b.json" # the inner corner, a chamfer (mitcad#2068)
  "github-403:253bc9b5:7f5fa5c4:fillet_exact_reported403_ball.json" # a cube at 5
  "github-403:d4663f27:c72ac063:fillet_exact_reported403_step.json" # corners meet
  "github-403:dfdf80d2:ebdd0f2c:fillet_exact_reported403_inner.json" # and the inner corner
  "github-557:6b8fce5c:e95dcc81:fillet_full_round_reported557.json" # a full round (mitcad#2279)
  "github-536:01a7f736:11e014d7:fillet_exact_reported536_two.json" # two features (mitcad#2312)
  "github-536:-:4eacad45:fillet_exact_reported536.json" # the same edges in one
  "github-566:7e2ab5e2:f225bd10:fillet_exact_reported566.json" # the back edge rounded past its end faces
  "github-566:2b8580b9:2d7fda11:fillet_exact_reported566b.json") # the back edge bevelled past them

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "fillet exact reported: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
# Unpacked into WORK, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
set(ran 0)
foreach(case IN LISTS cases)
  string(REPLACE ":" ";" fields "${case}")
  list(GET fields 0 folder)
  list(GET fields 1 archive_id)
  list(GET fields 2 design_id)
  list(GET fields 3 script)
  if(archive_id STREQUAL "-")
    # Designs stored as they are (no archive): the folder's designs copied
    # into WORK.
    set(unpacked "${WORK}/${folder}")
    if(NOT IS_DIRECTORY "${unpacked}")
      file(GLOB stored LIST_DIRECTORIES false "${reported}/${folder}/*.mitcad")
      file(MAKE_DIRECTORY "${unpacked}")
      foreach(candidate IN LISTS stored)
        get_filename_component(name "${candidate}" NAME)
        file(COPY_FILE "${candidate}" "${unpacked}/${name}")
      endforeach()
    endif()
    set(holder "the folder ${reported}/${folder}")
  else()
    file(GLOB archives LIST_DIRECTORIES false "${reported}/${folder}/*.zip")
    set(archive "")
    foreach(candidate IN LISTS archives)
      file(SHA256 "${candidate}" sum)
      string(SUBSTRING "${sum}" 0 8 id)
      if(id STREQUAL archive_id)
        set(archive "${candidate}")
        break()
      endif()
    endforeach()
    if(archive STREQUAL "")
      message(STATUS "fillet exact reported: no archive ${archive_id} in ${reported}/${folder}")
      continue()
    endif()
    set(unpacked "${WORK}/${archive_id}")
    if(NOT IS_DIRECTORY "${unpacked}")
      file(ARCHIVE_EXTRACT INPUT "${archive}" DESTINATION "${unpacked}")
    endif()
    set(holder "the archive ${archive_id}")
  endif()
  file(GLOB_RECURSE designs LIST_DIRECTORIES false "${unpacked}/*.mitcad")
  set(design "")
  foreach(candidate IN LISTS designs)
    file(SHA256 "${candidate}" sum)
    string(SUBSTRING "${sum}" 0 8 id)
    if(id STREQUAL design_id)
      set(design "${candidate}")
      break()
    endif()
  endforeach()
  if(design STREQUAL "")
    if(archive_id STREQUAL "-")
      # Not stored yet (an older copy of the reported files): skipped as a
      # missing archive is.
      message(STATUS "fillet exact reported: no design ${design_id} in ${holder}")
      continue()
    endif()
    message(FATAL_ERROR "${holder} holds no design ${design_id}")
  endif()
  execute_process(COMMAND "${CLI}" run "${SCRIPTS}/${script}" --open "${design}"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "${script} failed (${status}):\n${out}${err}")
  endif()
  message(STATUS "ok   ${script}")
  math(EXPR ran "${ran} + 1")
endforeach()
if(ran EQUAL 0)
  message(STATUS "fillet exact reported: skipped (no reported archive found)")
endif()
