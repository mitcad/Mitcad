# SPDX-License-Identifier: MIT
# Test of mitcad-geomtool on files it generates itself.
# Usage: cmake -DTOOL=<mitcad-geomtool> -DDIR=<scratch directory> -P geomtool-test.cmake
cmake_minimum_required(VERSION 3.24)

file(REMOVE_RECURSE "${DIR}")
file(MAKE_DIRECTORY "${DIR}")

# Runs the tool with the remaining arguments; the output goes to OUTPUT.
# Fails unless the exit code is EXPECT.
function(tool expect output)
  execute_process(COMMAND "${TOOL}" ${ARGN}
    RESULT_VARIABLE code OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT code STREQUAL "${expect}")
    message(SEND_ERROR "mitcad-geomtool ${ARGN}: exit ${code}, expected ${expect}\n${out}${err}")
  endif()
  set(${output} "${out}" PARENT_SCOPE)
endfunction()

# Checks that the first number of "KEY: value" lies in [LOW, HIGH].
function(expect_range text key low high)
  string(REGEX MATCH "(^|\n)${key}: ([^\n ]+)" match "${text}")
  set(value "${CMAKE_MATCH_2}")
  if(NOT value MATCHES "^-?[0-9]" OR "${value}" LESS "${low}" OR "${value}" GREATER "${high}")
    message(SEND_ERROR "${key} is '${value}', expected ${low} .. ${high}\n${text}")
  endif()
endfunction()

function(expect_line text line)
  string(FIND "${text}" "\n${line}\n" at)
  string(FIND "${text}" "${line}\n" first)
  if(at EQUAL -1 AND NOT first EQUAL 0)
    message(SEND_ERROR "missing line '${line}' in\n${text}")
  endif()
endfunction()

# Test files.
tool(0 out make box 10 20 30 --name Box "${DIR}/box.step")
tool(0 out make box 10 20 30 --at 0.1,0,0 "${DIR}/shifted.step")
tool(0 out make box 5 5 5 --at -20,0,0 "${DIR}/far.step")
tool(0 out make filleted-block "${DIR}/filleted.step")
tool(0 out make holed-block "${DIR}/holed.stp")

# Properties: steel box.
tool(0 out props "${DIR}/box.step" --density 7.85)
expect_line("${out}" "body1.name: Box")
expect_range("${out}" "body1.volume_mm3" 5999.9999 6000.0001)
expect_range("${out}" "total.area_mm2" 2199.9999 2200.0001)
expect_range("${out}" "total.mass_kg" 0.04709999 0.04710001)
expect_line("${out}" "total.center_mm: 5 10 15")
expect_line("${out}" "total.bounds_mm: 0 0 0 10 20 30")
tool(0 out props "${DIR}/box.step" --json)
if(NOT out MATCHES "^\\{\"body1.name\": \"Box\", .*\"total.volume_mm3\": 6000[,.]")
  message(SEND_ERROR "unexpected JSON: ${out}")
endif()

# A filleted block (every edge r 3): 48000 - 4 * 102 * (1 - pi/4) * 9 - 8 * (1 - pi/6) * 27.
set(filleted_low 47109.07)
set(filleted_high 47109.09)
tool(0 out props "${DIR}/filleted.step")
expect_range("${out}" "total.volume_mm3" ${filleted_low} ${filleted_high})

# Conversions keep the geometry; mesh formats come close.
tool(0 out convert "${DIR}/filleted.step" "${DIR}/filleted.igs" --unit in)
tool(0 out props "${DIR}/filleted.igs")
expect_range("${out}" "total.volume_mm3" ${filleted_low} ${filleted_high})
tool(0 out convert "${DIR}/filleted.step" "${DIR}/filleted.stl" --refinement high)
expect_range("${out}" "triangles" 1000 1000000)
tool(0 out props "${DIR}/filleted.stl")
expect_line("${out}" "body1.mesh: true")
expect_range("${out}" "total.volume_mm3" 46950 47110)
tool(0 out convert "${DIR}/filleted.step" "${DIR}/filleted.obj" --refinement low --ascii)
tool(0 out props "${DIR}/filleted.obj")
expect_range("${out}" "total.volume_mm3" 46600 47110)
tool(0 out convert "${DIR}/box.step" "${DIR}/box-inch.step" --unit in --schema ap242)
tool(0 out props "${DIR}/box-inch.step")
expect_range("${out}" "total.volume_mm3" 5999.9999 6000.0001)
expect_line("${out}" "body1.name: Box")
tool(0 out convert "${DIR}/box.step" "${DIR}/box.brep")
tool(0 out convert "${DIR}/box.brep" "${DIR}/box-ascii.stl" --ascii --deflection 0.5 --angle 20)

# Comparison: equal shapes pass, a 0.1 mm shift fails a 0.05 mm limit.
tool(0 out compare "${DIR}/box.step" "${DIR}/box-inch.step" --max-deviation 1e-6 --max-relative 1e-9)
expect_line("${out}" "within_limits: true")
tool(1 out compare "${DIR}/box.step" "${DIR}/shifted.step" --max-deviation 0.05 --samples 500)
expect_range("${out}" "max_deviation_mm" 0.0999999 0.1000001)
expect_range("${out}" "a_minus_b_mm3" 59.9999 60.0001)
expect_range("${out}" "relative_difference" 0.0199999 0.0200001)
expect_line("${out}" "within_limits: false")

# Sections.
tool(0 out section "${DIR}/box.step" --plane z=10 --out "${DIR}/section.brep")
expect_line("${out}" "edges: 4")
expect_range("${out}" "area_mm2" 199.9999 200.0001)
expect_range("${out}" "length_mm" 59.9999 60.0001)
if(NOT EXISTS "${DIR}/section.brep")
  message(SEND_ERROR "section --out wrote nothing")
endif()
# 50 x 30 block with a 12 mm hole: 1500 - 36 pi.
tool(0 out section "${DIR}/holed.stp" --plane 0,0,10,0,0,1)
expect_range("${out}" "area_mm2" 1386.9026 1386.9027)

# Interference: box/shifted 9.9 x 20 x 30, each box with the holed block 4000.
tool(0 out interference "${DIR}/box.step" "${DIR}/shifted.step" "${DIR}/holed.stp" "${DIR}/far.step")
expect_line("${out}" "bodies: 4")
expect_line("${out}" "interferences: 3")
expect_line("${out}" "pair1.first: Box")
expect_range("${out}" "pair1.volume_mm3" 5939.999 5940.001)
expect_range("${out}" "pair2.volume_mm3" 3999.999 4000.001)

tool(0 out distance "${DIR}/box.step" "${DIR}/far.step")
expect_range("${out}" "distance_mm" 14.9999999 15.0000001)

# Errors exit with 2.
tool(2 out props "${DIR}/missing.step")
tool(2 out frobnicate)
tool(2 out make box 1 2 "${DIR}/bad.step")
tool(2 out section "${DIR}/box.step" --plane w=3)
