# SPDX-License-Identifier: MIT
# OptiX in the render worker (mitcad#1332, docs/rendering.md "Building an
# OptiX worker yourself"). MitCad never builds or ships OptiX: Cycles
# compiles the OptiX SDK's headers into the worker, and NVIDIA's SDK licence
# allows passing such a program on only in binary form, only for NVIDIA
# hardware and only to recipients bound to NVIDIA's terms. A user may build
# an OptiX worker for their own machine from the SDK they downloaded from
# NVIDIA (tools/dev-env/build-cycles.sh --optix, build-cycles.ps1 -Optix,
# into a prefix of its own); MitCad uses it when it is picked in
# Preferences. Included by cmake/Render.cmake, and by the test
# tools/render-optix-gate-test.cmake, which calls the function below.

# Sets `out_var` to why a configure must stop, else to an empty string:
#   - `required` (MITCAD_RENDER_REQUIRED_DEVICES) names OPTIX: no build of
#     MitCad requires it;
#   - `required` is set (a release configure, which asks for CPU;CUDA) and
#     the prefix's `devices` (MITCAD_CYCLES_DEVICES) have OPTIX, or one of
#     its `kernels` (MITCAD_CYCLES_KERNELS) is an OptiX kernel: the shipped
#     worker never has OptiX.
# A configure without required devices (a development build, a user's own
# OptiX build) passes.
function(mitcad_render_optix_problem devices kernels required out_var)
  set(problem "")
  # The kernels' file names only: a prefix's folder may be named for OptiX.
  set(optix_kernels ${kernels})
  list(FILTER optix_kernels INCLUDE REGEX "[Oo][Pp][Tt][Ii][Xx][^/\\\\]*$")
  # One string each (string(CONCAT)): set() with several pieces would make a
  # list, which the FATAL_ERROR prints with semicolons between the pieces.
  if("OPTIX" IN_LIST required)
    string(CONCAT problem "MITCAD_RENDER_REQUIRED_DEVICES (${required}) names OPTIX: MitCad never builds or ships "
                  "OptiX (docs/rendering.md, \"Building an OptiX worker yourself\")")
  elseif(required AND ("OPTIX" IN_LIST devices OR optix_kernels))
    # The devices joined with commas, so that only the required devices'
    # own value shows a semicolon.
    list(JOIN devices ", " found)
    if(optix_kernels)
      list(GET optix_kernels 0 kernel)
      get_filename_component(kernel "${kernel}" NAME)
      if(NOT found STREQUAL "")
        string(APPEND found ", with ")
      endif()
      string(APPEND found "the OptiX kernel ${kernel}")
    endif()
    string(CONCAT problem "The render prefix was built with OptiX (${found}), and this configure is a release's "
                  "(MITCAD_RENDER_REQUIRED_DEVICES=${required}): MitCad never ships OptiX. Build the release "
                  "with a prefix of tools/dev-env/build-cycles.sh --cuda or build-cycles.ps1 -Cuda, without "
                  "--optix or -Optix")
  endif()
  set(${out_var} "${problem}" PARENT_SCOPE)
endfunction()
