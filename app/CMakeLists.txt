# SPDX-License-Identifier: MIT
find_package(Qt6 REQUIRED COMPONENTS Svg Network)

# Scheduling/activity support also serves the windowless render worker and
# CLI. Keep it independent of the QWidget-based native chrome.
if(APPLE)
  add_library(mitcad_mac_work STATIC platform/MacChrome.hpp platform/macos/MacWork.mm)
  target_include_directories(mitcad_mac_work PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(mitcad_mac_work PUBLIC Qt6::Gui PRIVATE TKernel "-framework Foundation" mitcad_warnings)
  set_source_files_properties(platform/macos/MacWork.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
endif()

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
  # The Browser's background in the Floating layout (mitcad#419).
  browser/BrowserBackground.cpp
  browser/BrowserBackground.hpp
  browser/DocumentHost.hpp
  browser/DocumentSnapshot.cpp
  browser/DocumentSnapshot.hpp
  browser/AppearanceDialog.cpp
  browser/AppearanceDialog.hpp
  # Find in Design (mitcad#350).
  browser/FindDialog.cpp
  browser/FindDialog.hpp
  browser/FindInDesign.cpp
  browser/FindInDesign.hpp
  browser/ParametersDialog.cpp
  browser/ParametersDialog.hpp
  # Property sets of components, occurrences and bodies (mitcad#496).
  browser/PropertySetsDialog.cpp
  browser/PropertySetsDialog.hpp
  # Design policy of components (mitcad#220).
  browser/PolicyDialog.cpp
  browser/PolicyDialog.hpp
  browser/ReorientDialog.cpp
  browser/ReorientDialog.hpp
  browser/ComponentColors.hpp
  browser/TimelineWidget.cpp
  browser/TimelineWidget.hpp
  browser/TimelineSearch.cpp
  browser/TimelineSearch.hpp
  # The command framework (COMMANDS.md).
  framework/Appearances.cpp
  framework/Appearances.hpp
  framework/AutoselectPanel.cpp
  framework/AutoselectPanel.hpp
  framework/ChromeStyle.cpp
  framework/ChromeStyle.hpp
  # Restarting the application (mitcad#419: the layout preference).
  framework/Restart.cpp
  framework/Restart.hpp
  framework/Command.cpp
  framework/Command.hpp
  framework/CommandPresenter.hpp
  framework/CommandMessage.cpp
  framework/CommandMessage.hpp
  framework/CommandPanel.cpp
  framework/CommandPanel.hpp
  framework/CutTargetsPanel.cpp
  framework/CutTargetsPanel.hpp
  framework/CommandRegistry.cpp
  framework/CommandRegistry.hpp
  framework/CommandSearch.cpp
  framework/CommandSearch.hpp
  framework/CommandSession.cpp
  framework/CommandSession.hpp
  framework/DocumentSession.cpp
  framework/DocumentSession.hpp
  framework/DesktopEntry.cpp
  framework/DesktopEntry.hpp
  framework/Diagnostics.hpp
  framework/EventFilterGuard.hpp
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
  framework/DialogFolders.cpp
  framework/DialogFolders.hpp
  framework/FitToScreen.cpp
  framework/FitToScreen.hpp
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
  # Motion studies and contact sets (mitcad#454).
  MainWindowMotion.cpp
  motion/MotionStudyDialog.cpp
  motion/MotionStudyDialog.hpp
  commands/MoveCommands.cpp
  commands/PatternCommands.cpp
  commands/PointPatternCommands.cpp
  commands/PrimitiveCommands.cpp
  commands/SketchCommands.cpp
  commands/SweepCommands.cpp
  # Emboss (mitcad#458).
  commands/EmbossCommands.cpp
  # Gear (mitcad#472).
  commands/GearCommands.cpp
  # Pulley (mitcad#562).
  commands/PulleyCommands.cpp
  # Bearing (mitcad#577).
  commands/BearingCommands.cpp
  # Equal spacing in sketches (mitcad#221).
  commands/EqualSpacingCommands.cpp
  # Surface modelling: the SURFACE group (mitcad#455).
  commands/SurfaceCommands.cpp
  # Sketch mode (U2).
  sketch/AutoConstraint.cpp
  sketch/AutoConstraint.hpp
  sketch/BodyGeometry.cpp
  sketch/BodyGeometry.hpp
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
  view/ViewNavigation.hpp
  # The File menu: import and export (U6).
  files/FileDialogs.cpp
  files/FileDialogs.hpp
  files/FileFormats.cpp
  files/FileFormats.hpp
  files/F3dImport.cpp
  files/F3dImport.hpp
  # An .iam assembly's parts with their design history (mitcad#507).
  files/IamImport.cpp
  files/IamImport.hpp
  files/ImportDeviations.cpp
  files/ImportDeviations.hpp
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
  files/AutosaveFiles.cpp
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
  # Calls after the current event, never during a job (mitcad#888).
  framework/IdleCall.hpp
  platform/WindowsTestSync.hpp
  # Typed text in the UI tests' log (mitcad#102).
  framework/TypedTextLog.cpp
  framework/TypedTextLog.hpp
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
  update/UpdaterChecks.hpp
  # 3D Print: bodies to a slicer (mitcad#13).
  files/MainWindowPrint.cpp
  files/Print3d.cpp
  files/Print3d.hpp
  # Save As Mesh: a body's or a component's bodies to mesh files (mitcad#734).
  files/MainWindowSaveAsMesh.cpp
  files/SaveAsMesh.cpp
  files/SaveAsMesh.hpp
  # Remote repositories: connect, open from a remote, sync (P12 remote).
  files/RemoteController.cpp
  files/RemoteController.hpp
  files/RemoteDialogs.cpp
  files/RemoteDialogs.hpp
  files/RemotePreferences.cpp
  files/RemotePreferences.hpp
  files/RemoteTask.cpp
  files/RemoteTask.hpp
  # Local and Cloud projects: New Project, Open Project, Open from Cloud,
  # Project Settings, the project indicator (mitcad#89).
  files/CloudAddress.cpp
  files/CloudAddress.hpp
  files/CloudSection.cpp
  files/CloudSection.hpp
  files/MainWindowProjects.cpp
  files/ProjectDialogs.cpp
  files/ProjectDialogs.hpp
  files/ProjectIndicator.cpp
  files/ProjectIndicator.hpp
  files/ProjectSettings.cpp
  files/ProjectSettings.hpp
  files/Projects.cpp
  files/Projects.hpp
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
  files/LibraryPaths.cpp
  files/LibraryDialogs.cpp
  files/LibraryDialogs.hpp
  files/MainWindowLibraries.cpp
  # Edit locks of Cloud projects' designs and read-only windows (mitcad#89).
  files/LockController.cpp
  files/LockController.hpp
  files/MainWindowLocks.cpp
  # The Projects panel: projects, folders and designs (mitcad#145).
  projects/MainWindowProjectsPanel.cpp
  projects/ProjectsPanel.cpp
  projects/ProjectsPanel.hpp
  projects/ProjectsTree.cpp
  projects/ProjectsTree.hpp
  # The project picker: a place, a design or a project among the listed
  # projects instead of a file dialog (mitcad#174, mitcad#176).
  projects/ProjectPicker.cpp
  projects/ProjectPicker.hpp
  # Hybrid assemblies: moving the root's features into a component (mitcad#171).
  framework/HybridAssembly.cpp
  framework/MoveToComponentDialog.cpp
  framework/HybridAssembly.hpp
  # Sheet metal: its command group, browser rows and Flat Pattern export
  # (mitcad#210).
  commands/SheetCommands.cpp
  # Plastic part features (mitcad#459).
  commands/PlasticCommands.cpp
  MainWindowSheet.cpp
  # Move to... on a component occurrence: another parent (mitcad#219).
  framework/MoveOccurrenceDialog.cpp
  framework/MoveOccurrenceDialog.hpp)

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
# Narrow strings as UTF-8 on Windows too: the paths the application hands
# the bridge and the geometry as std::string (cmake/Utf8CodePage.cmake).
mitcad_utf8_code_page(mitcad)

# Sketch selection filters: three states per category, body edges in sight
# or behind faces, Shift's wider clicks, Project's sources (mitcad#217).
target_sources(mitcad PRIVATE
  framework/SelectionScope.cpp
  framework/SelectionScope.hpp
  framework/SelectionFilterPanel.cpp
  framework/SelectionFilterPanel.hpp
  MainWindowSelection.cpp)

# The view's context menu: the commands that fit the selection in Solid,
# BUILD and Construct, those it completes first and the others under More;
# Delete for whole things and Hide (mitcad#494, mitcad#347).
target_sources(mitcad PRIVATE MainWindowContextMenu.cpp)

# Parameter names picked from the view into a feature panel's value field:
# the visible sketches' dimensions and the earlier features' values as
# labels while the field has the keyboard (mitcad#250).
target_sources(mitcad PRIVATE
  framework/CommandSessionPicks.cpp
  framework/ParameterPicker.cpp
  framework/ParameterPicker.hpp
  MainWindowParameterPicks.cpp)

# The Docked chrome's window style: raised panels, their titles, one accent
# colour; on Windows 11 the frame through DWM (mitcad#251).
target_sources(mitcad PRIVATE
  framework/WindowStyle.cpp
  framework/WindowStyle.hpp
  platform/WindowsFrame.hpp)
if(WIN32)
  target_sources(mitcad PRIVATE platform/windows/WindowsFrame.cpp)
  # The UI tests' sync key on Windows (mitcad#271).
  target_sources(mitcad PRIVATE platform/windows/WindowsTestSync.cpp)
  # DwmSetWindowAttribute: dwmapi.dll is in every Windows since Vista; the
  # Windows 11 attributes are skipped at run time on older builds.
  target_link_libraries(mitcad PRIVATE dwmapi)
endif()

# Which bodies a feature's preview draws and where: hidden ones stay
# hidden, placed and linked components' bodies where their occurrences put
# them (mitcad#266).
target_sources(mitcad PRIVATE
  framework/PreviewBodies.cpp
  framework/PreviewBodies.hpp)

# The Sketch Palette's warnings as links that select what they are about,
# the view panned to them (mitcad#264).
target_sources(mitcad PRIVATE
  sketch/PaletteWarnings.cpp
  sketch/PaletteWarnings.hpp
  sketch/SketchReveal.cpp
  view/ViewReveal.cpp
  view/ViewReveal.hpp)

# The debug log: Preferences' switch, --debug-log, its files with their
# rotation, Help > Open Log Folder (mitcad#269).
target_sources(mitcad PRIVATE
  framework/DebugLog.cpp
  framework/DebugLog.hpp
  MainWindowDebugLog.cpp)
# The scoped wait cursor of the jobs the window waits for, its log lines
# and its idle recovery (mitcad#268).
target_sources(mitcad PRIVATE
  framework/WaitCursor.cpp
  framework/WaitCursor.hpp)

target_include_directories(mitcad PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
target_compile_definitions(mitcad PRIVATE MITCAD_VERSION="${PROJECT_VERSION}")
target_link_libraries(mitcad PRIVATE
  Qt6::Widgets Qt6::OpenGL Qt6::OpenGLWidgets Qt6::Svg
  mitcad_ffi mitcad_geometry
  TKService TKV3d TKOpenGl TKMesh
  mitcad_warnings)
# The view's meshes of large curved faces (mitcad#398): mesh.hpp's
# bounded meshing, also on the model's worker thread at the end of jobs.
target_link_libraries(mitcad PRIVATE mitcad_geometry_io)
target_sources(mitcad PRIVATE
  framework/DisplayMeshes.cpp
  framework/DisplayMeshes.hpp)
# The process's end without static destructors while a cancelled display
# meshing's face or a given-up import still runs (mitcad#936).
target_sources(mitcad PRIVATE
  framework/ProcessEnd.cpp
  framework/ProcessEnd.hpp)
# A dragged Extrude's provisional tool while its exact preview computes
# (mitcad#636): an owned copy of the tool's mesh, changed on each move.
target_sources(mitcad PRIVATE
  framework/CommandSessionProxy.cpp
  framework/DragProxy.cpp
  framework/DragProxy.hpp
  commands/ExtrudeDragProxy.cpp
  commands/ExtrudeDragProxy.hpp
  view/ProvisionalMesh.cpp)
# A dragged Move/Copy's bodies or components follow the handle at once
# (mitcad#838): their meshes moved, the exact preview after the drag.
target_sources(mitcad PRIVATE
  commands/MoveDragMotion.cpp
  commands/MoveDragMotion.hpp)
# What Extrude's and Revolve's Profiles take with a click: a face alone or
# sketch items, a mixing click refused with a hint (mitcad#868).
target_sources(mitcad PRIVATE
  commands/ProfilePicks.cpp
  commands/ProfilePicks.hpp)
# A sketch's normal in the design through the occurrences it is seen at:
# Extrude's arrow in a turned component (mitcad#958).
target_sources(mitcad PRIVATE
  commands/SketchNormal.cpp
  commands/SketchNormal.hpp)
# The Hole panel's fields that follow a screw size, their marks and Reset
# to size (mitcad#903, mitcad#980).
target_sources(mitcad PRIVATE
  commands/HoleSizes.cpp
  commands/HoleSizes.hpp)

if(WIN32)
  # wglGetCurrentDC() in OcctViewer.cpp
  target_link_libraries(mitcad PRIVATE opengl32)
  # Crash reports' stacks and function names (report/CrashHandler.cpp).
  target_link_libraries(mitcad PRIVATE dbghelp)
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
  target_link_libraries(mitcad PRIVATE mitcad_mac_work)
  target_sources(mitcad PRIVATE platform/MacChrome.hpp platform/macos/MacChrome.mm)
  set_source_files_properties(platform/macos/MacChrome.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
  target_link_libraries(mitcad PRIVATE "-framework AppKit")
  if(MITCAD_MACOS_NATIVE_UI)
    target_link_libraries(mitcad PRIVATE "-framework UniformTypeIdentifiers" "-framework CoreGraphics")
    target_compile_definitions(mitcad PRIVATE MITCAD_MACOS_NATIVE_UI=1)
    target_sources(mitcad PRIVATE
      MainWindowNative.cpp
      platform/NativeMacWindow.hpp platform/macos/NativeMacWindow.mm
      platform/NativeForms.hpp platform/macos/NativeForms.mm
      platform/NativeDialogs.hpp platform/macos/NativeDialogs.mm
      platform/NativeBindingsCard.hpp platform/macos/NativeBindingsCard.mm
      platform/NativeInlineEditor.hpp platform/macos/NativeInlineEditor.mm
      platform/NativeTestInput.hpp platform/macos/NativeTestInput.mm
      platform/macos/NativeTableCells.hpp platform/macos/NativeTableCells.mm
      platform/macos/NativeRuleCanvas.hpp platform/macos/NativeRuleCanvas.mm
      platform/macos/NativeNavigationGuide.hpp platform/macos/NativeNavigationGuide.mm)
    set_source_files_properties(
      platform/macos/NativeMacWindow.mm platform/macos/NativeForms.mm
      platform/macos/NativeDialogs.mm platform/macos/NativeTestInput.mm
      platform/macos/NativeBindingsCard.mm
      platform/macos/NativeInlineEditor.mm
      platform/macos/NativeTableCells.mm platform/macos/NativeRuleCanvas.mm
      platform/macos/NativeNavigationGuide.mm
      PROPERTIES COMPILE_OPTIONS -fobjc-arc)
    # The native Projects outline's name editor (mitcad#816).
    target_sources(mitcad PRIVATE platform/macos/NativeProjects.hpp platform/macos/NativeProjects.mm)
    set_source_files_properties(platform/macos/NativeProjects.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
    # Tiles and expression tools shared by forms and dialogs (mitcad#815).
    target_sources(mitcad PRIVATE platform/macos/NativeControls.hpp platform/macos/NativeControls.mm)
    set_source_files_properties(platform/macos/NativeControls.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
    # Tile grids, catalogues, split panes and utility panels of the generic
    # presenter bridge (mitcad#813, mitcad#819).
    target_sources(mitcad PRIVATE platform/macos/NativeCollection.hpp platform/macos/NativeCollection.mm)
    set_source_files_properties(platform/macos/NativeCollection.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
    # The shared layout sizing contract and dialog button order (mitcad#812, #817).
    target_sources(mitcad PRIVATE platform/macos/NativeLayout.hpp platform/macos/NativeLayout.mm)
    set_source_files_properties(platform/macos/NativeLayout.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
    # The widget tree whose change rebuilds a presenter's native controls
    # (mitcad#986): plain Qt Widgets.
    target_sources(mitcad PRIVATE platform/NativeStructure.hpp platform/NativeStructure.cpp)
  endif()
endif()

set_target_properties(mitcad PROPERTIES WIN32_EXECUTABLE ON)

# Automatic updates (mitcad#9, docs/updates.md). The requests go through Qt
# Network (its TLS backends are Qt's plugins: no TLS library of Mitcad's
# own). The release keys the manifests and downloads are verified with: the
# file's lines of 64 hex digits (two while the key changes); while it has
# none (the placeholder), Mitcad makes no update request.
target_link_libraries(mitcad PRIVATE Qt6::Network)
# MCP connections target the open GUI document, using the shared protocol
# through the FFI rather than creating an invisible headless document.
target_sources(mitcad PRIVATE
  MainWindowMcp.cpp
  mcp/LocalEndpoint.cpp mcp/LocalEndpoint.hpp
  mcp/LiveServer.cpp mcp/LiveServer.hpp
  mcp/EndpointLimits.cpp mcp/EndpointLimits.hpp)
# Live MCP for debugging (mitcad#265): queries and sketch commands while a
# sketch is open, the UI's state, log and view, selections, UI commands by
# id, Save.
target_sources(mitcad PRIVATE
  MainWindowMcpUi.cpp
  framework/CommandPanelState.cpp
  framework/CommandSessionScript.cpp
  mcp/LiveErrors.hpp
  mcp/LogRing.cpp mcp/LogRing.hpp
  mcp/ModalGuard.cpp mcp/ModalGuard.hpp)
# Live MCP's Save As and Open into the folders for designs
# (mitcad#402).
target_sources(mitcad PRIVATE mcp/AgentFolders.cpp mcp/AgentFolders.hpp)
target_sources(mitcad PRIVATE
  mcp/AssistantSetup.cpp mcp/AssistantSetup.hpp
  mcp/ConnectAssistantDialog.cpp mcp/ConnectAssistantDialog.hpp
  mcp/MainWindowAssistant.cpp
  framework/TestDriverAssistant.cpp)
# The agent indicator beside the project indicator (mitcad#710).
target_sources(mitcad PRIVATE mcp/AgentIndicator.cpp mcp/AgentIndicator.hpp)
# The live MCP endpoint's listening socket: on Windows WinSock's own, with
# SO_EXCLUSIVEADDRUSE (mitcad#945).
target_sources(mitcad PRIVATE mcp/LoopbackListener.cpp mcp/LoopbackListener.hpp)
if(WIN32)
  target_link_libraries(mitcad PRIVATE ws2_32)
endif()
# The MCP Bundle contains this Qt-free launcher, which starts the installed
# CLI directly so its own runtime paths remain valid after extraction.
add_executable(mitcad-mcp-launcher mcp/AssistantLauncher.cpp)
target_link_libraries(mitcad-mcp-launcher PRIVATE mitcad_warnings)
mitcad_utf8_code_page(mitcad-mcp-launcher)
if(WIN32)
  set_target_properties(mitcad-mcp-launcher PROPERTIES MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
elseif(APPLE)
  set_target_properties(mitcad-mcp-launcher PROPERTIES RUNTIME_OUTPUT_DIRECTORY "$<TARGET_FILE_DIR:mitcad>")
endif()
add_dependencies(mitcad mitcad-mcp-launcher)
if(WIN32)
  target_sources(mitcad PRIVATE mcp/WindowsEndpointSecurity.cpp mcp/WindowsEndpointSecurity.hpp)
  # shlwapi: whether an application takes Claude Desktop's extension files
  # (ConnectAssistantDialog, mitcad#706).
  target_link_libraries(mitcad PRIVATE advapi32 shlwapi)
endif()
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
# The tests' environment variables only in a test build (mitcad#921, the
# option in the top CMakeLists.txt).
if(MITCAD_UPDATE_TEST_HOOKS)
  target_compile_definitions(mitcad PRIVATE MITCAD_UPDATE_TEST_HOOKS=1)
endif()
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

# Live updates through an MQTT broker (mitcad#89, files/LiveController.hpp):
# the core's LiveHub, the trust and sign-in questions, the brokers' user
# names and passwords in the system's keychain through QtKeychain (linked
# statically, cmake/Keychain.cmake).
target_sources(mitcad PRIVATE
  files/Keychain.cpp
  files/Keychain.hpp
  files/LiveBrokers.cpp
  files/LiveBrokers.hpp
  files/LiveController.cpp
  files/LiveController.hpp
  files/LiveDialogs.cpp
  files/LiveDialogs.hpp
  files/LiveLink.hpp
  files/MainWindowLive.cpp)
target_link_libraries(mitcad PRIVATE qt6keychain)

if(WIN32)
  # Runs the installer once Mitcad has quit (update/UpdaterWindows.cpp): no
  # Qt, the C++ runtime linked in, so that it runs from the temp folder. It
  # hashes the installer with the system's CNG (bcrypt) before it runs it
  # (mitcad#603).
  add_executable(mitcad-updater WIN32 update/UpdaterWindows.cpp update/UpdaterChecks.hpp)
  set_target_properties(mitcad-updater PROPERTIES MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
  target_link_libraries(mitcad-updater PRIVATE shell32 advapi32 bcrypt mitcad_warnings)
  add_dependencies(mitcad mitcad-updater)
endif()

if(APPLE)
  # The helper is copied out of the signed bundle before that bundle is replaced.
  add_executable(mitcad-updater update/UpdaterMacos.cpp)
  target_link_libraries(mitcad-updater PRIVATE mitcad_warnings)
  set_target_properties(mitcad-updater PROPERTIES RUNTIME_OUTPUT_DIRECTORY "$<TARGET_FILE_DIR:mitcad>")
  add_dependencies(mitcad mitcad-updater)
  qt_add_resources(mitcad macos_updater PREFIX /update BASE update FILES update/UpdaterMacos.sh)
endif()

# The parameter table's numbers on a shared decimal axis (mitcad#566).
target_sources(mitcad PRIVATE
  browser/DecimalAlign.cpp
  browser/DecimalAlign.hpp
  browser/DecimalAlignDelegate.cpp
  browser/DecimalAlignDelegate.hpp)

# Suppress When and Show When (mitcad#809), and the Appearances and
# Materials windows' assignment panel with Varies with (mitcad#861).
target_sources(mitcad PRIVATE
  browser/AssignmentPanel.cpp
  browser/AssignmentPanel.hpp
  browser/Bindings.cpp
  browser/Bindings.hpp)
# Configurations (mitcad#807): the Configurations window, the
# Configuration bar in the browser and the configurations' JSON; the Rules
# page is mitcad#808's (browser/ConfigurationRulesPage.cpp).
target_sources(mitcad PRIVATE
  browser/ConfigurationBar.cpp
  browser/ConfigurationBar.hpp
  browser/ConfigurationRulesPage.cpp
  browser/ConfigurationsData.cpp
  browser/ConfigurationsData.hpp
  browser/ConfigurationsDialog.cpp
  browser/ConfigurationsDialog.hpp)
# Bindings at a glance (mitcad#862): the design's bindings in words, the
# fx card of the browser's and the timeline's badges, and the
# Configurations window's Bindings tab.
target_sources(mitcad PRIVATE
  browser/BindingList.cpp
  browser/BindingList.hpp
  browser/BindingsCard.cpp
  browser/BindingsCard.hpp
  browser/ConfigurationBindingsPage.cpp
  browser/ConfigurationBindingsPage.hpp)

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
  # Save As Mesh's bodies and file names (mitcad#734).
  target_sources(test_app PRIVATE tests/save_as_mesh.cpp files/SaveAsMesh.cpp files/SaveAsMesh.hpp)
  target_sources(test_app PRIVATE framework/Dialogs.cpp framework/Dialogs.hpp)
  # The AppImage's desktop file and icons in the user's data folder.
  target_sources(test_app PRIVATE framework/DesktopEntry.cpp framework/DesktopEntry.hpp)
  # The rendered view's exposure and view transforms (mitcad#47): plain Qt
  # Gui.
  target_sources(test_app PRIVATE render/RenderImage.cpp render/RenderImage.hpp)
  # The render device's choice and the CPU's fallback (mitcad#50): plain
  # C++.
  target_sources(test_app PRIVATE render/RenderDevices.cpp render/RenderDevices.hpp)
  # Feedback and error reports (mitcad#61, mitcad#62): masking, crash files,
  # duplicate keys, the issue form's address.
  target_sources(test_app PRIVATE report/ReportText.cpp report/ReportText.hpp)
  # The addresses of Cloud projects (mitcad#89): composed and read back.
  target_sources(test_app PRIVATE files/CloudAddress.cpp files/CloudAddress.hpp)
  # Atomic autosave generations through normal recovery (mitcad#110).
  target_sources(test_app PRIVATE tests/autosave.cpp files/AutosaveFiles.cpp files/Recovery.cpp framework/Dialogs.cpp)
  # The timeline search's words and filters (mitcad#179): plain Qt Core.
  target_sources(test_app PRIVATE browser/TimelineSearch.cpp browser/TimelineSearch.hpp)
  # The command panel's message within narrow panels (mitcad#198): plain Qt
  # Widgets.
  target_sources(test_app PRIVATE tests/command_message.cpp framework/CommandMessage.cpp framework/CommandMessage.hpp)
  # Explicit move destinations and cancellation (mitcad#201): modal Qt Widgets.
  target_sources(test_app PRIVATE tests/move_to_component.cpp framework/MoveToComponentDialog.cpp)
  # Persistent assembly-cut exclusions and preview-fed checklist (mitcad#200).
  target_sources(test_app PRIVATE tests/cut_targets.cpp framework/CutTargetsPanel.cpp framework/CutTargetsPanel.hpp)
  # The sketch's auto constraints while drawing (mitcad#189): plain Qt Core
  # and plane geometry.
  target_sources(test_app PRIVATE sketch/AutoConstraint.cpp sketch/AutoConstraint.hpp sketch/SketchGeometry.cpp)
  # Selection inputs' entries: a click toggles one per region (mitcad#199):
  # plain Qt Core.
  target_sources(test_app PRIVATE framework/Selection.cpp framework/Selection.hpp)
  # Solid-mode profile groups share the actual selection and an independent hover.
  target_sources(test_app PRIVATE tests/autoselect.cpp framework/AutoselectPanel.cpp framework/AutoselectPanel.hpp)
  # When a navigation of the view ends (mitcad#214): plain C++.
  target_sources(test_app PRIVATE view/ViewNavigation.hpp)
  # Where an orthographic render's camera stands (mitcad#215): plain C++.
  target_sources(test_app PRIVATE render/RenderCamera.hpp)
  # Event filters on the 3D view when the main window closes (mitcad#226):
  # plain Qt Widgets.
  target_sources(test_app PRIVATE tests/event_filter_guard.cpp framework/EventFilterGuard.hpp)
  # Move to... on an occurrence: the parent picker and cancellation
  # (mitcad#219): modal Qt Widgets.
  target_sources(test_app PRIVATE tests/move_occurrence.cpp framework/MoveOccurrenceDialog.cpp)
  # Sketch selection filters' three states, Shift and edge sight (mitcad#217):
  # plain Qt Core.
  target_sources(test_app PRIVATE tests/selection_scope.cpp framework/SelectionScope.cpp
                 framework/SelectionScope.hpp)
  # The icons' colour roles on both themes and the disabled rule (mitcad#230).
  target_sources(test_app PRIVATE tests/icon_colours.cpp)
  # A feature preview's bodies in placed, linked and hidden components
  # (mitcad#266): plain Qt Core.
  target_sources(test_app PRIVATE tests/preview_bodies.cpp framework/PreviewBodies.cpp framework/PreviewBodies.hpp)
  # The debug log's files: rotation, the folder's limits, the header, the
  # command line (mitcad#269): plain Qt Core and the log ring.
  target_sources(test_app PRIVATE tests/debug_log.cpp framework/DebugLog.cpp framework/DebugLog.hpp
                 mcp/LogRing.cpp mcp/LogRing.hpp)
  # The wait cursor: nested guards, the progress dialog's release, the
  # log lines, the idle recovery (mitcad#268): plain Qt Widgets.
  target_sources(test_app PRIVATE tests/wait_cursor.cpp framework/WaitCursor.cpp framework/WaitCursor.hpp)
  # The sketch snapper's body vertices and edge midpoints (mitcad#258): the
  # sketch model's data with OCCT's geometry types.
  target_sources(test_app PRIVATE tests/sketch_snap.cpp sketch/SketchSnap.cpp sketch/BodyGeometry.cpp)
  # The maps of a texture set and their files for the renderer (mitcad#314):
  # plain Qt Gui.
  target_sources(test_app PRIVATE tests/texture_maps.cpp framework/Appearances.cpp framework/Appearances.hpp)
  # The rendered view's meshes face out of their solids however the
  # triangulations wind (mitcad#319): OCCT's shapes and meshing.
  target_sources(test_app PRIVATE tests/render_mesh.cpp render/RenderMesh.cpp render/RenderMesh.hpp)
  target_link_libraries(test_app PRIVATE TKMesh)
  # The folders for designs, which live MCP agents save to and open from (mitcad#402):
  # plain Qt Core and Widgets.
  target_sources(test_app PRIVATE tests/agent_folders.cpp mcp/AgentFolders.cpp mcp/AgentFolders.hpp)
  # The file dialogs' folders per group (mitcad#498): plain Qt Core.
  target_sources(test_app PRIVATE tests/dialog_folders.cpp framework/DialogFolders.cpp framework/DialogFolders.hpp)
  # An .iam assembly's import choice, progress lines and parts' outcomes
  # (mitcad#507): plain Qt Core.
  target_sources(test_app PRIVATE tests/iam_import.cpp files/IamImport.cpp files/IamImport.hpp)
  # A renamed command's former id and saved shortcut (mitcad#503): the
  # registry with the command definitions it keeps.
  target_sources(test_app PRIVATE tests/command_alias.cpp framework/CommandRegistry.cpp framework/Command.cpp)
  # The parameter table's numbers on a shared decimal axis (mitcad#566):
  # the number split, the column layout and the delegate's placement; plain
  # Qt Core and Widgets.
  target_sources(test_app PRIVATE tests/decimal_align.cpp browser/DecimalAlign.cpp browser/DecimalAlign.hpp
                 browser/DecimalAlignDelegate.cpp browser/DecimalAlignDelegate.hpp)
  # A scroll area as tall as its contents and a dialog sized to them within
  # the screen (mitcad#631): plain Qt Widgets.
  target_sources(test_app PRIVATE tests/fit_to_screen.cpp framework/FitToScreen.cpp framework/FitToScreen.hpp)
  # The configurations' JSON as the application reads and sends it
  # (mitcad#807): plain Qt Core.
  target_sources(test_app PRIVATE tests/configurations_data.cpp browser/ConfigurationsData.cpp
                 browser/ConfigurationsData.hpp)
  # What Extrude's and Revolve's Profiles take with a click (mitcad#868):
  # plain Qt Core.
  target_sources(test_app PRIVATE tests/profile_picks.cpp commands/ProfilePicks.cpp commands/ProfilePicks.hpp)
  # The live MCP endpoint's limits against a hostile local client
  # (mitcad#935): refusal echoes, the log ring's bytes, the connection
  # slots and the token's comparison; plain Qt Core and the log ring.
  target_sources(test_app PRIVATE tests/mcp_endpoint_limits.cpp mcp/EndpointLimits.cpp mcp/EndpointLimits.hpp)
  # A sketch's normal in turned, nested and oblique occurrences (mitcad#958):
  # Qt Core and OCCT's math.
  target_sources(test_app PRIVATE tests/sketch_normal.cpp commands/SketchNormal.cpp commands/SketchNormal.hpp)
  # The Hole panel's fields that follow a screw size (mitcad#980): Qt Core
  # and the command state, with a model of the size tables.
  target_sources(test_app PRIVATE tests/hole_sizes.cpp commands/HoleSizes.cpp commands/HoleSizes.hpp)
  # A scroll area's own children changing places rebuild no native
  # presenter, a widget added or removed or reordered elsewhere does
  # (mitcad#986): plain Qt Widgets.
  target_sources(test_app PRIVATE tests/native_structure.cpp platform/NativeStructure.cpp
                 platform/NativeStructure.hpp)
  target_link_libraries(test_app PRIVATE mitcad_geometry)
  target_include_directories(test_app PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_app PRIVATE Qt6::Widgets Qt6::Svg mitcad_warnings)
  if(APPLE)
    target_link_libraries(test_app PRIVATE mitcad_mac_work)
    # Widget helpers used by unit tests render the same SF Symbols as the app.
    target_sources(test_app PRIVATE platform/macos/MacChrome.mm)
  endif()
  add_test(NAME app.unit COMMAND test_app)
  # The viewer opens an X display even on Qt's offscreen platform (OCCT's
  # Aspect_DisplayConnection): on Linux a hidden one of its own, so that the
  # test neither needs nor uses the desktop's (the release's build root has
  # none).
  set(default_shortcuts_prefix "")
  if(UNIX AND NOT APPLE)
    find_program(MITCAD_XVFB_RUN_EXECUTABLE xvfb-run)
    if(MITCAD_XVFB_RUN_EXECUTABLE)
      set(default_shortcuts_prefix "${MITCAD_XVFB_RUN_EXECUTABLE}" -a -s "-screen 0 1280x1024x24")
    endif()
  endif()
  add_test(NAME app.default_shortcuts COMMAND ${default_shortcuts_prefix} $<TARGET_FILE:mitcad> --ui=qt --no-recovery --no-mcp)
  set_tests_properties(app.default_shortcuts PROPERTIES TIMEOUT 90
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;MITCAD_TEST_DEFAULT_SHORTCUTS=1;MITCAD_SETTINGS_DIR=${CMAKE_CURRENT_BINARY_DIR}/default-shortcuts-settings;MITCAD_AUTOSAVE_DIR=${CMAKE_CURRENT_BINARY_DIR}/default-shortcuts-autosave;MITCAD_RESULT_STORE=off"
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  # Verify the shared Rust/GUI cache path against the platform's actual Qt path.
  add_executable(test_library_paths tests/library_paths.cpp files/LibraryPaths.cpp)
  target_include_directories(test_library_paths PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_library_paths PRIVATE mitcad_ffi Qt6::Core mitcad_warnings)
  add_test(NAME app.library_paths COMMAND test_library_paths)
  set_tests_properties(app.library_paths PROPERTIES
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  add_executable(test_view_tessellation tests/view_tessellation.cpp)
  target_include_directories(test_view_tessellation PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_view_tessellation PRIVATE mitcad_geometry_io mitcad_geometry TKV3d TKPrim TKBO mitcad_warnings)
  add_test(NAME app.view_tessellation COMMAND test_view_tessellation
    "${CMAKE_CURRENT_BINARY_DIR}/view-tessellation-files")
  # A dragged Extrude's preview meshes on the synthetic 83-hole fixture
  # (mitcad#635): the model's commands and the preview job's meshing,
  # without a window; `--bench` times them.
  add_executable(test_preview_meshing tests/preview_meshing.cpp framework/DisplayMeshes.cpp)
  target_include_directories(test_preview_meshing PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_preview_meshing PRIVATE Qt6::Core mitcad_ffi mitcad_geometry mitcad_geometry_io
    TKMesh TKPrim TKTopAlgo TKBRep TKV3d mitcad_warnings)
  # A cancelled display meshing's endless face at the process's end
  # (mitcad#936): the application's end and its crash reports.
  target_sources(test_preview_meshing PRIVATE framework/ProcessEnd.cpp report/CrashHandler.cpp)
  if(WIN32)
    target_link_libraries(test_preview_meshing PRIVATE dbghelp)
  endif()
  # A dragged Extrude's provisional tool (mitcad#636): the owned mesh copy,
  # its stretches and moves, and Extrude's rules, without a window; Move/
  # Copy's provisional motion (mitcad#838).
  add_executable(test_drag_proxy tests/drag_proxy.cpp framework/DragProxy.cpp commands/ExtrudeDragProxy.cpp
    framework/Command.cpp commands/MoveDragMotion.cpp)
  target_include_directories(test_drag_proxy PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_drag_proxy PRIVATE Qt6::Core Qt6::Gui mitcad_geometry TKMesh TKPrim TKTopAlgo TKBRep
    mitcad_warnings)
  add_test(NAME app.drag_proxy COMMAND test_drag_proxy)
  set_tests_properties(app.drag_proxy PROPERTIES
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  add_test(NAME app.preview_meshing COMMAND test_preview_meshing)
  set_tests_properties(app.preview_meshing PROPERTIES TIMEOUT 300
    ENVIRONMENT "MITCAD_RESULT_STORE=off"
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  # Check renderer device policy and fallback independently of UI helpers.
  add_test(NAME app.render_devices COMMAND test_app --render-devices)
  set_tests_properties(app.unit app.render_devices PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()

# View > Rendered and its render worker mitcad-render (mitcad#32, mitcad#45,
# docs/rendering.md), in every build. The application has the
# client's side and links none of the renderer's libraries; only the
# worker, an executable of its own next to it, links Cycles and the rest.
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
target_include_directories(mitcad SYSTEM PRIVATE "${MITCAD_CGLTF_INCLUDE_DIR}")
# File > Render Image (mitcad#48): the final render in the worker's batch
# mode; RenderBatch is shared with mitcad-cli render.
target_sources(mitcad PRIVATE
  render/RenderBatch.cpp
  render/RenderBatch.hpp
  render/RenderImageDialog.cpp
  render/RenderImageDialog.hpp)
# The browser's light bulbs in every mode, outside a command's undo range
# (mitcad#295), and Create Sketch's origin planes (mitcad#844).
target_sources(mitcad PRIVATE
  framework/CommandSessionVisibility.cpp
  MainWindowVisibility.cpp)

# A same-process IOSurface display prerequisite. The production frame
# protocol and the interactive worker still use shared memory.
if(APPLE AND BUILD_TESTING)
  add_executable(test_mac_frame_surface tests/mac_frame_surface.cpp
    render/MacFrameSurface.cpp render/MacFrameDisplay.cpp render/RenderImage.cpp render/FrameMemory.cpp)
  target_include_directories(test_mac_frame_surface PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_compile_definitions(test_mac_frame_surface PRIVATE GL_SILENCE_DEPRECATION)
  target_link_libraries(test_mac_frame_surface PRIVATE Qt6::Gui Qt6::OpenGL TKOpenGl
    "-framework IOSurface" "-framework OpenGL" mitcad_warnings)
  add_test(NAME app.render_surface_macos COMMAND test_mac_frame_surface)
  set_tests_properties(app.render_surface_macos PROPERTIES RUN_SERIAL TRUE)
endif()

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
mitcad_utf8_code_page(mitcad-render)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  target_link_options(mitcad-render PRIVATE
    "LINKER:--dynamic-list=${CMAKE_CURRENT_SOURCE_DIR}/report/crash-symbols.list")
endif()
if(WIN32)
  target_link_libraries(mitcad-render PRIVATE dbghelp)
endif()
# The final render's image files (mitcad#48): PNG and JPEG through Qt,
# OpenEXR through OpenImageIO.
target_sources(mitcad-render PRIVATE render/RenderOutput.cpp render/RenderOutput.hpp)
target_include_directories(mitcad-render PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
target_include_directories(mitcad-render SYSTEM PRIVATE "${MITCAD_CGLTF_INCLUDE_DIR}")
target_compile_definitions(mitcad-render PRIVATE MITCAD_VERSION="${PROJECT_VERSION}")
target_link_libraries(mitcad-render PRIVATE
  mitcad_cycles_renderer Qt6::Gui TKMesh TKFillet TKPrim TKTopAlgo TKGeomBase TKG3d TKBRep mitcad_warnings)
# Built with the application. On macOS the worker lives in Contents/MacOS
# in the build bundle too, where the application's ordinary lookup finds it.
if(APPLE)
  target_link_libraries(mitcad-render PRIVATE mitcad_mac_work)
  set_target_properties(mitcad-render PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "$<TARGET_FILE_DIR:mitcad>"
    INSTALL_RPATH "@executable_path/../Frameworks")
  target_link_options(mitcad-render PRIVATE LINKER:-headerpad_max_install_names)
endif()
add_dependencies(mitcad mitcad-render)
if(APPLE)
  get_target_property(mitcad_frame_sources mitcad-render SOURCES)
  get_target_property(mitcad_frame_libraries mitcad-render LINK_LIBRARIES)
  get_target_property(mitcad_frame_includes mitcad-render INCLUDE_DIRECTORIES)
  get_target_property(mitcad_frame_definitions mitcad-render COMPILE_DEFINITIONS)
  add_executable(mitcad-render-service ${mitcad_frame_sources}
    render/MacRenderService.mm render/MacFrameSurface.cpp render/MacFrameMetal.mm render/MacCommandPayload.cpp)
  target_include_directories(mitcad-render-service PRIVATE ${mitcad_frame_includes})
  target_include_directories(mitcad-render-service SYSTEM PRIVATE "${MITCAD_CGLTF_INCLUDE_DIR}")
  target_compile_definitions(mitcad-render-service PRIVATE ${mitcad_frame_definitions}
    MITCAD_RENDER_RUNTIME_ENTRY MITCAD_RENDER_XPC_ENTRY)
  target_link_libraries(mitcad-render-service PRIVATE ${mitcad_frame_libraries}
    "-framework IOSurface" "-framework Foundation" "-framework Security" "-framework Metal")
  set(mitcad_render_service_dir "$<TARGET_FILE_DIR:mitcad>/../XPCServices/org.mitcad.renderer.xpc")
  set_target_properties(mitcad-render-service PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "${mitcad_render_service_dir}/Contents/MacOS"
    INSTALL_RPATH "@executable_path/../../../../Frameworks")
  target_link_options(mitcad-render-service PRIVATE LINKER:-headerpad_max_install_names)
  configure_file(packaging/macos/RenderService.plist.in RenderService.plist @ONLY)
  add_custom_command(TARGET mitcad-render-service POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${CMAKE_CURRENT_BINARY_DIR}/RenderService.plist"
            "${mitcad_render_service_dir}/Contents/Info.plist"
    COMMAND /usr/bin/codesign --force --sign - "${mitcad_render_service_dir}" VERBATIM)
  target_sources(mitcad PRIVATE render/MacRenderClient.mm render/MacFrameSurface.cpp render/MacFrameDisplay.cpp render/MacCommandPayload.cpp)
  target_link_libraries(mitcad PRIVATE "-framework IOSurface" "-framework OpenGL" "-framework Security")
  set_source_files_properties(render/MacRenderService.mm render/MacRenderClient.mm render/MacFrameMetal.mm
    PROPERTIES COMPILE_OPTIONS -fblocks)
  add_dependencies(mitcad mitcad-render-service)
  set_property(TARGET mitcad APPEND PROPERTY LINK_DEPENDS "$<TARGET_FILE:mitcad-render-service>")
  # Packaging replaces this development signature with its selected identity.
  add_custom_command(TARGET mitcad POST_BUILD
    COMMAND /usr/bin/codesign --force --sign - "$<TARGET_BUNDLE_DIR:mitcad>" VERBATIM)
  include("${PROJECT_SOURCE_DIR}/cmake/MacRenderServiceTests.cmake")
  include("${PROJECT_SOURCE_DIR}/cmake/MacCommandPayloadTests.cmake")
  include("${PROJECT_SOURCE_DIR}/cmake/MacFrameMetalGlTest.cmake")
  if(BUILD_TESTING)
    add_executable(test_mac_frame_metal tests/mac_frame_metal.mm render/MacFrameMetal.mm)
    target_include_directories(test_mac_frame_metal PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
    target_compile_options(test_mac_frame_metal PRIVATE -fblocks)
    target_link_libraries(test_mac_frame_metal PRIVATE "-framework IOSurface" "-framework Foundation"
      "-framework Metal" mitcad_warnings)
    add_test(NAME app.render_surface_metal_macos COMMAND test_mac_frame_metal)
    set_tests_properties(app.render_surface_metal_macos PROPERTIES TIMEOUT 20 RUN_SERIAL TRUE SKIP_RETURN_CODE 77)
  endif()
endif()
if(APPLE AND BUILD_TESTING)
  # The renderer itself is the private embedded XPC service. Its normal
  # Qt/Cycles dependencies initialize with an untouched launchd bootstrap.
  get_target_property(mitcad_frame_probe_sources mitcad-render SOURCES)
  get_target_property(mitcad_frame_probe_libraries mitcad-render LINK_LIBRARIES)
  get_target_property(mitcad_frame_probe_includes mitcad-render INCLUDE_DIRECTORIES)
  get_target_property(mitcad_frame_probe_definitions mitcad-render COMPILE_DEFINITIONS)
  add_executable(mitcad-render-xpc-probe ${mitcad_frame_probe_sources} render/MacFrameXpcProbeService.mm)
  target_include_directories(mitcad-render-xpc-probe PRIVATE ${mitcad_frame_probe_includes})
  target_include_directories(mitcad-render-xpc-probe SYSTEM PRIVATE "${MITCAD_CGLTF_INCLUDE_DIR}")
  target_compile_definitions(mitcad-render-xpc-probe PRIVATE ${mitcad_frame_probe_definitions}
    MITCAD_RENDER_RUNTIME_ENTRY)
  target_link_libraries(mitcad-render-xpc-probe PRIVATE ${mitcad_frame_probe_libraries}
    "-framework IOSurface" "-framework Metal" "-framework Foundation" "-framework Security")
  # Retain the actual worker call graph under release ThinLTO as well; this
  # prerequisite must not reduce to a tiny Qt/Metal-only executable.
  target_link_options(mitcad-render-xpc-probe PRIVATE "LINKER:-u,_mitcad_render_runtime_main")
  set(mitcad_frame_xpc_app "${CMAKE_CURRENT_BINARY_DIR}/frame-xpc-probe.app")
  set(mitcad_frame_xpc_service "${mitcad_frame_xpc_app}/Contents/XPCServices/org.mitcad.renderer.probe.xpc")
  set_target_properties(mitcad-render-xpc-probe PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "${mitcad_frame_xpc_service}/Contents/MacOS"
    INSTALL_RPATH "@executable_path/../../../../Frameworks")
  configure_file(packaging/macos/FrameXpcProbeService.plist.in FrameXpcProbeService.plist @ONLY)
  add_custom_command(TARGET mitcad-render-xpc-probe POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${CMAKE_CURRENT_BINARY_DIR}/FrameXpcProbeService.plist"
            "${mitcad_frame_xpc_service}/Contents/Info.plist"
    COMMAND /usr/bin/codesign --force --sign - "${mitcad_frame_xpc_service}" VERBATIM)
  add_executable(test_mac_frame_xpc MACOSX_BUNDLE tests/mac_frame_xpc.mm render/MacFrameSurface.cpp render/FrameMemory.cpp)
  set_source_files_properties(render/MacFrameXpcProbeService.mm tests/mac_frame_xpc.mm
    PROPERTIES COMPILE_OPTIONS -fblocks)
  target_include_directories(test_mac_frame_xpc PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_mac_frame_xpc PRIVATE Qt6::Core
    "-framework IOSurface" "-framework Foundation" "-framework Security" mitcad_warnings)
  configure_file(packaging/macos/FrameXpcProbeApp.plist.in FrameXpcProbeApp.plist @ONLY)
  set_target_properties(test_mac_frame_xpc PROPERTIES OUTPUT_NAME frame-xpc-probe
    MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_BINARY_DIR}/FrameXpcProbeApp.plist")
  add_dependencies(test_mac_frame_xpc mitcad-render-xpc-probe)
  # A changed nested executable invalidates the outer seal even if the client
  # source is unchanged. Relink then sign the complete development bundle.
  set_property(TARGET test_mac_frame_xpc APPEND PROPERTY LINK_DEPENDS "$<TARGET_FILE:mitcad-render-xpc-probe>")
  add_custom_command(TARGET test_mac_frame_xpc POST_BUILD
    COMMAND /usr/bin/codesign --force --sign - "${mitcad_frame_xpc_app}" VERBATIM)
  add_test(NAME app.render_surface_xpc_macos COMMAND test_mac_frame_xpc)
  set_tests_properties(app.render_surface_xpc_macos PROPERTIES TIMEOUT 45 RUN_SERIAL TRUE)
endif()
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
# Metal needs the complete source hierarchy at run time, not the flat
# compiled-kernel directory above. Keep the build bundle and installed
# bundle identical; the manifest also causes a relink/copy after a rebuild
# of the dependency prefix changes a runtime source file.
if(APPLE)
  set_property(TARGET mitcad-render APPEND PROPERTY LINK_DEPENDS "${MITCAD_CYCLES_RUNTIME}/source.sha256")
  add_custom_command(TARGET mitcad-render POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E rm -rf "$<TARGET_FILE_DIR:mitcad-render>/../Resources/cycles"
    COMMAND "${CMAKE_COMMAND}" -E copy_directory "${MITCAD_CYCLES_RUNTIME}"
            "$<TARGET_FILE_DIR:mitcad-render>/../Resources/cycles"
    VERBATIM)
endif()
# On Windows Open Image Denoise's DLLs next to it (vcpkg's come with
# vcpkg's applocal step).
if(MITCAD_OIDN_DLLS)
  add_custom_command(TARGET mitcad-render POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different ${MITCAD_OIDN_DLLS} "$<TARGET_FILE_DIR:mitcad-render>"
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
  # Open Image Denoise's own view of the GPUs (mitcad#202): its devices,
  # then a line per GPU (none without one).
  add_test(NAME app.render_worker_probe_denoiser COMMAND mitcad-render --probe-denoiser)
  set_tests_properties(app.render_worker_probe_denoiser PROPERTIES
    PASS_REGULAR_EXPRESSION "render-denoiser-probe Open Image Denoise [0-9.]+ finds: [^\r\n]*CPU")
  # The best GPU against the CPU: the same test scene, images close to each
  # other (8-bit sRGB: mean difference, 99th percentile). Skipped without a
  # GPU device (a machine without one, or a build without GPU devices).
  add_test(NAME app.render_worker_gpu
           COMMAND mitcad-render --bench 320x240 --samples 64 --device auto --compare cpu --tolerance 2:24
                   --output "${CMAKE_CURRENT_BINARY_DIR}/render-worker-gpu-test.png")
  set_tests_properties(app.render_worker_gpu PROPERTIES SKIP_RETURN_CODE 77
    PASS_REGULAR_EXPRESSION "render-compare (CUDA|OPTIX|HIP|METAL|ONEAPI) ")
  add_test(NAME app.render_required_metal_contract
    COMMAND "${CMAKE_COMMAND}" "-DWORKER=$<TARGET_FILE:mitcad-render>"
            -P "${CMAKE_CURRENT_SOURCE_DIR}/tests/render-required-metal-test.cmake")
  set_tests_properties(app.render_required_metal_contract PROPERTIES TIMEOUT 60)
  # Check completed denoising independently of render-device capabilities.
  if(APPLE)
    foreach(mode cpu failure cpu_previews failure_previews metal previews)
      add_test(NAME app.render_denoiser_${mode}
        COMMAND "${CMAKE_COMMAND}" "-DWORKER=$<TARGET_FILE:mitcad-render>" "-DMODE=${mode}"
                -P "${CMAKE_CURRENT_SOURCE_DIR}/tests/render-denoiser-test.cmake")
      set_tests_properties(app.render_denoiser_${mode} PROPERTIES
        TIMEOUT 220 SKIP_REGULAR_EXPRESSION "SKIP: Metal OIDN unavailable")
    endforeach()
  endif()
  # Each takes seconds; a hung GPU (mitcad#146) fails them in two minutes,
  # not at ctest's default of 25.
  set_tests_properties(app.render_worker app.render_updates app.render_worker_environment
                       app.render_worker_fallback app.render_worker_device_failure app.render_worker_gpu
                       app.render_worker_probe_denoiser
    PROPERTIES TIMEOUT 120)
  # On Windows the worker finds Qt's DLLs through PATH, as app.unit does.
  if(WIN32)
    set_tests_properties(app.render_worker app.render_updates app.render_worker_environment
                         app.render_worker_fallback app.render_worker_device_failure app.render_worker_gpu
                         app.render_worker_probe_denoiser app.render_required_metal_contract
      PROPERTIES ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  # The maps of a texture set (mitcad#314): generated normal, roughness,
  # metalness and occlusion maps on the test block change its image, or
  # leave it as it is where they equal what they replace.
  add_test(NAME app.render_texture_maps
           COMMAND mitcad-render --maps-test "${CMAKE_CURRENT_BINARY_DIR}/render-texture-maps")
  set_tests_properties(app.render_texture_maps PROPERTIES TIMEOUT 120 PASS_REGULAR_EXPRESSION "render-maps ok")
  if(WIN32)
    set_tests_properties(app.render_texture_maps PROPERTIES
      ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
  # Material swaps in a running session (mitcad#538): a body's material
  # changed to and from textured ones, back and forth, renders in it (not
  # black), as a renderer that starts with it; an image that cannot be read
  # any more when its material comes back is warned of.
  add_test(NAME app.render_material_swap
           COMMAND mitcad-render --swap-test "${CMAKE_CURRENT_BINARY_DIR}/render-material-swap")
  set_tests_properties(app.render_material_swap PROPERTIES TIMEOUT 180 PASS_REGULAR_EXPRESSION "render-swap ok")
  if(APPLE)
    add_test(NAME app.render_material_swap_metal
      COMMAND mitcad-render --swap-test "${CMAKE_CURRENT_BINARY_DIR}/render-material-swap-metal" --device metal)
    set_tests_properties(app.render_material_swap_metal PROPERTIES
      TIMEOUT 180 SKIP_RETURN_CODE 77 PASS_REGULAR_EXPRESSION "render-swap ok")
  endif()
  if(WIN32)
    set_tests_properties(app.render_material_swap PROPERTIES
      ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endif()
endif()

if(APPLE)
  # Configure the bundle after its render worker exists, so its
  # install rules include the worker, denoiser module and their libraries.
  include(packaging/macos/Bundle.cmake)
endif()

# The browser's and the timeline's places logged again when they move (mitcad#262).
target_sources(mitcad PRIVATE framework/PlaceWatcher.hpp)
# The side panels' trees' common indentation (mitcad#393).
target_sources(mitcad PRIVATE framework/PanelTree.hpp)
# The hover of a tangent chain in Fillet, Chamfer and the face tools (mitcad#290).
target_sources(mitcad PRIVATE framework/CommandSessionHover.cpp)
# A new feature's operation and objects follow what the model chooses from
# the preview until the user picks them: Extrude's automatic operation
# (mitcad#289).
target_sources(mitcad PRIVATE framework/CommandSessionAuto.cpp)
# Texture sources in Edit Appearances (mitcad#315, app/materials): the
# TextureSource interface and its registry, local texture folders, the
# browser's grid, Use's command, Preferences' Textures group.
target_sources(mitcad PRIVATE
  materials/FolderTextureSource.cpp
  materials/FolderTextureSource.hpp
  materials/TextureBrowser.cpp
  materials/TextureBrowser.hpp
  materials/TexturePreferences.cpp
  materials/TexturePreferences.hpp
  materials/TextureSource.cpp
  materials/TextureSource.hpp
  materials/TextureUse.cpp
  materials/TextureUse.hpp)
if(BUILD_TESTING)
  # The folder source's map detection, scan, thumbnails and fetch, the
  # registry, the cache's folders and Use's command: plain Qt Gui.
  target_sources(test_app PRIVATE tests/texture_sources.cpp
                 materials/FolderTextureSource.cpp materials/FolderTextureSource.hpp
                 materials/TextureSource.cpp materials/TextureSource.hpp
                 materials/TextureUse.cpp materials/TextureUse.hpp)
endif()
# Poly Haven as an online texture source (mitcad#313, app/materials): the
# requests of the texture sources on the internet (Qt Network; files of a
# folder for UI tests) and the source. The unit tests give it a fake
# fetcher and never use the network.
target_sources(mitcad PRIVATE
  materials/PolyHavenTextureSource.cpp
  materials/PolyHavenTextureSource.hpp
  materials/TextureFetcher.cpp
  materials/TextureFetcher.hpp)
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/polyhaven_source.cpp
                 materials/PolyHavenTextureSource.cpp materials/PolyHavenTextureSource.hpp
                 materials/TextureFetcher.cpp materials/TextureFetcher.hpp)
  target_link_libraries(test_app PRIVATE Qt6::Network)
endif()
# The BUILD tab: frames of slot profiles with their connectors, panels and
# linear parts, the cut list and the parts list (mitcad#375, app/COMMANDS.md
# "BUILD"); QPdfWriter of Qt Gui writes the lists' PDFs.
target_sources(mitcad PRIVATE
  build/BuildCatalog.cpp
  build/BuildCatalog.hpp
  build/BuildLists.cpp
  build/BuildLists.hpp
  build/BuildModel.cpp
  build/BuildModel.hpp
  commands/BuildCommands.cpp)
# The part picker's data (mitcad#418, app/library): the user's favourite
# parts and groups in the settings, the picker's order, the library parts
# the design uses (the model's `library_usage` query) and the groups' tree
# (mitcad#527).
target_sources(mitcad PRIVATE
  library/PartFavourites.cpp
  library/PartFavourites.hpp
  library/PartGroups.cpp
  library/PartGroups.hpp
  library/PartSuppliers.cpp
  library/PartSuppliers.hpp
  library/UsedParts.cpp
  library/UsedParts.hpp)
if(BUILD_TESTING)
  # The order with stars, the stars' settings, the used parts read from a
  # model's answer, the groups' tree and the Supplier filter (mitcad#949):
  # plain Qt Core and the command context's interface.
  target_sources(test_app PRIVATE tests/part_picker_data.cpp
                 library/PartFavourites.cpp library/PartFavourites.hpp
                 library/PartGroups.cpp library/PartGroups.hpp
                 library/PartSuppliers.cpp library/PartSuppliers.hpp
                 library/UsedParts.cpp library/UsedParts.hpp)
endif()
# The part picker (mitcad#418, app/library): the library's parts as a grid
# of pictures with favourites and the design's parts, a floating window for
# the part-choosing commands and Insert from Library's results.
target_sources(mitcad PRIVATE
  library/PartPicker.cpp
  library/PartPicker.hpp)
# Options of configurable components in the pickers and Edit Options…
# (mitcad#810): the `configure` answers read and the JSON sent (plain Qt
# Core), and the choosers.
target_sources(mitcad PRIVATE
  library/EditOptionsDialog.cpp
  library/EditOptionsDialog.hpp
  library/OptionChooser.cpp
  library/OptionChooser.hpp
  library/PartOptions.cpp
  library/PartOptions.hpp)
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/part_options.cpp library/PartOptions.cpp library/PartOptions.hpp)
endif()
# The suppliers' articles of a library part's row as a table behind a
# button (mitcad#979): Insert from Library, Library Parts and Edit Options….
target_sources(mitcad PRIVATE
  library/SupplierPartsDialog.cpp
  library/SupplierPartsDialog.hpp)
# The web viewer's pages (mitcad#423, app/web, app/COMMANDS.md "Web
# page"): a design as one self-contained HTML file, File > Export > Web
# page. The viewer's files (app/web/viewer, its libraries in
# third_party/three) are resources under :/web-viewer/<repository path>,
# found by a glob that may find none (pageHtml then names the file the
# build lacks).
target_sources(mitcad PRIVATE
  web/WebGlb.cpp
  web/WebGlb.hpp
  web/WebPage.cpp
  web/WebPage.hpp
  web/WebPageHtml.cpp)
file(GLOB_RECURSE MITCAD_WEB_VIEWER_FILES CONFIGURE_DEPENDS
  "${PROJECT_SOURCE_DIR}/app/web/viewer/*" "${PROJECT_SOURCE_DIR}/third_party/three/*")
if(MITCAD_WEB_VIEWER_FILES)
  foreach(web_viewer_file IN LISTS MITCAD_WEB_VIEWER_FILES)
    file(RELATIVE_PATH web_viewer_alias "${PROJECT_SOURCE_DIR}" "${web_viewer_file}")
    set_source_files_properties("${web_viewer_file}" PROPERTIES QT_RESOURCE_ALIAS "${web_viewer_alias}")
  endforeach()
  qt_add_resources(mitcad web_viewer PREFIX /web-viewer FILES ${MITCAD_WEB_VIEWER_FILES})
endif()
if(BUILD_TESTING)
  # The page's HTML from fixture files of the viewer, and the glb's nodes
  # with the tree: Qt Core and Gui and the renderer's plain types.
  target_sources(test_app PRIVATE tests/web_page.cpp
                 web/WebGlb.cpp web/WebGlb.hpp web/WebPage.hpp web/WebPageHtml.cpp)
endif()
# A project's web viewer (mitcad#423, app/web): File > Project > Web Viewer
# and Update Web Viewer, the Save that updates a design's page, the web/
# folder's index and designs.json (the pages: web/WebPage.hpp).
target_sources(mitcad PRIVATE
  web/MainWindowWeb.cpp
  web/ProjectWeb.cpp
  web/ProjectWeb.hpp)
if(BUILD_TESTING)
  # The designs' slugs, designs.json and the index: plain Qt Core.
  target_sources(test_app PRIVATE tests/project_web.cpp web/ProjectWeb.cpp web/ProjectWeb.hpp)
endif()
# The orientation cube and its Home button in glass of their own
# (mitcad#430).
target_sources(mitcad PRIVATE
  view/ViewCube.cpp
  view/ViewCube.hpp)
# The card at the top of the view while Create Sketch waits for a plane
# (mitcad#474).
target_sources(mitcad PRIVATE
  view/ModeBanner.cpp
  view/ModeBanner.hpp)
# File > Export Drawing (mitcad#471, app/COMMANDS.md "Drawing"): its dialog
# and the choices it keeps (settings group drawing); the model writes the
# file (export_drawing).
target_sources(mitcad PRIVATE
  files/DrawingDialog.cpp
  files/DrawingDialog.hpp
  files/DrawingSettings.cpp
  files/DrawingSettings.hpp
  files/MainWindowDrawing.cpp)
if(BUILD_TESTING)
  # The settings and the command they make: plain Qt Core.
  target_sources(test_app PRIVATE tests/drawing_settings.cpp files/DrawingSettings.cpp files/DrawingSettings.hpp)
endif()
# Drawings kept in the design (mitcad#707, app/COMMANDS.md "Drawings"): a
# drawing opened over the 3D view, its sheet painted from the model's
# drawing query, its tools as declarative definitions, the main window's
# Create Drawing and open and close.
target_sources(mitcad PRIVATE
  drawings/DrawingSheetView.cpp
  drawings/DrawingSheetView.hpp
  drawings/DrawingTemplates.cpp
  drawings/DrawingTemplates.hpp
  drawings/DrawingTools.cpp
  drawings/DrawingTools.hpp
  drawings/DrawingWorkspace.cpp
  drawings/DrawingWorkspace.hpp
  drawings/MainWindowDrawings.cpp)
if(BUILD_TESTING)
  # The tools' definitions and the commands they make: plain Qt Core.
  target_sources(test_app PRIVATE tests/drawing_tools.cpp drawings/DrawingTools.cpp drawings/DrawingTools.hpp
                                  drawings/DrawingTemplates.cpp drawings/DrawingTemplates.hpp)
  # The sheet's zoom kept through the model's answers (mitcad#751) and its
  # picture kept while the pointer moves (mitcad#759): plain Qt Widgets on
  # the offscreen platform.
  target_sources(test_app PRIVATE tests/drawing_sheet_view.cpp drawings/DrawingSheetView.cpp
                                  drawings/DrawingSheetView.hpp)
endif()
if(BUILD_TESTING)
  # The view's context menu (mitcad#494): which commands a selection
  # completes (first) and which need further picks (More), from the command
  # definitions.
  target_sources(test_app PRIVATE tests/context_menu.cpp framework/Command.cpp framework/Command.hpp)
endif()
# Sketch text from font files: Preferences' Fonts page and the font
# chooser of Edit Text (mitcad#473).
target_sources(mitcad PRIVATE
  sketch/FontFiles.cpp
  sketch/FontFiles.hpp)
# The sketch's origin and its x and y axes as picks for constraints and
# dimensions (mitcad#500).
target_sources(mitcad PRIVATE sketch/SketchAxes.cpp)
if(BUILD_TESTING)
  # Their picks on the sketch model's data: plain Qt Core and the sketch
  # geometry.
  target_sources(test_app PRIVATE tests/sketch_axes.cpp sketch/SketchAxes.cpp)
endif()
# Mount frames of any part and Place Part (mitcad#525, app/COMMANDS.md
# "Component libraries"): a library part placed by its frame on a face, a
# hole or a grid; the frames a manifest lists.
target_sources(mitcad PRIVATE
  commands/MountCommands.cpp
  commands/MountCommands.hpp
  library/MountFrames.cpp
  library/MountFrames.hpp)
if(BUILD_TESTING)
  # The frames of a manifest's block and their texts: plain Qt Core.
  target_sources(test_app PRIVATE tests/mount_frames.cpp library/MountFrames.cpp library/MountFrames.hpp)
endif()
# A timber frame's openings (mitcad#519, app/COMMANDS.md "Timber"): their
# rows, text form and checks, and the Openings window that edits them
# beside Wall's, Floor / Roof's and Timber Frame's panels.
target_sources(mitcad PRIVATE
  build/OpeningsWindow.cpp
  build/OpeningsWindow.hpp
  build/TimberOpenings.cpp
  build/TimberOpenings.hpp)
if(BUILD_TESTING)
  # Text to rows and back, the model's openings, the rows' checks: plain Qt
  # Core.
  target_sources(test_app PRIVATE tests/timber_openings.cpp build/TimberOpenings.cpp build/TimberOpenings.hpp)
endif()
# The joint highlight (mitcad#523): a selected joint's sides, frames and
# occurrences in the view, from the `joints` query.
target_sources(mitcad PRIVATE
  MainWindowJointHighlight.cpp
  framework/JointHighlight.cpp
  framework/JointHighlight.hpp)
if(BUILD_TESTING)
  # What each side shows, its log lines and the card's text: plain Qt Core.
  target_sources(test_app PRIVATE tests/joint_highlight.cpp framework/JointHighlight.cpp
                 framework/JointHighlight.hpp)
endif()
# Move Face (mitcad#531): its summary line ("MoveFace1: 2 faces of Body1,
# translate (...) mm"), for the panel, the log and the selection card.
target_sources(mitcad PRIVATE
  commands/MoveFaceText.cpp
  commands/MoveFaceText.hpp
  MainWindowMoveFace.cpp)
# Feature inputs where they were (mitcad#957): a selected or hovered
# Fillet's or Chamfer's picked edges as lines, from `feature_inputs`.
target_sources(mitcad PRIVATE
  MainWindowFeatureInputs.cpp)
if(BUILD_TESTING)
  # Its motions in words: plain Qt Core.
  target_sources(test_app PRIVATE tests/move_face_text.cpp commands/MoveFaceText.cpp commands/MoveFaceText.hpp)
endif()
# The Projects panel's rows (mitcad#529): compact marks for their state, a
# name elided in the middle.
target_sources(mitcad PRIVATE
  projects/ProjectRowText.cpp
  projects/ProjectRowText.hpp)
if(BUILD_TESTING)
  # The marks and the elision: plain Qt Core and Gui.
  target_sources(test_app PRIVATE tests/project_rows.cpp projects/ProjectRowText.cpp projects/ProjectRowText.hpp)
endif()
# File > Open from Project (mitcad#530): the projects browsed as folders,
# their designs searched by name.
target_sources(mitcad PRIVATE
  projects/ProjectBrowser.cpp
  projects/ProjectBrowser.hpp
  projects/ProjectBrowserData.cpp
  projects/ProjectBrowserData.hpp)
if(BUILD_TESTING)
  # A place's contents and the search over a fixture project tree: plain Qt
  # Core (the marks' words from ProjectRowText).
  target_sources(test_app PRIVATE tests/project_browser.cpp projects/ProjectBrowserData.cpp
                 projects/ProjectBrowserData.hpp)
endif()
# The view's fit of a design's first bodies, once per design (mitcad#467;
# not again after every body was hidden and one shown).
target_sources(mitcad PRIVATE view/FirstBodiesFit.hpp)
if(BUILD_TESTING)
  # The decision: plain C++.
  target_sources(test_app PRIVATE tests/first_bodies_fit.cpp view/FirstBodiesFit.hpp)
endif()
# Section Analysis off the UI thread (mitcad#438): the section worker's
# rounds, and the window's clips, view and measurement through them.
target_sources(mitcad PRIVATE
  MainWindowSection.cpp
  framework/SectionWorker.cpp
  framework/SectionWorker.hpp)
if(BUILD_TESTING)
  # Rounds superseded, waiting, stopped, off the UI thread: plain Qt Core
  # and the geometry's cancel.
  target_sources(test_app PRIVATE tests/section_worker.cpp framework/SectionWorker.cpp
                 framework/SectionWorker.hpp)
endif()
# Section Analysis and Hide Above Sketch over a feature preview
# (mitcad#943): a preview's bodies clipped by the view's clip, the caps of
# a cut body.
target_sources(mitcad PRIVATE
  framework/SectionPreview.cpp
  framework/SectionPreview.hpp)
if(BUILD_TESTING)
  # A preview's clips and caps through the model's commands, as the window
  # asks for them, without a window.
  add_executable(test_section_preview tests/section_preview.cpp framework/SectionPreview.cpp)
  target_include_directories(test_section_preview PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_section_preview PRIVATE Qt6::Core mitcad_ffi mitcad_geometry TKBO TKTopAlgo TKPrim
    TKBRep mitcad_warnings)
  add_test(NAME app.section_preview COMMAND test_section_preview)
  set_tests_properties(app.section_preview PROPERTIES TIMEOUT 120
    ENVIRONMENT "MITCAD_RESULT_STORE=off"
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()

# Native scheduling, pool initialization and exception-safe QoS restoration
# (mitcad#556). No windows or renderer performance campaign.
if(APPLE AND BUILD_TESTING)
  add_executable(test_mac_work tests/mac_work.cpp
    framework/ModelWorker.cpp framework/ModelWorker.hpp
    framework/SectionWorker.cpp framework/SectionWorker.hpp)
  set_target_properties(test_mac_work PROPERTIES AUTOMOC ON)
  target_link_libraries(test_mac_work PRIVATE mitcad_mac_work mitcad_ffi mitcad_geometry mitcad_warnings)
  add_test(NAME app.macos_work COMMAND test_mac_work)
  set_tests_properties(app.macos_work PROPERTIES TIMEOUT 60)
endif()
# Fillet's and Chamfer's handles (mitcad#534): where the arrows of a
# rounding's radius and of a bevel's distances stand on an edge.
target_sources(mitcad PRIVATE
  commands/ChamferHandles.cpp
  commands/ChamferHandles.hpp)
if(BUILD_TESTING)
  # Convex, concave, obtuse and placed edges, the measured face: plain OCCT
  # math; an L bracket's edges read from its solid (mitcad#571).
  target_sources(test_app PRIVATE tests/chamfer_handles.cpp commands/ChamferHandles.cpp
                 commands/ChamferHandles.hpp)
endif()
# The patterns' Direction Type and Distance Type (mitcad#545): the panels'
# choices as the definitions' `symmetric` and `distance_type`, the length
# row's label, the options in the log.
target_sources(mitcad PRIVATE
  commands/PatternOptions.cpp
  commands/PatternOptions.hpp)
if(BUILD_TESTING)
  # The mapping and the words: plain Qt Core.
  target_sources(test_app PRIVATE tests/pattern_options.cpp commands/PatternOptions.cpp
                 commands/PatternOptions.hpp)
endif()

# The sketches a command shows for its time (mitcad#546): an edited
# feature's, as if their light bulbs were on, until its panel closes.
target_sources(mitcad PRIVATE view/CommandSketches.hpp)
if(BUILD_TESTING)
  # Which sketches, which only for the command, what each goes back to:
  # plain Qt Core.
  target_sources(test_app PRIVATE tests/command_sketches.cpp view/CommandSketches.hpp)
endif()

# Change Parameters while a sketch or a feature is being created or edited
# (mitcad#564): SKETCH > MODIFY's entry, the parameter table's commands
# through an open panel's session, the panel's title row button.
target_sources(mitcad PRIVATE
  MainWindowParameterEdits.cpp
  framework/CommandPanelHeader.cpp
  framework/CommandSessionParameters.cpp
  framework/ParameterEdits.cpp
  framework/ParameterEdits.hpp)
if(BUILD_TESTING)
  # What the table leaves to an open panel, which of its changes the panel
  # follows, a rename in the panel's expressions: plain Qt Core.
  target_sources(test_app PRIVATE tests/parameter_edits.cpp framework/ParameterEdits.cpp
                 framework/ParameterEdits.hpp)
endif()

# Expression fields' parameter suggestions while typing, the fx browser,
# name = value and Create User Parameter (mitcad#567, mitcad#568): feature
# panels, sketch dimensions and tool values, the parameter table.
target_sources(mitcad PRIVATE
  framework/ParameterSuggest.cpp
  framework/ParameterSuggest.hpp
  framework/ParameterField.cpp
  framework/ParameterField.hpp)
if(BUILD_TESTING)
  # The name at the caret, matching, insertion, refused choices, new names;
  # the list's keys on a plain field: Qt Widgets on the offscreen platform.
  target_sources(test_app PRIVATE tests/parameter_suggest.cpp
                 framework/ParameterSuggest.cpp framework/ParameterSuggest.hpp
                 framework/ParameterField.cpp framework/ParameterField.hpp)
endif()

# Immutable edit-operation policy and manual override of delayed replies
# (mitcad#575): plain Qt Core, shared with the command session.
target_sources(mitcad PRIVATE framework/AutoOperationPolicy.hpp)
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/auto_operation_policy.cpp framework/AutoOperationPolicy.hpp)
endif()

# Auto-update of the library sources (mitcad#576, app/COMMANDS.md "Component
# libraries"): the sources' settings (plain Qt Core) and the background fetch
# at start-up.
target_sources(mitcad PRIVATE
  files/LibraryAutoUpdate.cpp
  files/LibraryAutoUpdate.hpp
  files/LibrarySettings.cpp)
if(BUILD_TESTING)
  # The settings' round trip, with and without the key: plain Qt Core.
  target_sources(test_app PRIVATE tests/library_settings.cpp files/LibrarySettings.cpp files/Libraries.hpp)
endif()

# The selection card's Create row (mitcad#619, app/COMMANDS.md "What the
# framework does"): up to four CREATE commands that fit the selection,
# Extrude first, under the selection's text.
target_sources(mitcad PRIVATE
  framework/SelectionCreate.cpp
  framework/SelectionCreate.hpp
  MainWindowSelectionCreate.cpp)
if(BUILD_TESTING)
  # Kinds of selection to the buttons, the commands' first inputs, the
  # row's buttons changed in place: Qt Widgets on the offscreen platform.
  target_sources(test_app PRIVATE tests/selection_create.cpp framework/SelectionCreate.cpp
                 framework/SelectionCreate.hpp framework/Command.cpp framework/Command.hpp)
endif()

# Updates (mitcad#603): the private download folder on every platform, the
# Windows updater's command line and its SHA-256 check of the installer:
# plain Qt Core and plain C++.
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/update_installer.cpp update/UpdateInstaller.cpp update/UpdateInstaller.hpp
                 update/UpdaterChecks.hpp)
  if(WIN32)
    # processHasAdministratorRights (mitcad#921).
    target_link_libraries(test_app PRIVATE advapi32)
  endif()
endif()

# The updates' test hooks (mitcad#921, docs/updates.md#the-test-hooks): the
# update source read as a release (MITCAD_UPDATE_TEST_HOOKS 0) and as a
# test build (1), whatever this build's option, with the tests' variables
# set; a manifest signed with their key by mitcad-release is refused by the
# release and accepted by the test build.
if(BUILD_TESTING AND TARGET mitcad-release)
  foreach(mitcad_update_build IN ITEMS release hooks)
    add_executable(test_update_source_${mitcad_update_build} tests/update_source.cpp update/UpdateSettings.cpp
      update/UpdateSettings.hpp)
    target_include_directories(test_update_source_${mitcad_update_build} PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
    target_compile_definitions(test_update_source_${mitcad_update_build} PRIVATE
      MITCAD_VERSION="${PROJECT_VERSION}" MITCAD_UPDATE_KEY="${mitcad_update_keys}")
    target_link_libraries(test_update_source_${mitcad_update_build} PRIVATE Qt6::Core mitcad_ffi mitcad_warnings)
    add_dependencies(test_update_source_${mitcad_update_build} mitcad-release)
    add_test(NAME app.update_source_${mitcad_update_build}
      COMMAND test_update_source_${mitcad_update_build} "$<TARGET_FILE:mitcad-release>")
    set_tests_properties(app.update_source_${mitcad_update_build} PROPERTIES TIMEOUT 120
      ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  endforeach()
  target_compile_definitions(test_update_source_release PRIVATE MITCAD_UPDATE_TEST_HOOKS=0)
  target_compile_definitions(test_update_source_hooks PRIVATE MITCAD_UPDATE_TEST_HOOKS=1)
endif()

# Print Analysis (mitcad#620, app/COMMANDS.md "Print Analysis"): the panel's
# settings (group print_analysis), the definition, the queries and their
# answers in words; the shown analysis' overlay in the window and the view.
target_sources(mitcad PRIVATE
  MainWindowPrintAnalysis.cpp
  commands/PrintAnalysis.cpp
  commands/PrintAnalysis.hpp
  view/PrintOverlay.cpp
  view/PrintOverlay.hpp)
if(BUILD_TESTING)
  # Settings, definition, queries, log line, words and overlay data: plain
  # Qt Core and Gui.
  target_sources(test_app PRIVATE tests/print_analysis.cpp commands/PrintAnalysis.cpp
                 commands/PrintAnalysis.hpp)
endif()

target_sources(mitcad PRIVATE framework/TestDriverManipulators.cpp)
target_sources(mitcad PRIVATE framework/TestDriverChrome.cpp)
target_sources(mitcad PRIVATE framework/MacToolButtonStyle.cpp framework/MacToolButtonStyle.hpp
               framework/TestDriverMacControls.cpp)
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/mac_tool_buttons.cpp framework/MacToolButtonStyle.cpp
                 framework/MacToolButtonStyle.hpp)
endif()
target_sources(mitcad PRIVATE framework/TestDriverDialogs.cpp platform/MacDialogs.hpp)


# Measure's history (mitcad#648, app/COMMANDS.md "Measure's history"): the
# rows kept while the design is open and their list in the Measure panel.
target_sources(mitcad PRIVATE
  MainWindowMeasureHistory.cpp
  commands/MeasureHistory.cpp
  commands/MeasureHistory.hpp
  commands/MeasureHistoryView.cpp
  commands/MeasureHistoryView.hpp)
if(BUILD_TESTING)
  # Rows added, extended and updated, gone, cleared, copied: plain Qt Core.
  target_sources(test_app PRIVATE tests/measure_history.cpp commands/MeasureHistory.cpp
                 commands/MeasureHistory.hpp)
endif()

# Print settings and materials (mitcad#640, mitcad#654, mitcad#655;
# app/COMMANDS.md "Materials and print settings"): the key table's
# categories and values, the shared settings form and its fields, the
# Materials window, the profile editor, the import's summary and Details…,
# the command panel's note row (3D Print's update bar and import line) and
# Show Build Volume in the view.
target_sources(mitcad PRIVATE
  framework/NoteRow.cpp
  framework/NoteRow.hpp
  print/PrintKeys.cpp
  print/PrintKeys.hpp
  print/PrintLibrary.cpp
  print/PrintLibrary.hpp
  print/SettingField.cpp
  print/SettingField.hpp
  print/SettingsForm.cpp
  print/SettingsForm.hpp
  print/PrintImportSummary.cpp
  print/PrintImportSummary.hpp
  print/ImportDetailsDialog.cpp
  print/ImportDetailsDialog.hpp
  print/MaterialsDialog.cpp
  print/MaterialsDialog.hpp
  print/ProfilesDialog.cpp
  print/ProfilesDialog.hpp
  print/OwnSettingsDialog.cpp
  print/OwnSettingsDialog.hpp
  view/BuildVolume.cpp)
# My Library (mitcad#704): the panels of Materials and Printers and
# Processes on the user's or the project's library, through a model of
# its own.
target_sources(mitcad PRIVATE
  print/PrintItemsHost.cpp
  print/PrintItemsHost.hpp
  print/MyLibraryDialog.cpp
  print/MyLibraryDialog.hpp)
# Import from Slicer (mitcad#829): the one import of 3D Print, Printers
# and Processes and My Library, its file dialog, line and Details…; the
# presets of an installed slicer to tick (mitcad#830).
target_sources(mitcad PRIVATE
  print/SlicerImport.cpp
  print/SlicerImport.hpp
  print/SlicerPresetsDialog.cpp
  print/SlicerPresetsDialog.hpp)
if(BUILD_TESTING)
  # The import's line and report (plain Qt Core); the key table's layout
  # and values, and the settings form's rows (Qt Widgets).
  target_sources(test_app PRIVATE tests/print_import_summary.cpp tests/print_settings_form.cpp
                 print/PrintImportSummary.cpp print/PrintImportSummary.hpp print/PrintKeys.cpp
                 print/PrintKeys.hpp print/SettingField.cpp print/SettingField.hpp print/SettingsForm.cpp
                 print/SettingsForm.hpp)
endif()

# Purge volumes on filament changes (mitcad#705): 3D Print's table of the
# filament slots.
target_sources(mitcad PRIVATE
  print/PurgeVolumesDialog.cpp
  print/PurgeVolumesDialog.hpp)

# Placement by picked geometry (mitcad#656, app/COMMANDS.md "Placement by
# picked geometry"): the placement input every part placed by a pick
# shares, its plane geometry, and Gear Pair and Belt Drive.
target_sources(mitcad PRIVATE
  commands/DriveCommands.cpp
  commands/PlaceGeometry.cpp
  commands/PlaceGeometry.hpp
  commands/PlaceInputs.cpp
  commands/PlaceInputs.hpp)
if(BUILD_TESTING)
  # Snapping, the two nearest edges and the point at two distances: OCCT's
  # geometry types.
  target_sources(test_app PRIVATE tests/place_geometry.cpp commands/PlaceGeometry.cpp commands/PlaceGeometry.hpp)
endif()

# Measurement objects (mitcad#649, app/COMMANDS.md "Measurement objects"):
# Keep in the Measure panel, the browser's Measurements, the dimensions
# over the view, View > Export Snapshot and the measurements over Render
# Image's image.
target_sources(mitcad PRIVATE
  MainWindowMeasurements.cpp
  view/MeasurementDrawing.cpp
  view/MeasurementDrawing.hpp
  view/MeasurementOverlay.cpp
  view/MeasurementOverlay.hpp
  view/Snapshot.cpp
  view/SnapshotDialog.cpp
  view/SnapshotDialog.hpp
  view/ViewSnapshot.cpp
  # Annotations (mitcad#702): their window, editor and dialogs, and the
  # view's clicks and drags on them.
  MainWindowAnnotations.cpp
  view/AnnotationDialogs.cpp
  view/AnnotationDialogs.hpp
  view/AnnotationsWindow.cpp
  view/AnnotationsWindow.hpp)
if(BUILD_TESTING)
  # The query's entries to dimensions, their layout through a projection and
  # at a picture's scale, painting, the values in words, a render camera's
  # projection: plain Qt Gui and OCCT's points.
  target_sources(test_app PRIVATE tests/measurement_drawing.cpp view/MeasurementDrawing.cpp
                 view/MeasurementDrawing.hpp)
endif()
# The annotations' technical lettering (mitcad#902, an ISO 3098 type B style
# font in third_party/fonts/iso-lettering, LGPL-3.0 with the font exception):
# a file of its own beside the executables, loaded at run time
# (measurementFontFamily), never linked in. A macOS bundle takes it into
# Resources/fonts (packaging/macos/Bundle.cmake). Without the file the
# annotations take the UI's font.
set(MITCAD_LETTERING "${PROJECT_SOURCE_DIR}/third_party/fonts/iso-lettering/lettering.ttf")
if(EXISTS "${MITCAD_LETTERING}")
  if(NOT APPLE)
    add_custom_command(TARGET mitcad POST_BUILD
      COMMAND "${CMAKE_COMMAND}" -E make_directory "$<TARGET_FILE_DIR:mitcad>/fonts"
      COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${MITCAD_LETTERING}" "$<TARGET_FILE_DIR:mitcad>/fonts/lettering.ttf"
      VERBATIM)
  endif()
  if(BUILD_TESTING)
    add_custom_command(TARGET test_app POST_BUILD
      COMMAND "${CMAKE_COMMAND}" -E make_directory "$<TARGET_FILE_DIR:test_app>/fonts"
      COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${MITCAD_LETTERING}" "$<TARGET_FILE_DIR:test_app>/fonts/lettering.ttf"
      VERBATIM)
  endif()
else()
  message(STATUS "No annotation lettering (${MITCAD_LETTERING}): the annotations take the UI's font")
endif()

# Opaque, correctly placed rename editors on transparent trees (mitcad#685).
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/tree_item_delegate.cpp framework/TreeItemDelegate.hpp)
endif()

# Navigation settings and actual camera input (mitcad#688).
target_sources(mitcad PRIVATE framework/TestDriverNavigation.cpp)
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/navigation_preferences.cpp)
endif()

# Text-based formula assistance and the shared larger editor (mitcad#668).
target_sources(mitcad PRIVATE framework/FormulaEditor.cpp framework/FormulaEditor.hpp
                 framework/TestDriverFormula.cpp)
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/formula_editor.cpp framework/FormulaEditor.cpp
                 framework/FormulaEditor.hpp)
endif()

# Isolated assistant setup plans, quoting and atomic configuration writes.
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/assistant_setup.cpp mcp/AssistantSetup.cpp)
  add_executable(test_assistant_client tests/assistant_client.cpp)
  target_link_libraries(test_assistant_client PRIVATE Qt6::Core mitcad_warnings)
  mitcad_utf8_code_page(test_assistant_client)
  add_dependencies(mitcad test_assistant_client)
endif()

# Copy, Cut and Paste of sketch geometry (mitcad#788): the sketch fragment
# on the clipboard and the Paste tool.
target_sources(mitcad PRIVATE sketch/SketchClipboard.cpp)

# Redefine Sketch Plane (mitcad#787): a sketch moved to another plane or
# face and placed there by an offset, a move, a rotation and a flip.
target_sources(mitcad PRIVATE commands/SketchPlaneCommands.cpp)

# Image references (mitcad#782, app/COMMANDS.md "Image references"): the
# Image Reference panel, the images drawn in the view and their
# calibration by two points clicked on them.
target_sources(mitcad PRIVATE
  MainWindowImageReferences.cpp
  view/ImageCalibration.cpp
  view/ImageCalibration.hpp
  view/ImageReferenceView.cpp
  view/ImageReferences.cpp
  view/ImageReferences.hpp)
if(BUILD_TESTING)
  # The query's entries, points on an image and on its plane, the log
  # line and the calibration's distance: plain Qt Core.
  target_sources(test_app PRIVATE tests/image_references.cpp view/ImageReferences.cpp view/ImageReferences.hpp)
endif()

# HEIF images of image references (mitcad#850, MITCAD_HEIF in the top
# CMakeLists.txt): view/ImageFile.* decodes an image reference's file, HEIF
# through libheif with its HEVC decoder libde265 from vcpkg (the manifest's
# feature `heif`, without encoders), the other formats through Qt.
# libheif and libde265 are LGPL: shared libraries only, never linked
# statically (docs/development.md, "Licence policy").
target_sources(mitcad PRIVATE view/ImageFile.cpp view/ImageFile.hpp)
if(BUILD_TESTING)
  # The formats told apart, HEIF images decoded upright: tests/heif_image.cpp.
  target_sources(test_app PRIVATE tests/heif_image.cpp view/ImageFile.cpp view/ImageFile.hpp)
endif()
if(MITCAD_HEIF)
  find_package(libheif CONFIG REQUIRED)
  if(TARGET libheif::heif)
    set(mitcad_heif_target libheif::heif)
  elseif(TARGET heif)
    set(mitcad_heif_target heif)
  else()
    message(FATAL_ERROR "libheif's CMake package has no target heif")
  endif()
  get_target_property(mitcad_heif_type ${mitcad_heif_target} TYPE)
  if(NOT mitcad_heif_type STREQUAL "SHARED_LIBRARY")
    message(FATAL_ERROR "libheif (LGPL) must be a shared library, not ${mitcad_heif_type}: use a dynamic vcpkg triplet")
  endif()
  # A Debug build links libheif's release library, as OCCT's (the top
  # CMakeLists.txt, MITCAD_OCCT_RELEASE_IN_DEBUG). Its debug one would put
  # vcpkg's debug/lib before lib in the executable's RUNPATH, and the loader
  # would then take OCCT's debug libraries, which have the same names as
  # the release ones: every OCCT computation several times slower (the UI
  # tests' previews and recomputes ran out of time, mitcad#850).
  if(MITCAD_OCCT_RELEASE_IN_DEBUG AND NOT WIN32)
    get_target_property(mitcad_heif_imported ${mitcad_heif_target} ALIASED_TARGET)
    if(NOT mitcad_heif_imported)
      set(mitcad_heif_imported ${mitcad_heif_target})
    endif()
    set_target_properties(${mitcad_heif_imported} PROPERTIES MAP_IMPORTED_CONFIG_DEBUG "Release")
  endif()
  # libde265, which only libheif links: a shared library too, when its
  # package names a target to check.
  find_package(libde265 CONFIG QUIET)
  foreach(mitcad_de265_target IN ITEMS libde265::de265 de265 libde265)
    if(TARGET ${mitcad_de265_target})
      get_target_property(mitcad_de265_type ${mitcad_de265_target} TYPE)
      if(NOT mitcad_de265_type STREQUAL "SHARED_LIBRARY")
        message(FATAL_ERROR "libde265 (LGPL) must be a shared library, not ${mitcad_de265_type}")
      endif()
    endif()
  endforeach()
  target_link_libraries(mitcad PRIVATE ${mitcad_heif_target})
  target_compile_definitions(mitcad PRIVATE MITCAD_HEIF=1)
  if(BUILD_TESTING)
    target_link_libraries(test_app PRIVATE ${mitcad_heif_target})
    target_compile_definitions(test_app PRIVATE MITCAD_HEIF=1)
  endif()
endif()

# The rule block editor of the Configurations window (mitcad#808,
# app/COMMANDS.md "The rule block editor"): a rule's blocks and their JSON,
# the native canvas and palette, and the Rules page that
# browser/ConfigurationRulesPage.cpp puts in the window.
target_sources(mitcad PRIVATE
  configurations/RuleBlocks.cpp
  configurations/RuleBlocks.hpp
  configurations/RuleCanvas.cpp
  configurations/RuleCanvas.hpp
  configurations/RulesPage.cpp
  configurations/RulesPage.hpp)
if(BUILD_TESTING)
  # The expression <-> block round trip, the edits, and the page on a design
  # of the test's own: Qt Widgets on the offscreen platform.
  target_sources(test_app PRIVATE tests/rule_blocks.cpp
                 configurations/RuleBlocks.cpp configurations/RuleBlocks.hpp
                 configurations/RuleCanvas.cpp configurations/RuleCanvas.hpp
                 configurations/RulesPage.cpp configurations/RulesPage.hpp
                 framework/FormulaEditor.cpp framework/FormulaEditor.hpp
                 framework/ParameterField.cpp framework/ParameterField.hpp
                 framework/ParameterSuggest.cpp framework/ParameterSuggest.hpp)
endif()

# The document snapshot's feature lookup by uid (mitcad#833): read() from a
# model's answers, the command context's interface.
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/document_snapshot.cpp browser/DocumentSnapshot.cpp
                 browser/DocumentSnapshot.hpp)
endif()

# Tables of a design (mitcad#667, docs/table-lookups.md): the Tables tab of
# Change Parameters.
target_sources(mitcad PRIVATE browser/TablesPage.cpp browser/TablesPage.hpp)

# Card widths and windows within the visible screen (mitcad#818): the test
# steps of both frontends, and in the AppKit frontend the cards' width
# handles and the secondary windows' scrolling body and screen fitting.
target_sources(mitcad PRIVATE framework/TestDriverWindowFit.cpp)
if(APPLE AND MITCAD_MACOS_NATIVE_UI)
  target_sources(mitcad PRIVATE
    platform/macos/NativeCardWidths.hpp platform/macos/NativeCardWidths.mm
    platform/macos/NativeWindowFit.hpp platform/macos/NativeWindowFit.mm)
  set_source_files_properties(platform/macos/NativeCardWidths.mm platform/macos/NativeWindowFit.mm
    PROPERTIES COMPILE_OPTIONS -fobjc-arc)
endif()

# The Sketch page of Preferences (mitcad#860): the first dimension that
# scales the whole sketch to it.
target_sources(mitcad PRIVATE sketch/SketchPreferences.cpp sketch/SketchPreferences.hpp)

# 3D mice (mitcad#951, docs/rendering.md "Navigation"): the view's mapping
# and the controller, the camera's side in OcctViewer, and one backend per
# platform without a vendor SDK (Raw Input on Windows, spacenavd's socket
# on Linux and the BSDs, IOKit's HID Manager on macOS).
target_sources(mitcad PRIVATE
  platform/SpaceMouseDevice.hpp
  view/SpaceMouseCamera.hpp
  view/SpaceMouseController.cpp
  view/SpaceMouseController.hpp
  view/SpaceMouseNavigation.hpp
  view/SpaceMouseView.cpp)
if(WIN32)
  target_sources(mitcad PRIVATE platform/windows/SpaceMouseWindows.cpp)
  # HidD_GetProductString for the device's name in the log: hid.dll is in every Windows.
  target_link_libraries(mitcad PRIVATE hid)
elseif(APPLE)
  target_sources(mitcad PRIVATE platform/macos/SpaceMouseMac.cpp)
  target_link_libraries(mitcad PRIVATE "-framework IOKit" "-framework CoreFoundation")
else()
  target_sources(mitcad PRIVATE platform/unix/SpaceMouseSpnav.cpp)
endif()
if(BUILD_TESTING)
  # The reports' parsing, the mapping to the camera and the controller with
  # a backend of the test's own (plain Qt Core and Gui), and the camera's step on
  # an OCCT camera as OcctViewer applies it (Graphic3d_Camera: TKService).
  target_sources(test_app PRIVATE tests/space_mouse.cpp view/SpaceMouseCamera.hpp view/SpaceMouseController.cpp
                 view/SpaceMouseController.hpp)
  target_link_libraries(test_app PRIVATE TKService)
endif()
# The AI chat panel (mitcad#947, docs/ai-chat.md): the user's own agent
# through its own interface (agent/ChatAgent.hpp; Claude Code, Codex), the
# Qt panel, its approvals enforced by the live server (mcp/ChatGate.hpp),
# one undo step per message and the history per design. The chat's own
# logic is Rust (core/mcp/src/chat.rs, agent_chat through the bridge).
target_sources(mitcad PRIVATE
  agent/ChatAgent.hpp
  agent/ChatController.cpp agent/ChatController.hpp
  agent/ChatHistoryStore.cpp agent/ChatHistoryStore.hpp
  agent/ChatLogic.hpp
  agent/ChatPanel.cpp agent/ChatPanel.hpp
  agent/ChatPreferences.cpp agent/ChatPreferences.hpp
  agent/ChatSettings.cpp agent/ChatSettings.hpp
  agent/ClaudeCodeAgent.cpp agent/ClaudeCodeAgent.hpp
  agent/CodexAgent.cpp agent/CodexAgent.hpp
  agent/MainWindowChat.cpp
  mcp/ChatGate.hpp)
if(BUILD_TESTING)
  # The history files (keys, the end read, moves, clearing) and the chat's
  # settings: plain Qt Core.
  target_sources(test_app PRIVATE tests/chat_history.cpp agent/ChatHistoryStore.cpp agent/ChatHistoryStore.hpp
                 agent/ChatSettings.cpp agent/ChatSettings.hpp)
  # The chat's UI tests' scripted agent (tools/ui-chat-test.py): Claude
  # Code's stream-json (as `claude`) or Codex's app server (as `codex`) on
  # stdin and stdout, the real bridge as its MCP server; no AI. It is never
  # installed.
  add_executable(test_chat_agent tests/chat_test_agent.cpp)
  target_link_libraries(test_chat_agent PRIVATE Qt6::Core mitcad_warnings)
  mitcad_utf8_code_page(test_chat_agent)
  add_dependencies(mitcad test_chat_agent)
  # The tool calls' group rows (mitcad#983) as the panel asks for them, the
  # chat's Rust logic through the bridge.
  add_executable(test_chat_tool_groups tests/chat_tool_groups.cpp agent/ChatLogic.hpp)
  target_include_directories(test_chat_tool_groups PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_chat_tool_groups PRIVATE mitcad_ffi Qt6::Core mitcad_warnings)
  add_test(NAME app.chat_tool_groups COMMAND test_chat_tool_groups)
  set_tests_properties(app.chat_tool_groups PROPERTIES
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()

# A stored view's height set as OCCT's camera scale through the view's
# aspect (mitcad#959): OcctViewer::setCamera restores named views and Home
# at any aspect; the unit tests use OCCT's cameras (TKService).
target_sources(mitcad PRIVATE view/CameraHeight.hpp)
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/camera_height.cpp view/CameraHeight.hpp)
  target_link_libraries(test_app PRIVATE TKService)
endif()
# Create Sketch and the Construct commands ask whether to activate the
# component of a picked face, plane or edge (mitcad#960, app/COMMANDS.md
# "Activating the picked component"): the decision and the question, and
# the main window's side.
target_sources(mitcad PRIVATE
  browser/ActivateQuestion.cpp
  browser/ActivateQuestion.hpp
  MainWindowActivate.cpp)
if(BUILD_TESTING)
  # When it asks and with what names, and the message box: Qt Widgets on
  # the offscreen platform.
  target_sources(test_app PRIVATE tests/activate_question.cpp browser/ActivateQuestion.cpp
                 browser/ActivateQuestion.hpp)
endif()
# A feature's build state on the timeline and the selection card
# (mitcad#961): the cell's fill, its status ring inside the selection's and
# the status badge that stays in every state; plain Qt Gui.
target_sources(mitcad PRIVATE browser/FeatureStatus.cpp browser/FeatureStatus.hpp)
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/feature_status.cpp browser/FeatureStatus.cpp browser/FeatureStatus.hpp)
endif()
# The sketch points' markers and the overlay's highlights with the markers over
# them (mitcad#946): plain Qt Gui, painted into images in the test.
target_sources(mitcad PRIVATE sketch/PointMarkers.cpp sketch/PointMarkers.hpp)
if(BUILD_TESTING)
  target_sources(test_app PRIVATE tests/sketch_point_markers.cpp sketch/PointMarkers.cpp sketch/PointMarkers.hpp)
endif()
