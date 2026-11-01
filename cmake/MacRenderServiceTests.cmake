# SPDX-License-Identifier: MIT
# Included from app/CMakeLists.txt after mitcad-render-service is defined.
if(NOT APPLE OR NOT BUILD_TESTING OR NOT TARGET mitcad-render-service)
  return()
endif()

set(mitcad_render_test_app "${CMAKE_CURRENT_BINARY_DIR}/render-service-test.app")
set(mitcad_render_test_service "${mitcad_render_test_app}/Contents/XPCServices/org.mitcad.renderer.xpc")
set(MITCAD_RENDER_SERVICE_TEST_MIN_VERSION "${CMAKE_OSX_DEPLOYMENT_TARGET}")
if(NOT MITCAD_RENDER_SERVICE_TEST_MIN_VERSION)
  set(MITCAD_RENDER_SERVICE_TEST_MIN_VERSION "14.4")
endif()
configure_file("${PROJECT_SOURCE_DIR}/app/packaging/macos/RenderServiceTest.plist.in"
  "${CMAKE_CURRENT_BINARY_DIR}/RenderServiceTest.plist" @ONLY)

add_executable(test_mac_render_service MACOSX_BUNDLE
  "${PROJECT_SOURCE_DIR}/app/tests/mac_render_service.cpp"
  "${PROJECT_SOURCE_DIR}/app/render/MacRenderClient.mm"
  "${PROJECT_SOURCE_DIR}/app/render/MacFrameSurface.cpp"
  "${PROJECT_SOURCE_DIR}/app/render/FrameMemory.cpp"
  "${PROJECT_SOURCE_DIR}/app/render/MacCommandPayload.cpp")
target_include_directories(test_mac_render_service PRIVATE "${PROJECT_SOURCE_DIR}/app")
target_link_libraries(test_mac_render_service PRIVATE Qt6::Core Qt6::Gui
  "-framework IOSurface" "-framework Foundation" "-framework Security" mitcad_warnings)
target_compile_options(test_mac_render_service PRIVATE "$<$<COMPILE_LANGUAGE:OBJCXX>:-fblocks>")
set_source_files_properties("${PROJECT_SOURCE_DIR}/app/tests/mac_render_service.cpp" PROPERTIES COMPILE_OPTIONS -fblocks)
set_target_properties(test_mac_render_service PROPERTIES
  OUTPUT_NAME render-service-test
  MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_BINARY_DIR}/RenderServiceTest.plist")
add_dependencies(test_mac_render_service mitcad-render-service)
set_property(TARGET test_mac_render_service APPEND PROPERTY LINK_DEPENDS
  "$<TARGET_FILE:mitcad-render-service>" "${MITCAD_CYCLES_RUNTIME}/source.sha256")
add_custom_command(TARGET test_mac_render_service POST_BUILD
  COMMAND "${CMAKE_COMMAND}" -E make_directory "${mitcad_render_test_service}/Contents/MacOS"
  COMMAND "${CMAKE_COMMAND}" -E copy_if_different "$<TARGET_FILE:mitcad-render-service>"
          "${mitcad_render_test_service}/Contents/MacOS/mitcad-render-service"
  COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${CMAKE_CURRENT_BINARY_DIR}/RenderService.plist"
          "${mitcad_render_test_service}/Contents/Info.plist"
  COMMAND "${CMAKE_COMMAND}" -E copy_directory "${MITCAD_CYCLES_RUNTIME}"
          "${mitcad_render_test_app}/Contents/Resources/cycles"
  # A documented launchd service environment tests the macOS 14.4 useResource
  # fallback on current physical hardware before any Cycles device is created.
  COMMAND /usr/libexec/PlistBuddy -c "Add :XPCService:EnvironmentVariables dict"
          "${mitcad_render_test_service}/Contents/Info.plist"
  COMMAND /usr/libexec/PlistBuddy -c "Add :XPCService:EnvironmentVariables:CYCLES_METAL_RESIDENCY_SETS string 0"
          "${mitcad_render_test_service}/Contents/Info.plist"
  COMMAND /usr/bin/codesign --force --sign - "${mitcad_render_test_service}"
  COMMAND /usr/bin/codesign --force --sign - "${mitcad_render_test_app}" VERBATIM)

add_test(NAME app.render_service_macos COMMAND test_mac_render_service --device cpu)
set_tests_properties(app.render_service_macos PROPERTIES TIMEOUT 150)
# Physical-GPU acceptance invokes the same signed executable with --device metal;
# it must use Metal successfully and never silently substitute the CPU.
