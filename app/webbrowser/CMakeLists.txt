# SPDX-License-Identifier: MIT
# Insert › Component from Web's web browser component (mitcad#1579,
# app/COMMANDS.md "Component from Web", docs/development.md "The web browser
# component"), added by app/CMakeLists.txt as a directory of its own: the module mitcad-webbrowser
# (the browser window on the Chromium Embedded Framework, loaded by MitCad at
# run time), CEF's subprocess helper, and CEF's runtime, staged in
# <build>/browser-component as the package unpacks it (a build's MitCad uses
# it with MITCAD_BROWSER_COMPONENT_DIR, as the UI test does), and the target
# browser-package, which zips it as mitcad-browser-<version>-<platform>.zip
# for the release (tools/browser-package.sh, .ps1). Never installed with
# MitCad.
#
# CEF is its official binary distribution, pinned in tools/dev-env/versions.sh
# (CEF_VERSION) and unpacked by tools/dev-env/fetch-cef.sh / fetch-cef.ps1;
# MITCAD_CEF_ROOT names it (else the fetch scripts' folder is looked in).
# libcef_dll_wrapper is built here from the distribution's sources with
# CEF's own CMake files, with the DLL runtime of the rest of MitCad on
# Windows and without CEF's sandbox (no_sandbox: the helper runs without
# it). MITCAD_BROWSER_COMPONENT: AUTO builds the component where the
# distribution is, ON requires it (the release tooling), OFF leaves it out.
# The licences: CEF and Chromium are BSD-3-Clause with Chromium's bundled
# components under their own (FFmpeg's LGPL-2.1 inside libcef, a shared
# library of its own in the package), which the licence policy allows; the
# package carries packaging/browser/THIRD-PARTY-NOTICES.txt and CEF's
# LICENSE.txt.

set(MITCAD_BROWSER_COMPONENT AUTO CACHE STRING "Build Insert > Component from Web's web browser component (CEF): AUTO, ON (required) or OFF")
set_property(CACHE MITCAD_BROWSER_COMPONENT PROPERTY STRINGS AUTO ON OFF)
set(MITCAD_CEF_ROOT "$ENV{MITCAD_CEF_ROOT}" CACHE PATH "The unpacked CEF binary distribution (tools/dev-env/fetch-cef.sh, fetch-cef.ps1)")
# The package's version: the release tooling sets it (the build's date and
# time, tools/browser-package.sh); a build's own component is 0.0.0-build.
set(MITCAD_BROWSER_VERSION "0.0.0-build" CACHE STRING "The web browser component package's version")

# The module's interface version, from the header both sides include.
file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/ComponentBrowserHost.hpp" mitcad_browser_abi_line
     REGEX "constexpr int kComponentBrowserAbi = [0-9]+;")
string(REGEX MATCH "[0-9]+" MITCAD_BROWSER_ABI "${mitcad_browser_abi_line}")
# The pinned CEF version.
file(STRINGS "${PROJECT_SOURCE_DIR}/tools/dev-env/versions.sh" mitcad_cef_line REGEX "^CEF_VERSION=")
string(REGEX REPLACE "^CEF_VERSION=" "" MITCAD_CEF_VERSION "${mitcad_cef_line}")

if(WIN32)
  set(mitcad_browser_platform windows-x64)
  set(mitcad_cef_platform windows64)
  set(mitcad_cef_base "C:/dev/cef")
elseif(APPLE)
  set(mitcad_browser_platform macos-arm64)
  set(mitcad_cef_platform macosarm64)
  set(mitcad_cef_base "$ENV{HOME}/cef")
else()
  set(mitcad_browser_platform linux-x64)
  set(mitcad_cef_platform linux64)
  set(mitcad_cef_base "$ENV{HOME}/cef")
endif()
if(NOT MITCAD_CEF_ROOT AND NOT MITCAD_BROWSER_COMPONENT STREQUAL "OFF")
  set(mitcad_cef_candidate
      "${mitcad_cef_base}/${MITCAD_CEF_VERSION}/cef_binary_${MITCAD_CEF_VERSION}_${mitcad_cef_platform}_minimal")
  if(EXISTS "${mitcad_cef_candidate}/cmake/FindCEF.cmake")
    set(MITCAD_CEF_ROOT "${mitcad_cef_candidate}" CACHE PATH
        "The unpacked CEF binary distribution (tools/dev-env/fetch-cef.sh, fetch-cef.ps1)" FORCE)
  endif()
endif()

if(MITCAD_BROWSER_COMPONENT STREQUAL "OFF")
  message(STATUS "Component from Web: the web browser component is left out (MITCAD_BROWSER_COMPONENT OFF)")
  return()
endif()
if(NOT EXISTS "${MITCAD_CEF_ROOT}/cmake/FindCEF.cmake")
  if(MITCAD_BROWSER_COMPONENT STREQUAL "ON")
    message(FATAL_ERROR "MITCAD_BROWSER_COMPONENT is ON but MITCAD_CEF_ROOT ('${MITCAD_CEF_ROOT}') is no CEF "
                        "distribution: run tools/dev-env/fetch-cef.sh (fetch-cef.ps1)")
  endif()
  message(STATUS "Component from Web: the web browser component is left out (no CEF ${MITCAD_CEF_VERSION} "
                 "distribution; tools/dev-env/fetch-cef.sh)")
  return()
endif()

# --- CEF and libcef_dll_wrapper ------------------------------------------------
set(CEF_ROOT "${MITCAD_CEF_ROOT}")
list(APPEND CMAKE_MODULE_PATH "${CEF_ROOT}/cmake")
set(USE_SANDBOX OFF CACHE BOOL "CEF's sandbox (off: MitCad's helper runs without it)" FORCE)
set(CEF_RUNTIME_LIBRARY_FLAG "/MD" CACHE STRING "The runtime of libcef_dll_wrapper: the DLL one, as MitCad's" FORCE)
# CEF's CMake files (FindCEF.cmake, cef_variables.cmake) change the compiler
# and linker flags of the directory that includes them, for CEF's own code:
# exceptions off (/EHs-c-, _HAS_EXCEPTIONS=0), its warnings, its runtime.
# They are included only here, in a directory of its own (app/CMakeLists.txt
# adds it), so nothing of them reaches MitCad's other targets; and here the
# flags, their cache entries and this directory's compile and link
# properties are saved before and put back after libcef_dll_wrapper is
# added, so that the wrapper (in its own directory, which takes CEF's
# flags) is built as CEF builds it, and the module and the helper below
# with MitCad's flags (/EHsc, the DLL runtime) like the rest of MitCad.
set(mitcad_cef_kept_variables
    CMAKE_C_FLAGS CMAKE_CXX_FLAGS CMAKE_EXE_LINKER_FLAGS CMAKE_SHARED_LINKER_FLAGS CMAKE_MODULE_LINKER_FLAGS
    CMAKE_STATIC_LINKER_FLAGS CMAKE_MSVC_RUNTIME_LIBRARY CMAKE_CONFIGURATION_TYPES CMAKE_BUILD_TYPE
    CMAKE_CXX_STANDARD CMAKE_CXX_EXTENSIONS CMAKE_POSITION_INDEPENDENT_CODE CMAKE_OSX_DEPLOYMENT_TARGET
    CMAKE_OSX_ARCHITECTURES)
foreach(mitcad_cef_config IN ITEMS DEBUG RELEASE RELWITHDEBINFO MINSIZEREL)
  list(APPEND mitcad_cef_kept_variables
       CMAKE_C_FLAGS_${mitcad_cef_config} CMAKE_CXX_FLAGS_${mitcad_cef_config}
       CMAKE_EXE_LINKER_FLAGS_${mitcad_cef_config} CMAKE_SHARED_LINKER_FLAGS_${mitcad_cef_config}
       CMAKE_MODULE_LINKER_FLAGS_${mitcad_cef_config} CMAKE_STATIC_LINKER_FLAGS_${mitcad_cef_config})
endforeach()
foreach(mitcad_cef_variable IN LISTS mitcad_cef_kept_variables)
  if(DEFINED ${mitcad_cef_variable})
    set(mitcad_cef_was_${mitcad_cef_variable} ON)
    set(mitcad_cef_value_${mitcad_cef_variable} "${${mitcad_cef_variable}}")
  else()
    set(mitcad_cef_was_${mitcad_cef_variable} OFF)
  endif()
  if(DEFINED CACHE{${mitcad_cef_variable}})
    set(mitcad_cef_cached_${mitcad_cef_variable} ON)
    get_property(mitcad_cef_cache_${mitcad_cef_variable} CACHE ${mitcad_cef_variable} PROPERTY VALUE)
  else()
    set(mitcad_cef_cached_${mitcad_cef_variable} OFF)
  endif()
endforeach()
set(mitcad_cef_directory_properties COMPILE_OPTIONS COMPILE_DEFINITIONS LINK_OPTIONS INCLUDE_DIRECTORIES
    LINK_DIRECTORIES)
foreach(mitcad_cef_property IN LISTS mitcad_cef_directory_properties)
  get_directory_property(mitcad_cef_directory_${mitcad_cef_property} ${mitcad_cef_property})
endforeach()

# CEF's Linux flags (cef_variables.cmake) test a C compiler flag
# (CHECK_C_COMPILER_FLAG), which CMake refuses unless C is enabled; MitCad
# itself is C++ only, so C is enabled here, for the browser's directory.
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  enable_language(C)
endif()
find_package(CEF REQUIRED)
add_subdirectory("${CEF_LIBCEF_DLL_WRAPPER_PATH}" "${CMAKE_CURRENT_BINARY_DIR}/libcef_dll_wrapper")

# MitCad's flags back (the values a directory's targets get are those at
# the end of the directory, so the module and the helper get these).
foreach(mitcad_cef_variable IN LISTS mitcad_cef_kept_variables)
  if(mitcad_cef_cached_${mitcad_cef_variable})
    get_property(mitcad_cef_now CACHE ${mitcad_cef_variable} PROPERTY VALUE)
    if(NOT mitcad_cef_now STREQUAL mitcad_cef_cache_${mitcad_cef_variable})
      set_property(CACHE ${mitcad_cef_variable} PROPERTY VALUE "${mitcad_cef_cache_${mitcad_cef_variable}}")
    endif()
  elseif(DEFINED CACHE{${mitcad_cef_variable}})
    unset(${mitcad_cef_variable} CACHE)
  endif()
  if(mitcad_cef_was_${mitcad_cef_variable})
    set(${mitcad_cef_variable} "${mitcad_cef_value_${mitcad_cef_variable}}")
  else()
    unset(${mitcad_cef_variable})
  endif()
endforeach()
foreach(mitcad_cef_property IN LISTS mitcad_cef_directory_properties)
  set_property(DIRECTORY PROPERTY ${mitcad_cef_property} "${mitcad_cef_directory_${mitcad_cef_property}}")
endforeach()
if(MSVC)
  set_property(TARGET libcef_dll_wrapper PROPERTY MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL")
endif()
# No build machine's folders in the package (mitcad#1658): CEF's logging
# macros (cef_logging.h: __FILE__, __builtin_FILE()) put the absolute path of
# every wrapper source and CEF header that logs into the module and the
# helper as strings, which stripping the debug information leaves. The
# wrapper's are mapped to cef/... entirely (CEF's code, never debugged
# here); the module's and the helper's macros only (below), so that their
# debug information keeps the real paths for a debugger, and the release
# strips it (tools/browser-package.sh). Windows keeps its paths in the PDBs,
# which are not packaged.
set(mitcad_browser_path_maps "")
if(NOT MSVC)
  get_filename_component(mitcad_cef_real_root "${CEF_ROOT}" REALPATH)
  get_filename_component(mitcad_source_real_root "${PROJECT_SOURCE_DIR}" REALPATH)
  set(mitcad_cef_roots "${CEF_ROOT}")
  if(NOT mitcad_cef_real_root STREQUAL CEF_ROOT)
    list(APPEND mitcad_cef_roots "${mitcad_cef_real_root}")
  endif()
  set(mitcad_source_roots "${PROJECT_SOURCE_DIR}")
  if(NOT mitcad_source_real_root STREQUAL PROJECT_SOURCE_DIR)
    list(APPEND mitcad_source_roots "${mitcad_source_real_root}")
  endif()
  foreach(mitcad_root IN LISTS mitcad_cef_roots)
    target_compile_options(libcef_dll_wrapper PRIVATE "-ffile-prefix-map=${mitcad_root}/=cef/")
  endforeach()
  # MitCad's sources first, CEF's last: a CEF inside the source tree takes
  # its own name.
  foreach(mitcad_root IN LISTS mitcad_source_roots)
    list(APPEND mitcad_browser_path_maps "-fmacro-prefix-map=${mitcad_root}/=")
  endforeach()
  foreach(mitcad_root IN LISTS mitcad_cef_roots)
    list(APPEND mitcad_browser_path_maps "-fmacro-prefix-map=${mitcad_root}/=cef/")
  endforeach()
endif()
if(NOT APPLE)
  # libcef itself (on macOS its framework is loaded at run time, never linked).
  # The release library in every configuration: the minimal distribution
  # has no debug build, and its interface is the same.
  ADD_LOGICAL_TARGET("libcef_lib" "${CEF_LIB_RELEASE}" "${CEF_LIB_RELEASE}")
endif()

set(MITCAD_BROWSER_COMPONENT_DIR "${CMAKE_BINARY_DIR}/browser-component")
file(MAKE_DIRECTORY "${MITCAD_BROWSER_COMPONENT_DIR}")
# For the UI test (cmake/SelectionUiTests.cmake), outside this folder.
set_property(GLOBAL PROPERTY MITCAD_BROWSER_COMPONENT_DIR "${MITCAD_BROWSER_COMPONENT_DIR}")

# --- The module ------------------------------------------------------------------
add_library(mitcad-webbrowser MODULE
  ComponentBrowser.cpp
  ComponentBrowser.hpp
  ComponentBrowserHost.hpp
  ComponentWebFiles.cpp
  ComponentWebFiles.hpp
  cef/CefEngine.cpp
  cef/CefEngine.hpp
  cef/CefView.cpp
  cef/CefView.hpp
  ../files/FileFormats.cpp
  ../files/FileFormats.hpp)
target_include_directories(mitcad-webbrowser PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/..")
# CEF's headers as the system's: MitCad's warning flags are for its own code.
target_include_directories(mitcad-webbrowser SYSTEM PRIVATE "${CEF_INCLUDE_PATH}")
target_link_libraries(mitcad-webbrowser PRIVATE Qt6::Core Qt6::Gui Qt6::Widgets libcef_dll_wrapper mitcad_warnings)
target_compile_options(mitcad-webbrowser PRIVATE ${mitcad_browser_path_maps})
if(WIN32)
  target_compile_definitions(mitcad-webbrowser PRIVATE NOMINMAX WIN32_LEAN_AND_MEAN)
endif()
if(APPLE)
  target_sources(mitcad-webbrowser PRIVATE cef/CefMacApplication.mm)
  target_link_libraries(mitcad-webbrowser PRIVATE "-framework Cocoa")
else()
  target_link_libraries(mitcad-webbrowser PRIVATE libcef_lib)
endif()
# CEF's headers need C++20; MitCad's own code in the module is C++17.
if(APPLE)
  set(mitcad_browser_rpath "@loader_path")
else()
  set(mitcad_browser_rpath "$ORIGIN")
endif()
set_target_properties(mitcad-webbrowser PROPERTIES
  PREFIX ""
  CXX_STANDARD 20
  CXX_STANDARD_REQUIRED ON
  LIBRARY_OUTPUT_DIRECTORY "${MITCAD_BROWSER_COMPONENT_DIR}"
  RUNTIME_OUTPUT_DIRECTORY "${MITCAD_BROWSER_COMPONENT_DIR}"
  # libcef beside it; Qt from the MitCad that loads it.
  BUILD_RPATH "${mitcad_browser_rpath}"
  INSTALL_RPATH "${mitcad_browser_rpath}")
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  # Only its own folder (mitcad#1654): CMake would add the build machine's
  # Qt and CEF folders to the RUNPATH, which then went out in the package,
  # and a user's machine with a folder of that name would have the module
  # take its libraries from there.
  set_target_properties(mitcad-webbrowser PROPERTIES BUILD_WITH_INSTALL_RPATH ON)
endif()
# Built with MitCad, so that a build's MitCad finds its component.
add_dependencies(mitcad mitcad-webbrowser)

# --- The subprocess helper ----------------------------------------------------------
if(APPLE)
  # The helper apps CEF starts: "MitCad Browser Helper.app" and its variants,
  # beside the framework in the component's folder.
  set(mitcad_browser_helpers "")
  foreach(mitcad_helper_variant IN ITEMS ":" "_gpu: (GPU)" "_renderer: (Renderer)" "_plugin: (Plugin)"
                                         "_alerts: (Alerts)")
    string(REPLACE ":" ";" mitcad_helper_parts "${mitcad_helper_variant}")
    list(GET mitcad_helper_parts 0 mitcad_helper_target_suffix)
    list(LENGTH mitcad_helper_parts mitcad_helper_count)
    set(mitcad_helper_name_suffix "")
    if(mitcad_helper_count GREATER 1)
      list(GET mitcad_helper_parts 1 mitcad_helper_name_suffix)
    endif()
    set(mitcad_helper_target "mitcad-webbrowser-helper${mitcad_helper_target_suffix}")
    set(MITCAD_HELPER_NAME "MitCad Browser Helper${mitcad_helper_name_suffix}")
    string(REPLACE "_" "." MITCAD_HELPER_ID "org.mitcad.browser.helper${mitcad_helper_target_suffix}")
    configure_file(cef/HelperInfo.plist.in
                   "${CMAKE_CURRENT_BINARY_DIR}/${mitcad_helper_target}-Info.plist" @ONLY)
    add_executable(${mitcad_helper_target} MACOSX_BUNDLE cef/CefHelper.cpp)
    target_include_directories(${mitcad_helper_target} SYSTEM PRIVATE "${CEF_INCLUDE_PATH}")
    target_link_libraries(${mitcad_helper_target} PRIVATE libcef_dll_wrapper)
    target_compile_options(${mitcad_helper_target} PRIVATE ${mitcad_browser_path_maps})
    set_target_properties(${mitcad_helper_target} PROPERTIES
      OUTPUT_NAME "${MITCAD_HELPER_NAME}"
      MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_BINARY_DIR}/${mitcad_helper_target}-Info.plist"
      CXX_STANDARD 20
      RUNTIME_OUTPUT_DIRECTORY "${MITCAD_BROWSER_COMPONENT_DIR}")
    add_dependencies(mitcad-webbrowser ${mitcad_helper_target})
    list(APPEND mitcad_browser_helpers
         "\"${MITCAD_HELPER_NAME}.app/Contents/MacOS/${MITCAD_HELPER_NAME}\"")
  endforeach()
  string(JOIN ", " mitcad_browser_executables ${mitcad_browser_helpers})
  # CEF's framework beside the module.
  add_custom_command(TARGET mitcad-webbrowser POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_directory
            "${CEF_BINARY_DIR_RELEASE}/Chromium Embedded Framework.framework"
            "${MITCAD_BROWSER_COMPONENT_DIR}/Chromium Embedded Framework.framework"
    VERBATIM)
else()
  add_executable(mitcad-webbrowser-helper WIN32 cef/CefHelper.cpp)
  target_include_directories(mitcad-webbrowser-helper SYSTEM PRIVATE "${CEF_INCLUDE_PATH}")
  # The wrapper before libcef: with --as-needed (the Linux default) a libcef
  # named before anything uses it is dropped and the wrapper's calls stay
  # unresolved.
  target_link_libraries(mitcad-webbrowser-helper PRIVATE libcef_dll_wrapper libcef_lib)
  target_compile_options(mitcad-webbrowser-helper PRIVATE ${mitcad_browser_path_maps})
  if(WIN32)
    target_compile_definitions(mitcad-webbrowser-helper PRIVATE NOMINMAX WIN32_LEAN_AND_MEAN UNICODE _UNICODE)
  endif()
  set_target_properties(mitcad-webbrowser-helper PROPERTIES
    CXX_STANDARD 20
    RUNTIME_OUTPUT_DIRECTORY "${MITCAD_BROWSER_COMPONENT_DIR}"
    BUILD_RPATH "$ORIGIN"
    INSTALL_RPATH "$ORIGIN"
    # Only its own folder, as the module's (above).
    BUILD_WITH_INSTALL_RPATH ON)
  add_dependencies(mitcad-webbrowser mitcad-webbrowser-helper)
  set(mitcad_browser_executables "\"$<TARGET_FILE_NAME:mitcad-webbrowser-helper>\"")
  # CEF's runtime beside the module: libcef and what it loads, its resources
  # and locales (CEF's lists of them, cef_variables.cmake).
  COPY_FILES(mitcad-webbrowser "${CEF_BINARY_FILES}" "${CEF_BINARY_DIR_RELEASE}" "${MITCAD_BROWSER_COMPONENT_DIR}")
  COPY_FILES(mitcad-webbrowser "${CEF_RESOURCE_FILES}" "${CEF_RESOURCE_DIR}" "${MITCAD_BROWSER_COMPONENT_DIR}")
endif()

# --- The component's description, notices and licence -----------------------------
file(GENERATE OUTPUT "${MITCAD_BROWSER_COMPONENT_DIR}/component.json" CONTENT "{
  \"abi\": ${MITCAD_BROWSER_ABI},
  \"version\": \"${MITCAD_BROWSER_VERSION}\",
  \"cef\": \"${MITCAD_CEF_VERSION}\",
  \"platform\": \"${mitcad_browser_platform}\",
  \"module\": \"$<TARGET_FILE_NAME:mitcad-webbrowser>\",
  \"executables\": [${mitcad_browser_executables}]
}
")
configure_file("${PROJECT_SOURCE_DIR}/packaging/browser/THIRD-PARTY-NOTICES.txt"
               "${MITCAD_BROWSER_COMPONENT_DIR}/THIRD-PARTY-NOTICES.txt" @ONLY)
configure_file("${CEF_ROOT}/LICENSE.txt" "${MITCAD_BROWSER_COMPONENT_DIR}/CEF-LICENSE.txt" COPYONLY)

# --- The package ----------------------------------------------------------------------
set(MITCAD_BROWSER_PACKAGE
    "${CMAKE_BINARY_DIR}/mitcad-browser-${MITCAD_BROWSER_VERSION}-${mitcad_browser_platform}.zip")
add_custom_target(browser-package
  COMMAND "${CMAKE_COMMAND}" -E rm -f "${MITCAD_BROWSER_PACKAGE}"
  COMMAND "${CMAKE_COMMAND}" -E tar cf "${MITCAD_BROWSER_PACKAGE}" --format=zip .
  WORKING_DIRECTORY "${MITCAD_BROWSER_COMPONENT_DIR}"
  COMMENT "The web browser component's package ${MITCAD_BROWSER_PACKAGE}"
  VERBATIM)
add_dependencies(browser-package mitcad-webbrowser)
message(STATUS "Component from Web: the web browser component (interface ${MITCAD_BROWSER_ABI}, CEF "
               "${MITCAD_CEF_VERSION}) in ${MITCAD_BROWSER_COMPONENT_DIR}")
