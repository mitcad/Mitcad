# SPDX-License-Identifier: MIT
# The release gate's "no OptiX in the shipped worker" check (mitcad#1332,
# cmake/RenderOptix.cmake), on made-up device and kernel lists: a release
# configure (MITCAD_RENDER_REQUIRED_DEVICES set, CPU;CUDA) stops on a
# render prefix with OptiX or an OptiX kernel; a development configure and
# a user's own OptiX build pass; no configure may require OPTIX.
#
#   cmake -P tools/render-optix-gate-test.cmake
#
# The ctest repo.render_optix_gate.

# The project's policies (if(... IN_LIST ...) among them) in script mode.
cmake_minimum_required(VERSION 3.24)
include("${CMAKE_CURRENT_LIST_DIR}/../cmake/RenderOptix.cmake")

set(failures 0)
# expect(description, devices, kernels, required, stops)
function(expect description devices kernels required stops)
  mitcad_render_optix_problem("${devices}" "${kernels}" "${required}" problem)
  if(stops AND NOT problem)
    message("FAIL: ${description}: the configure goes on")
    math(EXPR failures "${failures} + 1")
  elseif(NOT stops AND problem)
    message("FAIL: ${description}: the configure stops: ${problem}")
    math(EXPR failures "${failures} + 1")
  else()
    message("ok   ${description}")
  endif()
  set(failures ${failures} PARENT_SCOPE)
endfunction()

# expect_one_string(description, devices, kernels, required): the configure
# stops, and with a one-element required list the reason is one string, not
# a list: no semicolons between its pieces (mitcad#1611).
function(expect_one_string description devices kernels required)
  mitcad_render_optix_problem("${devices}" "${kernels}" "${required}" problem)
  string(FIND "${problem}" ";" semicolon)
  if(problem STREQUAL "")
    message("FAIL: ${description}: the configure goes on")
    math(EXPR failures "${failures} + 1")
  elseif(NOT semicolon EQUAL -1)
    message("FAIL: ${description}: a semicolon in the reason: ${problem}")
    math(EXPR failures "${failures} + 1")
  else()
    message("ok   ${description}: ${problem}")
  endif()
  set(failures ${failures} PARENT_SCOPE)
endfunction()

set(cuda_kernels"/prefix/install/cycles/kernels/lib/kernel_sm_75.cubin.zst"
                 "/prefix/install/cycles/kernels/lib/kernel_compute_75.ptx.zst")
set(optix_kernels ${cuda_kernels} "/prefix/install/cycles/kernels/lib/kernel_optix.ptx.zst"
                  "/prefix/install/cycles/kernels/lib/kernel_optix_shader_raytrace.ptx.zst")

expect("a release with CPU and CUDA" "CPU;CUDA" "${cuda_kernels}" "CPU;CUDA" FALSE)
expect("a release with OptiX in the devices" "CPU;CUDA;OPTIX" "${optix_kernels}" "CPU;CUDA" TRUE)
expect("a release with OptiX kernels only" "CPU;CUDA" "${optix_kernels}" "CPU;CUDA" TRUE)
expect("a release with a Windows OptiX kernel" "CPU;CUDA"
       "C:/dev/mitcad-render-deps/install/cycles/kernels/lib/kernel_optix.ptx.zst" "CPU;CUDA" TRUE)
# A folder named for OptiX holds no OptiX kernel by its name alone.
expect("a release from a folder named optix" "CPU;CUDA"
       "/home/user/optix-free/install/cycles/kernels/lib/kernel_sm_75.cubin.zst" "CPU;CUDA" FALSE)
expect("a development build without OptiX" "CPU;CUDA" "${cuda_kernels}" "" FALSE)
expect("a user's own OptiX build" "CPU;CUDA;OPTIX" "${optix_kernels}" "" FALSE)
expect("a configure that requires OPTIX" "CPU;CUDA;OPTIX" "${optix_kernels}" "CPU;CUDA;OPTIX" TRUE)
expect("a CPU-only release check" "CPU" "" "CPU" FALSE)

# The stopping reasons read as one sentence each.
expect_one_string("the reason for one OptiX device" "OPTIX" "" "CUDA")
expect_one_string("the reason for several devices with OptiX" "CPU;CUDA;OPTIX" "" "CUDA")
expect_one_string("the reason for an OptiX kernel" "CUDA" "${optix_kernels}" "CUDA")
expect_one_string("the reason for an OptiX kernel without devices" "" "${optix_kernels}" "CUDA")
expect_one_string("the reason for a required OPTIX" "OPTIX" "" "OPTIX")

if(failures GREATER 0)
  message(FATAL_ERROR "${failures} checks of the OptiX gate failed")
endif()
message("PASS: the shipped worker's devices are checked for OptiX")
