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
  commands/ModifyCommands.cpp
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
  files/RemoteTask.hpp)

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
  target_include_directories(test_app PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_app PRIVATE Qt6::Widgets Qt6::Svg mitcad_warnings)
  add_test(NAME app.unit COMMAND test_app)
  set_tests_properties(app.unit PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()
