# SPDX-License-Identifier: MIT
# A user's own OptiX render worker (mitcad#1332, docs/rendering.md "Building
# an OptiX worker yourself"): in a configure whose render prefix has OptiX
# (cmake/RenderOptix.cmake), the target optix-worker builds
# mitcad-render-optix and copies it, with Cycles' kernels and on Windows the
# DLLs beside it, into a folder of its own, MITCAD_OPTIX_WORKER_DIR. Not
# into MitCad's installation: an update or an uninstall of MitCad leaves it,
# and the shipped mitcad-render stays as it is, the fallback when the own
# worker cannot be used. The user then picks it in Preferences › Display ›
# Render worker. Such a build makes no installer and no AppImage
# (cmake/Packaging.cmake, cmake/AppImage.cmake).
#
#   cmake --build <build folder> --target optix-worker

get_property(mitcad_render_optix GLOBAL PROPERTY MITCAD_RENDER_OPTIX)
if(NOT mitcad_render_optix)
  return()
endif()

if(WIN32)
  file(TO_CMAKE_PATH "$ENV{LOCALAPPDATA}/MitCad/mitcad-render-optix" mitcad_optix_default)
else()
  set(mitcad_optix_default "$ENV{HOME}/.local/lib/mitcad-render-optix")
endif()
set(MITCAD_OPTIX_WORKER_DIR "${mitcad_optix_default}" CACHE PATH
    "The folder of the OptiX render worker built for this machine (the target optix-worker)")
add_custom_target(optix-worker
  COMMAND "${CMAKE_COMMAND}" "-DWORKER=$<TARGET_FILE:mitcad-render>" "-DDESTINATION=${MITCAD_OPTIX_WORKER_DIR}"
          -P "${PROJECT_SOURCE_DIR}/cmake/OptixWorkerInstall.cmake"
  COMMENT "Copying mitcad-render-optix to ${MITCAD_OPTIX_WORKER_DIR}"
  VERBATIM
  USES_TERMINAL)
add_dependencies(optix-worker mitcad-render)
