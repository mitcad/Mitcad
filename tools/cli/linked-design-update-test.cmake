# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# A design holding a linked design whose file changed after the holder was
# saved, from an issue report (mitcad#1416; never committed): the project
# subset under MITCAD_MITCAD_REPORTED with its `.mitcad` store, the holder
# found by its content id (the first 8 hex digits of its SHA-256). Copied
# into WORK and opened with mitcad-cli info as the application opens a
# design (its links followed from its folder): the linked component takes
# the changed file's bodies, with the load warning naming the file, and
# its base feature's bodies are those of the linked file opened alone
# (their volumes and areas). Skipped when the folder or the design is not
# there.
#
# cmake -DCLI=<mitcad-cli> -DWORK=<folder> -P linked-design-update-test.cmake

foreach(var CLI WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "linked-design-update-test.cmake needs -D${var}=...")
  endif()
endforeach()

set(holder_id "1f82e155") # the holder, saved before the linked file changed
set(linked_id "ccf5808a") # the linked file as changed since

set(reported "$ENV{MITCAD_MITCAD_REPORTED}")
if(reported STREQUAL "" OR NOT IS_DIRECTORY "${reported}")
  message(STATUS "linked design update: skipped (MITCAD_MITCAD_REPORTED is not set)")
  return()
endif()
file(GLOB_RECURSE designs LIST_DIRECTORIES false "${reported}/*.mitcad")
set(holder "")
foreach(design IN LISTS designs)
  file(SHA256 "${design}" sum)
  string(SUBSTRING "${sum}" 0 8 id)
  if(id STREQUAL holder_id)
    set(holder "${design}")
    break()
  endif()
endforeach()
if(holder STREQUAL "")
  message(STATUS "linked design update: skipped (no design ${holder_id} in ${reported})")
  return()
endif()

# Its project: the folder above it with the project marker.
get_filename_component(root "${holder}" DIRECTORY)
while(NOT EXISTS "${root}/.mitcad/project.json")
  get_filename_component(parent "${root}" DIRECTORY)
  if(parent STREQUAL root)
    message(FATAL_ERROR "${holder} is in no MitCad project")
  endif()
  set(root "${parent}")
endwhile()
file(RELATIVE_PATH inside "${root}" "${holder}")

# The linked file, by the holder's link.
file(READ "${holder}" text)
string(REGEX MATCH "\"link\": {[^}]*\"path\": \"([^\"]+)\",[^}]*\"feature\": \"(F[0-9]+)\"" link "${text}")
if(link STREQUAL "")
  message(FATAL_ERROR "the design ${holder_id} has no linked component")
endif()
set(link_path "${CMAKE_MATCH_1}")
set(feature "${CMAKE_MATCH_2}")
get_filename_component(holder_dir "${holder}" DIRECTORY)
file(SHA256 "${holder_dir}/${link_path}" sum)
string(SUBSTRING "${sum}" 0 8 id)
if(NOT id STREQUAL linked_id)
  message(FATAL_ERROR "the linked file of ${holder_id} is ${id}, not the reported ${linked_id}")
endif()

# A copy, so that nothing is written beside the reported files.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
file(COPY "${root}/" DESTINATION "${WORK}/project")
set(copy "${WORK}/project/${inside}")
get_filename_component(copy_dir "${copy}" DIRECTORY)

execute_process(COMMAND "${CLI}" info "${copy}" RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "opening the holder failed (${status}):\n${out}${err}")
endif()
string(REGEX REPLACE "\\." "\\\\." link_pattern "${link_path}")
if(NOT out MATCHES "Warning: [^\n]+ is updated from ${link_pattern}\n")
  message(FATAL_ERROR "the holder did not take the changed ${link_path} (no load warning naming it):\n${out}${err}")
endif()

execute_process(COMMAND "${CLI}" info "${copy_dir}/${link_path}" RESULT_VARIABLE status OUTPUT_VARIABLE linked_out
                ERROR_VARIABLE linked_err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "opening the linked file failed (${status}):\n${linked_out}${linked_err}")
endif()

# The volumes and areas of the bodies of `uid` (a feature, or any: F[0-9]+)
# in a report's Bodies list, sorted.
function(body_sizes report uid result)
  string(REGEX MATCHALL "\\(${uid}\\.b[0-9]+\\): volume [-0-9.]+ mm3, area [0-9.]+ mm2" lines "${report}")
  set(sizes)
  foreach(line IN LISTS lines)
    string(REGEX REPLACE "^.*: volume " "" size "${line}")
    list(APPEND sizes "${size}")
  endforeach()
  list(SORT sizes)
  set(${result} "${sizes}" PARENT_SCOPE)
endfunction()
body_sizes("${out}" "${feature}" taken)
body_sizes("${linked_out}" "F[0-9]+" current)
list(LENGTH current count)
if(count EQUAL 0)
  message(FATAL_ERROR "the linked file shows no bodies:\n${linked_out}")
endif()
if(NOT taken STREQUAL current)
  message(FATAL_ERROR "the linked component's bodies (${feature}) are not the linked file's:\n"
                      "taken: ${taken}\ncurrent: ${current}")
endif()
message(STATUS "linked design update: ${count} bodies of ${link_path} taken into ${feature}")
