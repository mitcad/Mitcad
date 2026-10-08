# SPDX-License-Identifier: MIT
# mitcad-cli render (mitcad#48): the test design (tools/cli/tests/
# render_scene.json) rendered by the render worker in every format, and the
# image files checked: PNG's size, bit depth and alpha from its header, a
# transparent PNG's alpha that is not empty (larger than an empty one),
# JPEG's and OpenEXR's signatures; an unknown view fails; the render
# devices, and a missing one's fallback to the CPU.
#   cmake -DCLI=<mitcad-cli> -DWORKER=<mitcad-render> -DSCRIPT=<render_scene.json> -DWORK=<dir> -P render-test.cmake

file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")

function(run)
  execute_process(COMMAND ${ARGN} RESULT_VARIABLE result OUTPUT_VARIABLE out ERROR_VARIABLE err)
  if(NOT result EQUAL 0)
    message(FATAL_ERROR "failed (${result}): ${ARGN}\n${out}\n${err}")
  endif()
  message(STATUS "${out}")
endfunction()

# png_header file width height depth colour: checks a PNG's IHDR (colour 2:
# RGB, 6: RGBA).
function(png_header file width height depth colour)
  file(READ "${file}" hex LIMIT 26 HEX)
  string(SUBSTRING "${hex}" 0 16 signature)
  string(SUBSTRING "${hex}" 32 8 w)
  string(SUBSTRING "${hex}" 40 8 h)
  string(SUBSTRING "${hex}" 48 2 d)
  string(SUBSTRING "${hex}" 50 2 c)
  math(EXPR w "0x${w}")
  math(EXPR h "0x${h}")
  math(EXPR d "0x${d}")
  math(EXPR c "0x${c}")
  if(NOT signature STREQUAL "89504e470d0a1a0a" OR NOT w EQUAL ${width} OR NOT h EQUAL ${height}
     OR NOT d EQUAL ${depth} OR NOT c EQUAL ${colour})
    message(FATAL_ERROR "${file}: ${signature} ${w} x ${h}, ${d} bits, colour type ${c}; "
                        "expected ${width} x ${height}, ${depth} bits, colour type ${colour}")
  endif()
  message(STATUS "${file}: ${w} x ${h}, ${d} bits, colour type ${c}")
endfunction()

run("${CLI}" run "${SCRIPT}" --save "${WORK}/render.mitcad")
set(render "${CLI}" render "${WORK}/render.mitcad" --worker "${WORKER}")

# The settings' size (320 x 240) and samples (4); a named view.
run(${render} --view Corner -o "${WORK}/corner.png")
png_header("${WORK}/corner.png" 320 240 8 2)
# Options over the settings; a standard view fitted to the bodies.
run(${render} --view iso --size 160x120 --samples 2 --no-denoise -o "${WORK}/iso.png")
png_header("${WORK}/iso.png" 160 120 8 2)
run(${render} --view top --size 120x160 --transparent --format png16 -o "${WORK}/top.png")
png_header("${WORK}/top.png" 120 160 16 6)
# A transparent image with the bodies holds more than an empty one would
# (a few hundred bytes).
file(SIZE "${WORK}/top.png" bytes)
if(bytes LESS 4000)
  message(FATAL_ERROR "top.png: ${bytes} bytes, as if nothing was rendered")
endif()
run(${render} --view front --size 160x120 -o "${WORK}/front.jpg")
file(READ "${WORK}/front.jpg" hex LIMIT 3 HEX)
if(NOT hex STREQUAL "ffd8ff")
  message(FATAL_ERROR "front.jpg is no JPEG: ${hex}")
endif()
run(${render} --size 160x120 -o "${WORK}/home.exr")
file(READ "${WORK}/home.exr" hex LIMIT 4 HEX)
if(NOT hex STREQUAL "762f3101")
  message(FATAL_ERROR "home.exr is no OpenEXR file: ${hex}")
endif()

# A view the design does not have.
execute_process(COMMAND ${render} --view Nowhere -o "${WORK}/none.png" RESULT_VARIABLE result
                ERROR_VARIABLE err)
if(result EQUAL 0 OR NOT err MATCHES "no view 'Nowhere': the design's named views are Corner")
  message(FATAL_ERROR "an unknown view was not refused (${result}): ${err}")
endif()
# The render device (mitcad#50): the worker's devices, the CPU first; a
# device that is not there renders on the CPU with a warning.
execute_process(COMMAND "${CLI}" render --list-devices --worker "${WORKER}" RESULT_VARIABLE result
                OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT result EQUAL 0 OR NOT out MATCHES "^CPU\tCPU\t")
  message(FATAL_ERROR "--list-devices (${result}): ${out}${err}")
endif()
message(STATUS "${out}")
execute_process(COMMAND ${render} --size 80x60 --samples 2 --device NO_SUCH_DEVICE -o "${WORK}/fallback.png"
                RESULT_VARIABLE result OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT result EQUAL 0 OR NOT err MATCHES "Warning: There is no render device \"NO_SUCH_DEVICE\"; rendering on the CPU"
   OR NOT out MATCHES "Rendered .*, on [^(]+[)]")
  message(FATAL_ERROR "a missing device did not render on the CPU (${result}): ${out}${err}")
endif()
png_header("${WORK}/fallback.png" 80 60 8 2)
message(STATUS "${out}")
message(STATUS "Render test passed")
