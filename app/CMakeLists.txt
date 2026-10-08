# SPDX-License-Identifier: MIT
find_package(Qt6 REQUIRED COMPONENTS Svg Network)

qt_add_executable(mitcad
  main.cpp
  MainWindow.cpp
  MainWindow.hpp
  OcctViewer.cpp
  OcctViewer.hpp
  LayoutGrid.cpp
  LayoutGrid.hpp
  PickContext.cpp
  PickContext.hpp
  # The browser, the timeline and the parameters dialog (U3).
  browser/BrowserController.cpp
  browser/BrowserController.hpp
  browser/BrowserPanel.cpp
  browser/BrowserPanel.hpp
  browser/DocumentHost.hpp
  browser/DocumentSnapshot.cpp
  browser/DocumentSnapshot.hpp
  browser/AppearanceDialog.cpp
  browser/AppearanceDialog.hpp
  browser/ParametersDialog.cpp
  browser/ParametersDialog.hpp
  browser/TimelineWidget.cpp
  browser/TimelineWidget.hpp
  # The command framework (COMMANDS.md).
  framework/Appearances.cpp
  framework/Appearances.hpp
  framework/ChromeStyle.cpp
  framework/ChromeStyle.hpp
  framework/Command.cpp
  framework/Command.hpp
  framework/CommandPanel.cpp
  framework/CommandPanel.hpp
  framework/CommandRegistry.cpp
  framework/CommandRegistry.hpp
  framework/CommandSearch.cpp
  framework/CommandSearch.hpp
  framework/CommandSession.cpp
  framework/CommandSession.hpp
  framework/DesktopEntry.cpp
  framework/DesktopEntry.hpp
  framework/Diagnostics.hpp
  framework/Icons.cpp
  framework/Icons.hpp
  framework/Json.hpp
  framework/ManipulatorOverlay.cpp
  framework/ManipulatorOverlay.hpp
  framework/ModelShapes.cpp
  framework/ModelShapes.hpp
  framework/SettingsWindow.cpp
  framework/SettingsWindow.hpp
  framework/Ribbon.cpp
  framework/Ribbon.hpp
  framework/Selection.cpp
  framework/Selection.hpp
  framework/ShortcutDialog.cpp
  framework/ShortcutDialog.hpp
  framework/Dialogs.cpp
  framework/Dialogs.hpp
  framework/Theme.cpp
  framework/Theme.hpp
  framework/TitleBar.cpp
  framework/TitleBar.hpp
  # Command definitions: one registration per toolbar family (U4).
  commands/CommandFactories.hpp
  commands/Commands.hpp
  commands/CommandSupport.cpp
  commands/CommandSupport.hpp
  commands/ConstructCommands.cpp
  commands/CreateCommands.cpp
  commands/ExtrudeCommands.cpp
  commands/FaceCommands.cpp
  commands/FilletCommands.cpp
  commands/HoleCommands.cpp
  commands/InspectCommands.cpp
  commands/JointCommands.cpp
  commands/ModifyCommands.cpp
  # Dragging components the joints move, animating joints (mitcad#55).
  MainWindowJoints.cpp
  commands/MoveCommands.cpp
  commands/PatternCommands.cpp
  commands/PrimitiveCommands.cpp
  commands/SketchCommands.cpp
  commands/SweepCommands.cpp
  # Sketch mode (U2).
  sketch/DrawTools.cpp
  sketch/EditTools.cpp
  sketch/SketchController.cpp
  sketch/SketchController.hpp
  sketch/SketchGeometry.cpp
  sketch/SketchGeometry.hpp
  sketch/SketchModel.cpp
  sketch/SketchModel.hpp
  sketch/SketchOverlay.cpp
  sketch/SketchOverlay.hpp
  sketch/SketchPalette.cpp
  sketch/SketchPalette.hpp
  sketch/SketchSnap.cpp
  sketch/SketchSnap.hpp
  sketch/SketchTool.hpp
  sketch/ToolSupport.hpp
  # The View menu, display settings and dialogs (U5).
  view/ViewController.cpp
  view/ViewController.hpp
  # The File menu: import and export (U6).
  files/FileDialogs.cpp
  files/FileDialogs.hpp
  files/FileFormats.cpp
  files/FileFormats.hpp
  files/F3dImport.cpp
  files/F3dImport.hpp
  files/MainWindowFiles.cpp
  # Background computation (P7): the worker thread, the progress dialog and
  # the main window's jobs.
  MainWindowJobs.cpp
  framework/ComputeProgress.cpp
  framework/ComputeProgress.hpp
  framework/ModelWorker.cpp
  framework/ModelWorker.hpp
  # Autosave to the recovery folder (P8) and the general settings.
  files/Autosave.cpp
  files/Autosave.hpp
  files/MainWindowAutosave.cpp
  framework/AppSettings.cpp
  framework/AppSettings.hpp
  # Recovery of autosaved work (P8c).
  files/Recovery.cpp
  files/Recovery.hpp
  # The result store on disk and its settings, the caches' diagnostics (P7d).
  framework/CacheDiagnostics.cpp
  framework/CacheDiagnostics.hpp
  framework/CachePreferences.cpp
  framework/CachePreferences.hpp
  framework/ResultCache.cpp
  framework/ResultCache.hpp
  # Saving versions in projects with version history (P12d).
  files/MainWindowVersions.cpp
  files/VersionDialogs.cpp
  files/VersionDialogs.hpp
  # The Version History window (P12e).
  files/VersionHistory.cpp
  files/VersionHistory.hpp
  # Help > About Mitcad.
  AboutDialog.cpp
  AboutDialog.hpp
  # Files the system asks to open (Finder, the Dock).
  files/FileOpenFilter.cpp
  files/FileOpenFilter.hpp
  # The in-process UI test driver (MITCAD_TEST_INPUT).
  framework/TestDriver.cpp
  framework/TestDriver.hpp
  # Floating chrome: the cards over the 3D view.
  framework/FloatingLayout.cpp
  framework/FloatingLayout.hpp
  framework/GlassCard.cpp
  framework/GlassCard.hpp
  # Numbers typed with a decimal comma or point (mitcad#2).
  framework/Numbers.cpp
  framework/Numbers.hpp
  # The sketch tools' precision cursor (mitcad#3).
  framework/Cursors.cpp
  framework/Cursors.hpp
  # The UI tests' sync key (mitcad#15).
  framework/TestSync.cpp
  framework/TestSync.hpp
  # Automatic updates (mitcad#9).
  update/UpdateClient.cpp
  update/UpdateClient.hpp
  update/UpdateController.cpp
  update/UpdateController.hpp
  update/UpdateInstaller.cpp
  update/UpdateInstaller.hpp
  update/UpdateNotice.cpp
  update/UpdateNotice.hpp
  update/UpdatePreferences.cpp
  update/UpdatePreferences.hpp
  update/UpdateSettings.cpp
  update/UpdateSettings.hpp
  # 3D Print: bodies to a slicer (mitcad#13).
  files/MainWindowPrint.cpp
  files/Print3d.cpp
  files/Print3d.hpp
  # Remote repositories: connect, open from a remote, sync (P12 remote).
  files/RemoteController.cpp
  files/RemoteController.hpp
  files/RemoteDialogs.cpp
  files/RemoteDialogs.hpp
  files/RemotePreferences.cpp
  files/RemotePreferences.hpp
  files/RemoteTask.cpp
  files/RemoteTask.hpp
  # Feedback and error reports: Help > Send Feedback, crash capture, the
  # preview and the issue form (mitcad#61, mitcad#62).
  report/CrashHandler.cpp
  report/CrashHandler.hpp
  report/ReportCenter.cpp
  report/ReportCenter.hpp
  report/ReportDialogs.cpp
  report/ReportDialogs.hpp
  report/ReportText.cpp
  report/ReportText.hpp
  # Component libraries and the community library (mitcad#64, mitcad#63).
  files/Libraries.cpp
  files/Libraries.hpp
  files/LibraryDialogs.cpp
  files/LibraryDialogs.hpp
  files/MainWindowLibraries.cpp)

# Toolbar and command icons (own drawings, MIT), rendered with Qt SVG.
file(GLOB MITCAD_ICONS RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}" CONFIGURE_DEPENDS icons/*.svg)
qt_add_resources(mitcad icons PREFIX / FILES ${MITCAD_ICONS})

# The application icon (branding/mitcad.svg, rendered by tools/make-icons.sh):
# the window's icon from the PNG files, Explorer's and the taskbar's from
# mitcad.ico in the executable's resources.
file(GLOB MITCAD_APP_ICONS RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}" CONFIGURE_DEPENDS branding/png/*.png)
qt_add_resources(mitcad branding PREFIX / FILES ${MITCAD_APP_ICONS})
if(WIN32)
  target_sources(mitcad PRIVATE branding/mitcad.rc)
endif()

target_include_directories(mitcad PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
target_compile_definitions(mitcad PRIVATE MITCAD_VERSION="${PROJECT_VERSION}")
target_link_libraries(mitcad PRIVATE
  Qt6::Widgets Qt6::OpenGL Qt6::OpenGLWidgets Qt6::Svg
  mitcad_ffi mitcad_geometry
  TKService TKV3d TKOpenGl TKMesh
  mitcad_warnings)

if(WIN32)
  # wglGetCurrentDC() in OcctViewer.cpp
  target_link_libraries(mitcad PRIVATE opengl32)
endif()
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  # The X keyboard mapping in framework/TestSync.cpp.
  find_package(X11 REQUIRED)
  if(NOT TARGET X11::xcb)
    message(FATAL_ERROR "libxcb (libxcb1-dev) is required")
  endif()
  target_link_libraries(mitcad PRIVATE X11::xcb)
endif()

if(APPLE)
  # Qt's and OCCT's OpenGL headers on macOS warn about the deprecated API.
  target_compile_definitions(mitcad PRIVATE GL_SILENCE_DEPRECATION)
endif()

if(APPLE)
  # Native macOS parts (SF Symbols, menu images) in Objective-C++.
  target_sources(mitcad PRIVATE platform/MacChrome.hpp platform/macos/MacChrome.mm)
  set_source_files_properties(platform/macos/MacChrome.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
  target_link_libraries(mitcad PRIVATE "-framework AppKit")
endif()

set_target_properties(mitcad PROPERTIES WIN32_EXECUTABLE ON)

if(APPLE)
  # The application bundle, its icon and install rules (macOS packaging).
  include(packaging/macos/Bundle.cmake)
endif()

# Automatic updates (mitcad#9, docs/updates.md). The requests go through Qt
# Network (its TLS backends are Qt's plugins: no TLS library of Mitcad's
# own). The release keys the manifests and downloads are verified with: the
# file's lines of 64 hex digits (two while the key changes); while it has
# none (the placeholder), Mitcad makes no update request.
target_link_libraries(mitcad PRIVATE Qt6::Network)
set(MITCAD_UPDATE_KEY_FILE "${PROJECT_SOURCE_DIR}/packaging/update-key.txt" CACHE FILEPATH
    "The public Ed25519 keys of Mitcad's releases (lines of 64 hex digits)")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${MITCAD_UPDATE_KEY_FILE}")
file(STRINGS "${MITCAD_UPDATE_KEY_FILE}" mitcad_update_key_lines REGEX "^[0-9a-fA-F]+$")
set(mitcad_update_keys "")
foreach(line IN LISTS mitcad_update_key_lines)
  string(LENGTH "${line}" length)
  if(NOT length EQUAL 64)
    message(FATAL_ERROR "${MITCAD_UPDATE_KEY_FILE}: '${line}' is not a key of 64 hex digits")
  endif()
  string(APPEND mitcad_update_keys " ${line}")
endforeach()
string(STRIP "${mitcad_update_keys}" mitcad_update_keys)
if(NOT mitcad_update_keys)
  message(STATUS "No release key in ${MITCAD_UPDATE_KEY_FILE}: no update checks")
endif()
target_compile_definitions(mitcad PRIVATE MITCAD_UPDATE_KEY="${mitcad_update_keys}")
# Feedback and error reports (mitcad#61, mitcad#62) go to the issue form of
# this tracker, filled in through its address and opened in the browser
# (report/ReportCenter.hpp): an issue list's address (".../issues") or a
# template with {title} and {body}. The setting reports/issueUrl overrides
# it. The default is the issue page README.md names.
set(MITCAD_ISSUE_URL "https://github.com/mitcad/Mitcad/issues" CACHE STRING
    "The issue tracker feedback and error reports open (an issue list's address, or a template with {title} and {body})")
target_compile_definitions(mitcad PRIVATE MITCAD_ISSUE_URL="${MITCAD_ISSUE_URL}")
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  # Mitcad's own functions by name in crash reports' stacks
  # (backtrace_symbols reads the dynamic symbol table): only the symbols
  # of its namespaces, so that nothing of a library linked in statically
  # takes the place of a shared library's.
  target_link_options(mitcad PRIVATE "LINKER:--dynamic-list=${CMAKE_CURRENT_SOURCE_DIR}/report/crash-symbols.list")
  set_property(TARGET mitcad APPEND PROPERTY LINK_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/report/crash-symbols.list")
endif()

if(WIN32)
  # Runs the installer once Mitcad has quit (update/UpdaterWindows.cpp): no
  # Qt, the C++ runtime linked in, so that it runs from the temp folder.
  add_executable(mitcad-updater WIN32 update/UpdaterWindows.cpp)
  set_target_properties(mitcad-updater PROPERTIES MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
  target_link_libraries(mitcad-updater PRIVATE shell32 mitcad_warnings)
  add_dependencies(mitcad mitcad-updater)
endif()

if(BUILD_TESTING)
  # Unit tests of the application's own helpers, on Qt's offscreen platform
  # (no window); Qt's DLLs are not next to the test on Windows.
  qt_add_executable(test_app tests/test_app.cpp)
  # Numbers typed with a decimal comma or point (mitcad#2).
  target_sources(test_app PRIVATE framework/Numbers.cpp)
  # The sketch tools' precision cursor with its tool icons (mitcad#3); the
  # icons' dark variants ask the theme.
  target_sources(test_app PRIVATE framework/Cursors.cpp framework/Icons.cpp framework/Theme.cpp)
  qt_add_resources(test_app test_icons PREFIX / FILES ${MITCAD_ICONS})
  # 3D Print's slicers and file names (mitcad#13).
  target_sources(test_app PRIVATE files/Print3d.cpp files/Print3d.hpp)
  # The AppImage's desktop file and icons in the user's data folder.
  target_sources(test_app PRIVATE framework/DesktopEntry.cpp framework/DesktopEntry.hpp)
  # The rendered view's exposure and view transforms (mitcad#47): plain Qt
  # Gui, also in builds without the renderer.
  target_sources(test_app PRIVATE render/RenderImage.cpp render/RenderImage.hpp)
  # The render device's choice and the CPU's fallback (mitcad#50): plain
  # C++, also in builds without the renderer.
  target_sources(test_app PRIVATE render/RenderDevices.cpp render/RenderDevices.hpp)
  # Feedback and error reports (mitcad#61, mitcad#62): masking, crash files,
  # duplicate keys, the issue form's address.
  target_sources(test_app PRIVATE report/ReportText.cpp report/ReportText.hpp)
  target_include_directories(test_app PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_app PRIVATE Qt6::Widgets Qt6::Svg mitcad_warnings)
  add_test(NAME app.unit COMMAND test_app)
  set_tests_properties(app.unit PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()

# View > Rendered and its render worker mitcad-render (mitcad#32, mitcad#45,
# docs/rendering.md), only with MITCAD_RENDER. The application has the
# client's side and links none of the renderer's libraries; only the
# worker, an executable of its own next to it, links Cycles and the rest.
if(MITCAD_RENDER)
  include("${PROJECT_SOURCE_DIR}/cmake/Render.cmake")
  target_sources(mitcad PRIVATE
    render/FrameMemory.cpp
    render/FrameMemory.hpp
    render/RenderClient.cpp
    render/RenderClient.hpp
    render/RenderDevice.cpp
    render/RenderDevice.hpp
    render/RenderEnvironmentDialog.cpp
    render/RenderEnvironmentDialog.hpp
    render/RenderLights.cpp
    render/RenderLights.hpp
    render/RenderImage.cpp
    render/RenderImage.hpp
    render/RenderMesh.cpp
    render/RenderMesh.hpp
    render/RenderMode.cpp
    render/RenderMode.hpp
    render/RenderProtocol.hpp
    render/RenderScene.cpp
    render/RenderScene.hpp
    render/Renderer.hpp)
  target_compile_definitions(mitcad PRIVATE MITCAD_RENDER)
  target_include_directories(mitcad SYSTEM PRIVATE "${MITCAD_CGLTF_INCLUDE_DIR}")
  # File > Render Image (mitcad#48): the final render in the worker's batch
  # mode; RenderBatch is shared with mitcad-cli render.
  target_sources(mitcad PRIVATE
    render/RenderBatch.cpp
    render/RenderBatch.hpp
    render/RenderImageDialog.cpp
    render/RenderImageDialog.hpp)

  # The worker: no windows, Qt Core and Gui (QImage for --bench --output),
  # OCCT for the test scene, the renderer.
  add_executable(mitcad-render
    render/RenderWorker.cpp
    render/FrameMemory.cpp
    render/FrameMemory.hpp
    render/RenderDevices.cpp
    render/RenderDevices.hpp
    render/RenderImage.cpp
    render/RenderImage.hpp
    render/RenderMesh.cpp
    render/RenderMesh.hpp
    render/RenderProtocol.hpp
    render/RenderScene.cpp
    render/RenderScene.hpp
    render/Renderer.hpp)
  # Crash reports (mitcad#62), with the worker's functions named in their
  # stacks as the application's are.
  target_sources(mitcad-render PRIVATE report/CrashHandler.cpp report/CrashHandler.hpp)
  if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    target_link_options(mitcad-render PRIVATE
      "LINKER:--dynamic-list=${CMAKE_CURRENT_SOURCE_DIR}/report/crash-symbols.list")
  endif()
  # The final render's image files (mitcad#48): PNG and JPEG through Qt,
  # OpenEXR through OpenImageIO.
  target_sources(mitcad-render PRIVATE render/RenderOutput.cpp render/RenderOutput.hpp)
  target_include_directories(mitcad-render PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_include_directories(mitcad-render SYSTEM PRIVATE "${MITCAD_CGLTF_INCLUDE_DIR}")
  target_compile_definitions(mitcad-render PRIVATE MITCAD_VERSION="${PROJECT_VERSION}")
  target_link_libraries(mitcad-render PRIVATE
    mitcad_cycles_renderer Qt6::Gui TKMesh TKFillet TKPrim TKTopAlgo TKBRep mitcad_warnings)
  # Built with the application, into the same folder (the application
  # looks for it next to its executable; a macOS bundle would need it in
  # Contents/MacOS), so that the UI tests find it.
  add_dependencies(mitcad mitcad-render)
  # The GPU devices' kernels (mitcad#50, build-cycles.sh --cuda and the
  # like) where the worker looks for them in a build folder: cycles/lib
  # next to it.
  if(MITCAD_CYCLES_KERNELS)
    add_custom_command(TARGET mitcad-render POST_BUILD
      COMMAND "${CMAKE_COMMAND}" -E make_directory "$<TARGET_FILE_DIR:mitcad-render>/cycles/lib"
      COMMAND "${CMAKE_COMMAND}" -E copy_if_different ${MITCAD_CYCLES_KERNELS}
              "$<TARGET_FILE_DIR:mitcad-render>/cycles/lib/"
      VERBATIM)
  endif()
  if(BUILD_TESTING)
    # The worker renders the test scene headless (no display needed).
    add_test(NAME app.render_worker
             COMMAND mitcad-render --bench 160x120 --samples 4
                     --output "${CMAKE_CURRENT_BINARY_DIR}/render-worker-test.png")
    set_tests_properties(app.render_worker PROPERTIES PASS_REGULAR_EXPRESSION "render-bench 160x120 samples 4")
    # Scene updates change only what changed (mitcad#49).
    add_test(NAME app.render_updates COMMAND mitcad-render --update-test)
    # In an environment of the render settings (mitcad#47): the outdoor sky
    # as the background, a polished ground at a height.
    add_test(NAME app.render_worker_environment
             COMMAND mitcad-render --bench 160x120 --samples 4
                     --environment "{\"environment\": {\"preset\": \"outdoor\", \"sun_elevation\": 0.4}, \"background\": {\"mode\": \"environment\"}, \"ground\": {\"reflections\": true, \"height\": -5}}"
                     --output "${CMAKE_CURRENT_BINARY_DIR}/render-worker-environment-test.png")
    set_tests_properties(app.render_worker_environment PROPERTIES
      PASS_REGULAR_EXPRESSION "render-bench 160x120 samples 4" FAIL_REGULAR_EXPRESSION "render-environment")
    # Render devices (mitcad#50). A device that is not there: the CPU
    # renders, and says why.
    add_test(NAME app.render_worker_fallback
             COMMAND mitcad-render --bench 64x48 --samples 2 --device NO_SUCH_DEVICE)
    set_tests_properties(app.render_worker_fallback PROPERTIES PASS_REGULAR_EXPRESSION
      "render-fallback There is no render device \"NO_SUCH_DEVICE\"; rendering on the CPU.*render-bench 64x48 samples 2.*render-device CPU")
    # A device that fails while it renders (the test hook fails the first
    # one, also the CPU): the CPU renders the same scene and view again.
    add_test(NAME app.render_worker_device_failure COMMAND mitcad-render --bench 64x48 --samples 2)
    set_tests_properties(app.render_worker_device_failure PROPERTIES
      ENVIRONMENT "MITCAD_RENDER_TEST_DEVICE_FAILURE=simulated device failure"
      PASS_REGULAR_EXPRESSION "render-fallback Rendering on the CPU: .* failed \\(simulated device failure\\).*render-bench 64x48 samples 2.*render-device CPU")
    # The best GPU against the CPU: the same test scene, images close to each
    # other (8-bit sRGB: mean difference, 99th percentile). Skipped without a
    # GPU device (a machine without one, or a build without GPU devices).
    add_test(NAME app.render_worker_gpu
             COMMAND mitcad-render --bench 320x240 --samples 64 --device auto --compare cpu --tolerance 2:24
                     --output "${CMAKE_CURRENT_BINARY_DIR}/render-worker-gpu-test.png")
    set_tests_properties(app.render_worker_gpu PROPERTIES SKIP_RETURN_CODE 77
      PASS_REGULAR_EXPRESSION "render-compare (CUDA|OPTIX|HIP|METAL|ONEAPI) ")
  endif()
endif()
