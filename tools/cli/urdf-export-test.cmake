# SPDX-License-Identifier: MIT
cmake_minimum_required(VERSION 3.24)
# URDF export for ROS 2 (mitcad#2100): mitcad-cli runs
# tests/urdf_export.json in WORK (a grounded plate, a pin on a limited
# revolute joint, a shaft on a cylindrical joint) and this script checks
# the package folder it writes, arm/:
#
# - urdf/arm.urdf, meshes/ with a binary STL per body, package.xml and
#   CMakeLists.txt (installing urdf and meshes);
# - the URDF's structure: a robot of four links (base_link, Pin_1, Shaft_1
#   and the cylindrical joint's intermediate Joint2_1) and three joints
#   (Joint1 revolute with its limits in radians, Joint2_tz prismatic with
#   its limits in metres, Joint2_rz continuous), each link's meshes as
#   package:// URIs of files that exist, every link but the base a child
#   once;
# - check_urdf (liburdfdom-tools) parses it when it is installed.
#
# cmake -DCLI=<mitcad-cli> -DSCRIPT=<urdf_export.json> -DWORK=<folder> -P urdf-export-test.cmake

foreach(var CLI SCRIPT WORK)
  if(NOT DEFINED ${var})
    message(FATAL_ERROR "urdf-export-test.cmake needs -D${var}=...")
  endif()
endforeach()

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
execute_process(COMMAND "${CLI}" run "${SCRIPT}" WORKING_DIRECTORY "${WORK}"
                RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT status EQUAL 0)
  message(FATAL_ERROR "mitcad-cli run failed (${status}):\n${out}\n${err}")
endif()

set(package "${WORK}/arm")
foreach(file urdf/arm.urdf package.xml CMakeLists.txt)
  if(NOT EXISTS "${package}/${file}")
    message(FATAL_ERROR "the package has no ${file}")
  endif()
endforeach()
file(READ "${package}/CMakeLists.txt" cmake_text)
if(NOT cmake_text MATCHES "install\\(DIRECTORY urdf meshes DESTINATION share/\\$\\{PROJECT_NAME\\}\\)")
  message(FATAL_ERROR "CMakeLists.txt does not install urdf and meshes:\n${cmake_text}")
endif()
file(READ "${package}/package.xml" package_text)
if(NOT package_text MATCHES "<name>arm</name>" OR NOT package_text MATCHES "<build_type>ament_cmake</build_type>")
  message(FATAL_ERROR "package.xml is not the arm package's:\n${package_text}")
endif()

file(READ "${package}/urdf/arm.urdf" urdf)
# check(regex what): the URDF matches.
function(check regex what)
  if(NOT urdf MATCHES "${regex}")
    message(FATAL_ERROR "the URDF has no ${what}:\n${urdf}")
  endif()
endfunction()
# count(regex expected what): how often it matches.
function(count regex expected what)
  string(REGEX MATCHALL "${regex}" found "${urdf}")
  list(LENGTH found n)
  if(NOT n EQUAL expected)
    message(FATAL_ERROR "the URDF has ${n} ${what}, not ${expected}:\n${urdf}")
  endif()
endfunction()

check("^<\\?xml version=\"1.0\"\\?>" "XML declaration")
check("<robot name=\"arm\">" "robot arm")
check("</robot>\n$" "end of the robot")
count("<link name=\"[^\"]+\">" 4 "links")
count("<joint name=\"[^\"]+\" type=\"[a-z]+\">" 3 "joints")
foreach(link base_link Pin_1 Shaft_1 Joint2_1)
  check("<link name=\"${link}\">" "link ${link}")
endforeach()
check("<joint name=\"Joint1\" type=\"revolute\">[^<]*<parent link=\"base_link\"/>[^<]*<child link=\"Pin_1\"/>"
      "revolute Joint1 from the base to the pin")
check("lower=\"-1.57079632679\" upper=\"1.57079632679\"" "Joint1's limits in radians")
check("<joint name=\"Joint2_tz\" type=\"prismatic\">[^<]*<parent link=\"base_link\"/>[^<]*<child link=\"Joint2_1\"/>"
      "prismatic Joint2_tz from the base")
check("lower=\"0\" upper=\"0.015\"" "Joint2_tz's limits in metres")
check("<joint name=\"Joint2_rz\" type=\"continuous\">[^<]*<parent link=\"Joint2_1\"/>[^<]*<child link=\"Shaft_1\"/>"
      "continuous Joint2_rz to the shaft")
# Each child once, never the base.
foreach(link Pin_1 Shaft_1 Joint2_1)
  count("<child link=\"${link}\"/>" 1 "joints into ${link}")
endforeach()
count("<child link=\"base_link\"/>" 0 "joints into the base")
# Every mesh referred to exists, a binary STL (its header is not "solid").
string(REGEX MATCHALL "package://arm/meshes/[^\"]+" uris "${urdf}")
list(REMOVE_DUPLICATES uris)
list(LENGTH uris mesh_count)
if(NOT mesh_count EQUAL 3)
  message(FATAL_ERROR "the URDF refers to ${mesh_count} meshes, not 3:\n${urdf}")
endif()
foreach(uri IN LISTS uris)
  string(REPLACE "package://arm/" "${package}/" file "${uri}")
  if(NOT EXISTS "${file}")
    message(FATAL_ERROR "${uri} was not written")
  endif()
  file(READ "${file}" head LIMIT 5)
  if(head STREQUAL "solid")
    message(FATAL_ERROR "${uri} is not a binary STL")
  endif()
endforeach()

find_program(CHECK_URDF check_urdf)
if(CHECK_URDF)
  execute_process(COMMAND "${CHECK_URDF}" "${package}/urdf/arm.urdf"
                  RESULT_VARIABLE status OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT status EQUAL 0)
    message(FATAL_ERROR "check_urdf refused arm.urdf:\n${out}\n${err}")
  endif()
  message(STATUS "check_urdf: arm.urdf parses")
else()
  message(STATUS "check_urdf is not installed: the structure checked by this script only")
endif()
message(STATUS "URDF package arm: 4 links, 3 joints, 3 meshes")
