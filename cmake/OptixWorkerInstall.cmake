# SPDX-License-Identifier: MIT
# The target optix-worker's step (cmake/OptixWorker.cmake, mitcad#1332):
# copies a user's own OptiX render worker into a folder of its own.
#
#   cmake -DWORKER=<build>/app/mitcad-render-optix[.exe] -DDESTINATION=<folder>
#         -P cmake/OptixWorkerInstall.cmake
#
# The worker, Cycles' kernels where it looks for them (cycles/lib beside
# it) and, on Windows, the DLLs beside it in the build folder (vcpkg's, Qt's
# and Open Image Denoise's: it loads them from its own folder). On Linux and
# macOS it keeps the build's run path: the build folder and the render
# prefix stay where they are while the worker is used.

foreach(variable WORKER DESTINATION)
  if(NOT ${variable})
    message(FATAL_ERROR "OptixWorkerInstall.cmake needs -D${variable}=...")
  endif()
endforeach()
if(NOT EXISTS "${WORKER}")
  message(FATAL_ERROR "No render worker ${WORKER}")
endif()
get_filename_component(source "${WORKER}" DIRECTORY)
get_filename_component(destination "${DESTINATION}" ABSOLUTE)
get_filename_component(source "${source}" ABSOLUTE)
if(destination STREQUAL source)
  message(FATAL_ERROR "The OptiX worker is already in ${destination}: give MITCAD_OPTIX_WORKER_DIR another folder")
endif()
# Never into MitCad's installation (or a build folder of it): its DLLs
# would replace MitCad's, and an update or an uninstall removes the folder.
foreach(shipped mitcad mitcad.exe mitcad-render mitcad-render.exe)
  if(EXISTS "${destination}/${shipped}")
    message(FATAL_ERROR "${destination} has ${shipped}: the OptiX worker goes into a folder of its own, not "
                        "into MitCad's (MITCAD_OPTIX_WORKER_DIR)")
  endif()
endforeach()

file(MAKE_DIRECTORY "${destination}")
file(COPY "${WORKER}" DESTINATION "${destination}" USE_SOURCE_PERMISSIONS)
if(EXISTS "${source}/cycles")
  file(COPY "${source}/cycles" DESTINATION "${destination}")
endif()
if(CMAKE_HOST_WIN32)
  file(GLOB libraries "${source}/*.dll")
  if(libraries)
    file(COPY ${libraries} DESTINATION "${destination}")
  endif()
endif()
get_filename_component(name "${WORKER}" NAME)
file(TO_NATIVE_PATH "${destination}/${name}" picked)
message(STATUS "The OptiX render worker is ${picked}: pick it in MitCad's Preferences, Display, Render worker")
