# SPDX-License-Identifier: MIT
# PrintSupport: File › Print… (mitcad#1089), its printer and print dialog.
find_package(Qt6 REQUIRED COMPONENTS Svg Network PrintSupport)

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
  # Show used sketches under their features (mitcad#1699).
  browser/BrowserPreferences.cpp
  browser/BrowserPreferences.hpp
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
  # Convert to Features: a placed library part to its own features (mitcad#1815).
  browser/ConvertToFeatures.cpp
  browser/ConvertToFeatures.hpp
  # File > Design Properties: the design's summary and statistics (mitcad#1088).
  browser/DesignPropertiesDialog.cpp
  browser/DesignPropertiesDialog.hpp
  browser/ComponentColors.hpp
  browser/TimelineWidget.cpp
  browser/TimelineWidget.hpp
  browser/TimelineSearch.cpp
  browser/TimelineSearch.hpp
  # The command framework (COMMANDS.md).
  framework/Appearances.cpp
  framework/Appearances.hpp
  # Preferences' Theme: System, Light or Dark (mitcad#1330).
  framework/AppTheme.cpp
  framework/AppTheme.hpp
  framework/AutoselectPanel.cpp
  framework/AutoselectPanel.hpp
  framework/ChromeStyle.cpp
  framework/ChromeStyle.hpp
  # Restarting the application (mitcad#419: the layout preference).
  framework/Restart.cpp
  framework/Restart.hpp
  framework/Command.cpp
  framework/Command.hpp
  framework/CommandItemText.cpp
  framework/CommandPresenter.hpp
  framework/CommandMessage.cpp
  framework/CommandMessage.hpp
  framework/CommandPanel.cpp
  framework/CommandPanel.hpp
  # The panel's searchable choice drop-downs, also the Make card's (mitcad#1239).
  framework/PickerComboBox.cpp
  framework/PickerComboBox.hpp
  # An inspection's answer as rows of names and values (mitcad#1010).
  framework/ResultRows.cpp
  framework/ResultRows.hpp
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
  framework/RibbonFit.cpp
  framework/RibbonFit.hpp
  framework/Selection.cpp
  framework/Selection.hpp
  # Openings picked by their free edges (mitcad#1927).
  framework/OpeningEdges.cpp
  framework/OpeningEdges.hpp
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
  framework/UserText.hpp
  framework/ViewHintPlace.hpp
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
  # The sketch constraints' Shift keys (mitcad#1229).
  commands/SketchConstraintKeys.hpp
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
  # Auto Constrain in sketches (mitcad#2019).
  commands/AutoConstrainCommands.cpp
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
  view/ShortcutOverview.cpp
  view/ShortcutOverview.hpp
  # Preferences › Restore Defaults (mitcad#1887).
  view/PreferencesRestore.cpp
  view/ViewNavigation.hpp
  # The File menu: import and export (U6).
  files/FileDialogs.cpp
  files/FileDialogs.hpp
  files/FileFormats.cpp
  files/FileFormats.hpp
  files/F3dImport.cpp
  files/F3dImport.hpp
  files/ImportWorkerFailure.cpp
  files/ImportWorkerFailure.hpp
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
  # The UI tests' guard against internal identifiers in visible text
  # (mitcad#1741).
  framework/VisibleTextGuard.cpp
  framework/VisibleTextGuard.hpp
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
  update/UpdateOffer.cpp
  update/UpdateOffer.hpp
  update/UpdatePreferences.cpp
  update/UpdatePreferences.hpp
  update/UpdateSettings.cpp
  update/UpdateSettings.hpp
  update/UpdaterChecks.hpp
  # The Linux AppImage's OpenSSL configuration (mitcad#1208).
  update/OpenSslConfiguration.cpp
  update/OpenSslConfiguration.hpp
  # The version over the 3D view's lower right (mitcad#1209).
  view/VersionWatermark.cpp
  view/VersionWatermark.hpp
  # 3D Print: bodies to a slicer (mitcad#13).
  files/MainWindowPrint.cpp
  # Plates…: the design's print plates (mitcad#1050).
  files/MainWindowPlates.cpp
  files/Print3d.cpp
  files/Print3d.hpp
  # Use the open slicer window: the slicer processes it started (mitcad#1162).
  files/SlicerProcesses.cpp
  files/SlicerProcesses.hpp
  # Save As Mesh: a body's or a component's bodies to mesh files (mitcad#734).
  files/MainWindowSaveAsMesh.cpp
  files/SaveAsMesh.cpp
  files/SaveAsMesh.hpp
  # Save Component as Design: a component as a design of its own (mitcad#1057).
  files/MainWindowSaveComponent.cpp
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

# Windows 10's dark app mode (mitcad#1282): Qt's own style with a dark
# palette while the app mode is dark; nothing on Windows 11 and elsewhere.
target_sources(mitcad PRIVATE platform/WindowsTheme.hpp)
if(WIN32)
  target_sources(mitcad PRIVATE platform/windows/WindowsTheme.cpp)
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
# The view's lines from detached copies of the edges, cut finely
# (mitcad#1633): bodies, highlights, the hover, sketch curves.
target_sources(mitcad PRIVATE
  view/ViewEdges.cpp
  view/ViewEdges.hpp)
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
      platform/macos/NativeNavigationGuide.hpp platform/macos/NativeNavigationGuide.mm
      platform/macos/NativeColors.hpp platform/macos/NativeColors.mm)
    set_source_files_properties(
      platform/macos/NativeMacWindow.mm platform/macos/NativeForms.mm
      platform/macos/NativeDialogs.mm platform/macos/NativeTestInput.mm
      platform/macos/NativeBindingsCard.mm
      platform/macos/NativeInlineEditor.mm
      platform/macos/NativeTableCells.mm platform/macos/NativeRuleCanvas.mm
      platform/macos/NativeNavigationGuide.mm platform/macos/NativeColors.mm
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
    # The pickers' popover: the Material and Appearance tools' and the
    # Materials window's (mitcad#1066, mitcad#1068).
    target_sources(mitcad PRIVATE platform/NativePicker.hpp platform/macos/NativePicker.mm)
    set_source_files_properties(platform/macos/NativePicker.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
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
# Help › User Guide (mitcad#1032) opens docs/user-guide.md in the public
# repository README.md names, at the release's tag v<version>. main never
# carries a release's version commit (that is made on the release's own
# branch): it keeps MITCAD_MAIN_VERSION, the version of the last release
# made on main, so a build at that version is a development build and opens
# main's guide. MITCAD_GUIDE_REF, when set, is the branch or tag instead.
set(MITCAD_MAIN_VERSION "0.18.0")
set(MITCAD_GUIDE_REF "" CACHE STRING
    "The branch or tag of the user guide that Help > User Guide opens (empty: the release's tag, main in a development build)")
if(MITCAD_GUIDE_REF)
  set(mitcad_guide_ref "${MITCAD_GUIDE_REF}")
elseif(PROJECT_VERSION VERSION_EQUAL MITCAD_MAIN_VERSION)
  set(mitcad_guide_ref "main")
else()
  set(mitcad_guide_ref "v${PROJECT_VERSION}")
endif()
target_compile_definitions(mitcad PRIVATE
  MITCAD_GUIDE_URL="https://github.com/mitcad/Mitcad/blob/${mitcad_guide_ref}/docs/user-guide.md")
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
  # The programs still running from the installation (mitcad#1454): the
  # updater asks the MCP bridges to end and names what runs when the
  # installer refuses to replace files in use. What still runs it offers to
  # end in a task dialog (mitcad#1584): comctl32 6 is loaded at run time,
  # user32 asks MitCad's windows to close.
  target_sources(mitcad-updater PRIVATE update/RunningPrograms.hpp)
  target_link_libraries(mitcad-updater PRIVATE user32)
endif()

# A mixed installation at start-up (mitcad#1454): the OCCT DLLs loaded
# against this build's stamp (cmake/OcctBuildStamp.cmake), with the
# programs still running from bin; Windows only, nothing elsewhere.
target_sources(mitcad PRIVATE update/BuildStamp.hpp update/InstallationCheck.cpp update/InstallationCheck.hpp
               update/RunningPrograms.hpp)
mitcad_occt_build_stamp(mitcad)

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

# Suppress When and Show When (mitcad#809); the option mappings of the
# Material and Appearance tools' fx (mitcad#861, mitcad#1059) in plain Qt
# Core.
target_sources(mitcad PRIVATE
  browser/Bindings.cpp
  browser/Bindings.hpp
  browser/OptionMapping.cpp)
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
  target_sources(test_app PRIVATE framework/Dialogs.cpp framework/Dialogs.hpp)
  # The AppImage's desktop file and icons in the user's data folder.
  target_sources(test_app PRIVATE framework/DesktopEntry.cpp framework/DesktopEntry.hpp)
  # The rendered view's exposure and view transforms (mitcad#47): plain Qt
  # Gui.
  target_sources(test_app PRIVATE render/RenderImage.cpp render/RenderImage.hpp)
  # The render device's choice and the CPU's fallback (mitcad#50): plain
  # C++.
  target_sources(test_app PRIVATE render/RenderDevices.cpp render/RenderDevices.hpp)
  # Which render worker starts: the picked one, the shipped one, the
  # override (mitcad#1332): plain Qt Core.
  target_sources(test_app PRIVATE render/RenderWorkerChoice.cpp render/RenderWorkerChoice.hpp)
  # Feedback and error reports (mitcad#61, mitcad#62): masking, crash files,
  # duplicate keys, the issue form's address.
  target_sources(test_app PRIVATE report/ReportText.cpp report/ReportText.hpp)
  # The addresses of Cloud projects (mitcad#89): composed and read back.
  target_sources(test_app PRIVATE files/CloudAddress.cpp files/CloudAddress.hpp)
  # The timeline search's words and filters (mitcad#179): plain Qt Core.
  target_sources(test_app PRIVATE browser/TimelineSearch.cpp browser/TimelineSearch.hpp)
  # The sketch's auto constraints while drawing (mitcad#189): plain Qt Core
  # and plane geometry.
  target_sources(test_app PRIVATE sketch/AutoConstraint.cpp sketch/AutoConstraint.hpp sketch/SketchGeometry.cpp)
  # Selection inputs' entries: a click toggles one per region (mitcad#199):
  # plain Qt Core.
  target_sources(test_app PRIVATE framework/Selection.cpp framework/Selection.hpp framework/CommandItemText.cpp)
  # When a navigation of the view ends (mitcad#214): plain C++.
  target_sources(test_app PRIVATE view/ViewNavigation.hpp)
  # Where an orthographic render's camera stands (mitcad#215): plain C++.
  target_sources(test_app PRIVATE render/RenderCamera.hpp)
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
      # Without WAYLAND_DISPLAY, or the app picks the desktop's Wayland
      # session over xvfb-run's display (mitcad#2031).
      set(default_shortcuts_prefix env -u WAYLAND_DISPLAY "${MITCAD_XVFB_RUN_EXECUTABLE}" -a -s "-screen 0 1280x1024x24")
    endif()
  endif()
  add_test(NAME app.default_shortcuts COMMAND ${default_shortcuts_prefix} $<TARGET_FILE:mitcad> --ui=qt --no-recovery --no-mcp)
  set_tests_properties(app.default_shortcuts PROPERTIES TIMEOUT 90
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;MITCAD_TEST_DEFAULT_SHORTCUTS=1;MITCAD_SETTINGS_DIR=${CMAKE_CURRENT_BINARY_DIR}/default-shortcuts-settings;MITCAD_AUTOSAVE_DIR=${CMAKE_CURRENT_BINARY_DIR}/default-shortcuts-autosave;MITCAD_RESULT_STORE=off"
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
  # The command layout's coverage (mitcad#1951): every registered command
  # placed in app/resources/layout/default.json or listed as on no toolbar,
  # every id there a command, every key a caption.
  add_test(NAME app.layout_coverage COMMAND ${default_shortcuts_prefix} $<TARGET_FILE:mitcad> --ui=qt --no-recovery --no-mcp)
  set_tests_properties(app.layout_coverage PROPERTIES TIMEOUT 90
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;MITCAD_TEST_LAYOUT_COVERAGE=1;MITCAD_SETTINGS_DIR=${CMAKE_CURRENT_BINARY_DIR}/layout-coverage-settings;MITCAD_AUTOSAVE_DIR=${CMAKE_CURRENT_BINARY_DIR}/layout-coverage-autosave;MITCAD_RESULT_STORE=off"
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
  # The view's lines (mitcad#1633): detached copies of a body's edges cut
  # finely, the body's own edges unwritten, a highlight's copy the body's.
  add_executable(test_view_edges tests/view_edges.cpp view/ViewEdges.cpp)
  target_include_directories(test_view_edges PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_view_edges PRIVATE mitcad_geometry_io mitcad_geometry TKV3d TKPrim TKBO mitcad_warnings)
  add_test(NAME app.view_edges COMMAND test_view_edges)
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
  # No print server (mitcad#1591): the print tests' QPrinters print to PDF
  # files and must not wait for the machine's printers (CUPS sets up a queue
  # for a network printer it found and waits for that printer to answer).
  set_tests_properties(app.unit app.render_devices PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;CUPS_SERVER=/nonexistent/mitcad-test-cups.sock"
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
# The render worker picked in Preferences (mitcad#1332): a worker the user
# built, such as an OptiX one, else the shipped one.
target_sources(mitcad PRIVATE render/RenderWorkerChoice.cpp render/RenderWorkerChoice.hpp)
# File > Render Image (mitcad#48): the final render in the worker's batch
# mode; RenderBatch is shared with mitcad-cli render.
target_sources(mitcad PRIVATE
  render/RenderBatch.cpp
  render/RenderBatch.hpp
  render/RenderImageDialog.cpp
  render/RenderImageDialog.hpp)
# The Render workspace (mitcad#2013): the stages of the Render tab, Render
# Environment's and Render Image's contents as panels, Ready to Render.
target_sources(mitcad PRIVATE
  MainWindowRender.cpp
  render/RenderEnvironmentPanel.cpp
  render/RenderEnvironmentPanel.hpp
  render/RenderImagePanel.cpp
  render/RenderImagePanel.hpp
  render/RenderReadyCheck.cpp
  render/RenderReadyCheck.hpp
  render/RenderWorkspace.cpp
  render/RenderWorkspace.hpp)
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
  set_tests_properties(app.render_surface_macos PROPERTIES RESOURCE_LOCK macos_desktop)
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
# A user's own OptiX worker (mitcad#1332, cmake/RenderOptix.cmake) has a
# name of its own, so that it never replaces the shipped mitcad-render.
if(MITCAD_RENDER_OPTIX)
  set_target_properties(mitcad-render PROPERTIES OUTPUT_NAME mitcad-render-optix)
endif()
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
  set_tests_properties(app.render_surface_xpc_macos PROPERTIES TIMEOUT 45)
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

# The interface languages (mitcad#1460): the language chosen at start-up and
# its catalogues, built from app/translations and copied beside mitcad
# (cmake/Translations.cmake, before the bundle, which carries them).
target_sources(mitcad PRIVATE framework/Language.cpp framework/Language.hpp)
# The model's messages in the interface language (mitcad#1284): matched to
# their templates (ModelMessageTemplates.cpp, generated from core/ by
# tools/translations) and translated in the context ModelMessage.
target_sources(mitcad PRIVATE framework/ModelMessages.cpp framework/ModelMessages.hpp
               framework/ModelMessageTemplates.cpp)
include("${PROJECT_SOURCE_DIR}/cmake/Translations.cmake")

# Insert › Component from Web (mitcad#1579, GitHub mitcad/Mitcad#191,
# app/COMMANDS.md "Component from Web"): an embedded browser with the user's
# own sites whose downloads of the formats MitCad imports, and zips of them,
# come into the design as components. The browser is the Chromium Embedded
# Framework (CEF, BSD-3-Clause, with Chromium's components under their
# licences), in the web browser component: the module mitcad-webbrowser, its
# subprocess helper and CEF's runtime, a package of its own per platform that
# MitCad downloads on first use and verifies with the release key; never
# part of the installers, the AppImage or the disk image. mitcad links
# nothing of CEF and starts without the component. What MitCad decides
# without the browser (the format filter, the name and part number, the
# sites, the temporary folders, the component's installation and checks)
# is in mitcad itself; the component is built by
# app/webbrowser/CMakeLists.txt, a directory of its own, where a CEF
# distribution is.
target_sources(mitcad PRIVATE
  webbrowser/BrowserComponent.cpp
  webbrowser/BrowserComponent.hpp
  webbrowser/BrowserComponentDownload.cpp
  webbrowser/BrowserComponentDownload.hpp
  webbrowser/BrowserComponentInstall.cpp
  webbrowser/BrowserComponentInstall.hpp
  webbrowser/BrowserModuleLoad.cpp
  webbrowser/BrowserModuleLoad.hpp
  webbrowser/ComponentBrowserHost.hpp
  webbrowser/ComponentWebFiles.cpp
  webbrowser/ComponentWebFiles.hpp
  webbrowser/MainWindowComponentWeb.cpp
  webbrowser/BrowserDiagnostics.cpp
  webbrowser/BrowserDiagnostics.hpp)
# The first use with a local manifest (mitcad#1579): a package and a manifest
# signed by mitcad-release with a test key, checked, verified, unpacked and
# installed through the bridge; a tampered package and another interface
# refused.
if(BUILD_TESTING AND TARGET mitcad-release)
  add_executable(test_browser_component_flow tests/browser_component_flow.cpp webbrowser/BrowserComponent.cpp
    webbrowser/BrowserComponent.hpp webbrowser/BrowserComponentInstall.cpp webbrowser/BrowserComponentInstall.hpp)
  target_include_directories(test_browser_component_flow PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_browser_component_flow PRIVATE Qt6::Core Qt6::Gui mitcad_ffi mitcad_warnings)
  add_dependencies(test_browser_component_flow mitcad-release)
  add_test(NAME app.browser_component_flow
    COMMAND test_browser_component_flow "$<TARGET_FILE:mitcad-release>")
  set_tests_properties(app.browser_component_flow PROPERTIES TIMEOUT 120
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()
add_subdirectory(webbrowser)
# In-app tutorials (mitcad#2012, docs/tutorials.md): the guide step shown in
# a design window (one engine for a tutorial page's bridge and live MCP's
# ui_guide), the tutorial pane on the web browser component's module and the
# Start page's choice of the tutorial to start (mitcad#2346).
target_sources(mitcad PRIVATE
  tutorials/GuideBindings.cpp
  tutorials/GuideBindings.hpp
  tutorials/GuideDemonstration.cpp
  tutorials/GuideDemonstration.hpp
  tutorials/GuideStep.cpp
  tutorials/GuideStep.hpp
  tutorials/MainWindowGuide.cpp
  tutorials/MainWindowTutorials.cpp
  tutorials/TutorialChoice.cpp
  tutorials/TutorialChoice.hpp
  tutorials/TutorialGuide.cpp
  tutorials/TutorialGuide.hpp
  tutorials/TutorialSources.cpp
  tutorials/TutorialSources.hpp)
# The tutorial pane's places (mitcad#2367): left, right or centred, the
# move animated unless the system asks for reduced motion.
target_sources(mitcad PRIVATE
  framework/PaneMotion.cpp
  framework/PaneMotion.hpp
  tutorials/TutorialPlace.hpp)
# Windows Explorer's thumbnails of designs (mitcad#1695): the thumbnail
# handler mitcad-thumbnails.dll; Windows only.
if(WIN32)
  add_subdirectory(thumbnails)
endif()

if(APPLE)
  # Configure the bundle after its render worker exists, so its
  # install rules include the worker, denoiser module and their libraries.
  option(MITCAD_3DXWARE_PREVIEW "Build a separately identified 3DxWare test application" OFF)
  include(packaging/macos/Bundle.cmake)
  # Finder's thumbnails of designs (mitcad#1696): the Quick Look extension
  # MitCadThumbnails.appex in the bundle's PlugIns.
  if(NOT MITCAD_3DXWARE_PREVIEW)
    include("${PROJECT_SOURCE_DIR}/cmake/MacQuickLook.cmake")
  endif()
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
# Poly Haven as an online texture source (mitcad#313, app/materials): the
# requests of the texture sources on the internet (Qt Network; files of a
# folder for UI tests) and the source. The unit tests give it a fake
# fetcher and never use the network.
target_sources(mitcad PRIVATE
  materials/PolyHavenTextureSource.cpp
  materials/PolyHavenTextureSource.hpp
  materials/TextureFetcher.cpp
  materials/TextureFetcher.hpp)
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
# The edit panel of a derived part: a BUILD part, or a component inserted
# with options (mitcad#1812, app/COMMANDS.md "Derived Component").
target_sources(mitcad PRIVATE commands/DerivedPartCommands.cpp)
# BUILD's End Cap: plates closing profiles' and tubes' ends (mitcad#2243,
# app/COMMANDS.md "End Cap").
target_sources(mitcad PRIVATE
  commands/BuildEndCapCommand.cpp
  commands/BuildEndCapCommand.hpp)
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
# The suppliers' articles of a library part's row as a table behind a
# button (mitcad#979): Insert from Library, Library Components and Edit Options….
target_sources(mitcad PRIVATE
  library/SupplierPartsDialog.cpp
  library/SupplierPartsDialog.hpp)
# The suppliers' shops (mitcad#950): the region and the shops chosen in
# Preferences (its Suppliers page), and the product links of the
# suppliers' part numbers (the model's `supplier_shops`, `supplier_links`).
target_sources(mitcad PRIVATE
  library/SupplierPreferences.cpp
  library/SupplierPreferences.hpp
  library/SupplierShops.cpp
  library/SupplierShops.hpp)
# The web viewer's pages (mitcad#423, app/web, app/COMMANDS.md "Web
# page"): a design as one self-contained HTML file, File > Export > Web
# page. The viewer's files (app/web/viewer, its libraries in
# third_party/three) are resources under :/web-viewer/<repository path>,
# found by a glob that may find none (pageHtml then names the file the
# build lacks).
target_sources(mitcad PRIVATE
  web/DocumentContext.hpp
  web/WebBodies.cpp
  web/WebGlb.cpp
  web/WebGlb.hpp
  web/WebPage.cpp
  web/WebPage.hpp
  web/WebPageHtml.cpp)
# File > Export's glTF types (mitcad#1826): the page's bodies and the
# component tree as a .glb or .gltf file of the page's writer.
target_sources(mitcad PRIVATE
  web/GltfExport.cpp
  web/GltfExport.hpp)
file(GLOB_RECURSE MITCAD_WEB_VIEWER_FILES CONFIGURE_DEPENDS
  "${PROJECT_SOURCE_DIR}/app/web/viewer/*" "${PROJECT_SOURCE_DIR}/third_party/three/*")
if(MITCAD_WEB_VIEWER_FILES)
  foreach(web_viewer_file IN LISTS MITCAD_WEB_VIEWER_FILES)
    file(RELATIVE_PATH web_viewer_alias "${PROJECT_SOURCE_DIR}" "${web_viewer_file}")
    set_source_files_properties("${web_viewer_file}" PROPERTIES QT_RESOURCE_ALIAS "${web_viewer_alias}")
  endforeach()
  qt_add_resources(mitcad web_viewer PREFIX /web-viewer FILES ${MITCAD_WEB_VIEWER_FILES})
endif()
# A project's web viewer (mitcad#423, app/web): File > Project > Web Viewer
# and Update Web Viewer, the Save that updates a design's page, the web/
# folder's index and designs.json (the pages: web/WebPage.hpp).
target_sources(mitcad PRIVATE
  web/MainWindowWeb.cpp
  web/ProjectWeb.cpp
  web/ProjectWeb.hpp)
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
# Drawing templates of the user's own (mitcad#1625): the title block editor
# on the sheet and a sheet's custom size in Create Drawing and Sheet….
target_sources(mitcad PRIVATE
  drawings/SheetSizeRows.cpp
  drawings/SheetSizeRows.hpp
  drawings/TitleBlockEditor.cpp
  drawings/TitleBlockEditor.hpp)
# Sketch text from font files: Preferences' Fonts page and the font
# chooser of Edit Text (mitcad#473).
target_sources(mitcad PRIVATE
  sketch/FontFiles.cpp
  sketch/FontFiles.hpp)
# The sketch's origin and its x and y axes as picks for constraints and
# dimensions (mitcad#500).
target_sources(mitcad PRIVATE sketch/SketchAxes.cpp)
# The text a pick inside a letter or its counter stands for (mitcad#1369):
# Edit Text and Edit QR Code from a text's region.
target_sources(mitcad PRIVATE sketch/SketchTextPicks.cpp)
# Sketch Move/Copy (mitcad#1563): Point to Point's points picked with the
# sketch's snaps while the panel runs, Translate's handle, and the places
# they name and the objects' middle.
target_sources(mitcad PRIVATE sketch/SketchPlaces.cpp sketch/SketchCommandPicks.cpp)
# Chains of sketch curves: Offset's Chain Selection and a double-click on a
# curve, its closed loop or open chain (mitcad#1541).
target_sources(mitcad PRIVATE
  sketch/SketchChains.cpp
  sketch/SketchChains.hpp)
# Mount frames of any part and Place Part (mitcad#525, app/COMMANDS.md
# "Component libraries"): a library part placed by its frame on a face, a
# hole or a grid; the frames a manifest lists.
target_sources(mitcad PRIVATE
  commands/MountCommands.cpp
  commands/MountCommands.hpp
  library/MountFrames.cpp
  library/MountFrames.hpp)
# A timber frame's openings (mitcad#519, app/COMMANDS.md "Timber"): their
# rows, text form and checks, and the Openings window that edits them
# beside Wall's, Floor / Roof's and Timber Frame's panels.
target_sources(mitcad PRIVATE
  build/OpeningsWindow.cpp
  build/OpeningsWindow.hpp
  build/TimberOpenings.cpp
  build/TimberOpenings.hpp)
# The joint highlight (mitcad#523): a selected joint's sides, frames and
# occurrences in the view, from the `joints` query.
target_sources(mitcad PRIVATE
  MainWindowJointHighlight.cpp
  framework/JointHighlight.cpp
  framework/JointHighlight.hpp)
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
# The Projects panel's rows (mitcad#529): compact marks for their state, a
# name elided in the middle.
target_sources(mitcad PRIVATE
  projects/ProjectRowText.cpp
  projects/ProjectRowText.hpp)
# File > Open from Project (mitcad#530): the projects browsed as folders,
# their designs searched by name.
target_sources(mitcad PRIVATE
  projects/ProjectBrowser.cpp
  projects/ProjectBrowser.hpp
  projects/ProjectBrowserData.cpp
  projects/ProjectBrowserData.hpp)
# The view's fit of a design's first bodies, once per design (mitcad#467;
# not again after every body was hidden and one shown).
target_sources(mitcad PRIVATE view/FirstBodiesFit.hpp)
# Section Analysis off the UI thread (mitcad#438): the section worker's
# rounds, and the window's clips, view and measurement through them.
target_sources(mitcad PRIVATE
  MainWindowSection.cpp
  framework/SectionWorker.cpp
  framework/SectionWorker.hpp)
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
# The patterns' Direction Type and Distance Type (mitcad#545): the panels'
# choices as the definitions' `symmetric` and `distance_type`, the length
# row's label, the options in the log.
target_sources(mitcad PRIVATE
  commands/PatternOptions.cpp
  commands/PatternOptions.hpp)

# The sketches a command shows for its time (mitcad#546): an edited
# feature's, as if their light bulbs were on, until its panel closes.
target_sources(mitcad PRIVATE view/CommandSketches.hpp)

# Change Parameters while a sketch or a feature is being created or edited
# (mitcad#564): SKETCH > MODIFY's entry, the parameter table's commands
# through an open panel's session, the panel's title row button.
target_sources(mitcad PRIVATE
  MainWindowParameterEdits.cpp
  framework/CommandPanelHeader.cpp
  framework/CommandSessionParameters.cpp
  framework/ParameterEdits.cpp
  framework/ParameterEdits.hpp)

# Expression fields' parameter suggestions while typing, the fx browser,
# name = value and Add User Parameter (mitcad#567, mitcad#568): feature
# panels, sketch dimensions and tool values, the parameter table.
target_sources(mitcad PRIVATE
  framework/ParameterSuggest.cpp
  framework/ParameterSuggest.hpp
  framework/ParameterField.cpp
  framework/ParameterField.hpp)

# Immutable edit-operation policy and manual override of delayed replies
# (mitcad#575): plain Qt Core, shared with the command session.
target_sources(mitcad PRIVATE framework/AutoOperationPolicy.hpp)

# Auto-update of the library sources (mitcad#576, app/COMMANDS.md "Component
# libraries"): the sources' settings (plain Qt Core) and the background fetch
# at start-up.
target_sources(mitcad PRIVATE
  files/LibraryAutoUpdate.cpp
  files/LibraryAutoUpdate.hpp
  files/LibrarySettings.cpp)

# The selection card's Create row (mitcad#619, app/COMMANDS.md "What the
# framework does"): up to four CREATE commands that fit the selection,
# Extrude first, under the selection's text.
target_sources(mitcad PRIVATE
  framework/SelectionCreate.cpp
  framework/SelectionCreate.hpp
  MainWindowSelectionCreate.cpp)

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

target_sources(mitcad PRIVATE framework/TestDriverManipulators.cpp)
target_sources(mitcad PRIVATE framework/TestDriverChrome.cpp)
target_sources(mitcad PRIVATE framework/MacToolButtonStyle.cpp framework/MacToolButtonStyle.hpp
               framework/TestDriverMacControls.cpp)
target_sources(mitcad PRIVATE framework/TestDriverDialogs.cpp platform/MacDialogs.hpp)


# Measure's history (mitcad#648, app/COMMANDS.md "Measure's history"): the
# rows kept while the design is open and their list in the Measure panel.
target_sources(mitcad PRIVATE
  MainWindowMeasureHistory.cpp
  commands/MeasureHistory.cpp
  commands/MeasureHistory.hpp
  commands/MeasureHistoryView.cpp
  commands/MeasureHistoryView.hpp)

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
# The models the print managers work through (mitcad#704, mitcad#1135):
# the open design's, or with no design open one of their own on the
# user's and the project's libraries.
target_sources(mitcad PRIVATE
  print/PrintItemsHost.cpp
  print/PrintItemsHost.hpp)
# Import from Slicer (mitcad#829): the one import of 3D Print and the
# print managers, its file dialog, line and Details…; the
# presets of an installed slicer to tick (mitcad#830).
target_sources(mitcad PRIVATE
  print/SlicerImport.cpp
  print/SlicerImport.hpp
  print/SlicerPresetsDialog.cpp
  print/SlicerPresetsDialog.hpp)

# Purge volumes on filament changes (mitcad#705): 3D Print's table of the
# filament slots.
target_sources(mitcad PRIVATE
  print/PurgeVolumesDialog.cpp
  print/PurgeVolumesDialog.hpp)

# Filament changes at planes, faces and heights in Send to Slicer
# (mitcad#1643): the panel's section, the Filament Change window and the
# view's print colours and rings.
target_sources(mitcad PRIVATE
  files/MainWindowFilamentChanges.cpp
  print/FilamentChangeDialog.cpp
  print/FilamentChangeDialog.hpp
  view/FilamentChangeOverlay.cpp)

# A filament's variants per printer and nozzle (mitcad#1054): the Variants
# tab of the Materials window.
target_sources(mitcad PRIVATE
  print/MaterialVariantsPage.cpp
  print/MaterialVariantsPage.hpp)

# A material's appearances (mitcad#1055): the appearance picker, a button
# with the swatch that opens the appearance library as a popover.
target_sources(mitcad PRIVATE
  print/AppearancePicker.cpp
  print/AppearancePicker.hpp)

# Placement by picked geometry (mitcad#656, app/COMMANDS.md "Placement by
# picked geometry"): the placement input every part placed by a pick
# shares, its plane geometry, and Gear Pair and Belt Drive.
target_sources(mitcad PRIVATE
  commands/DriveCommands.cpp
  commands/PlaceGeometry.cpp
  commands/PlaceGeometry.hpp
  commands/PlaceInputs.cpp
  commands/PlaceInputs.hpp)

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

# Navigation settings and actual camera input (mitcad#688).
target_sources(mitcad PRIVATE framework/TestDriverNavigation.cpp)

# Text-based formula assistance and the shared larger editor (mitcad#668).
target_sources(mitcad PRIVATE framework/FormulaEditor.cpp framework/FormulaEditor.hpp
                 framework/TestDriverFormula.cpp)

# Isolated assistant setup plans, quoting and atomic configuration writes.
if(BUILD_TESTING)
  add_executable(test_assistant_client tests/assistant_client.cpp)
  target_link_libraries(test_assistant_client PRIVATE Qt6::Core mitcad_warnings)
  mitcad_utf8_code_page(test_assistant_client)
  add_dependencies(mitcad test_assistant_client)
endif()

# Copy, Cut and Paste of sketch geometry (mitcad#788): the sketch fragment
# on the clipboard and the Paste tool.
target_sources(mitcad PRIVATE sketch/SketchClipboard.cpp sketch/SketchClipboard.hpp)
# Copy, Cut and Paste in a 3D sketch, and a 2D sketch's copy pasted into
# one (mitcad#1661).
target_sources(mitcad PRIVATE sketch/Sketch3dClipboard.cpp)
# Sketch blocks (mitcad#2110): Create, Insert, Explode and Edit Block, and
# an instance picked as one.
target_sources(mitcad PRIVATE sketch/SketchBlocks.cpp MainWindowSketchBlocks.cpp)

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

# HEIF images of image references: view/ImageFile.* decodes an image
# reference's file, the formats other than HEIF through Qt, HEIF through
# view/HeifDecoder.* (mitcad#1304): the operating system's decoder, in
# every build, MitCad shipping none: ImageIO on macOS, WIC with the
# system's HEIF decoder on Windows (windowscodecs, ole32), the
# distribution's libheif loaded at run time on Linux and the BSDs (QLibrary;
# never linked or shipped, no header needed). A build with MITCAD_HEIF (the
# top CMakeLists.txt, off by default, releases included; mitcad#850) links
# libheif with its HEVC decoder libde265 from vcpkg instead (the manifest's
# feature `heif`, without encoders); libheif and libde265 are LGPL: shared
# libraries only, never linked statically (docs/development.md, "Licence
# policy").
set(mitcad_heif_sources view/ImageFile.cpp view/ImageFile.hpp view/HeifDecoder.cpp view/HeifDecoder.hpp
    platform/PlatformHeif.hpp)
if(WIN32)
  list(APPEND mitcad_heif_sources platform/windows/HeifWic.cpp)
  set(mitcad_heif_platform_libraries windowscodecs ole32)
elseif(APPLE)
  list(APPEND mitcad_heif_sources platform/macos/HeifImageIO.mm)
  set_source_files_properties(platform/macos/HeifImageIO.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
  set(mitcad_heif_platform_libraries "-framework ImageIO" "-framework CoreGraphics" "-framework CoreFoundation"
      "-framework Foundation")
else()
  list(APPEND mitcad_heif_sources platform/unix/HeifUnix.cpp view/HeifLibrary.cpp view/HeifLibrary.hpp)
  set(mitcad_heif_platform_libraries "")
endif()
target_sources(mitcad PRIVATE ${mitcad_heif_sources})
if(mitcad_heif_platform_libraries)
  target_link_libraries(mitcad PRIVATE ${mitcad_heif_platform_libraries})
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
# and the controller, the camera's side in OcctViewer, and the platform
# backends, including optional routes through an installed 3DxWare driver.
target_sources(mitcad PRIVATE
  platform/SpaceMouseDevice.hpp
  platform/SpaceMouseDriverReports.hpp
  view/SpaceMouseCamera.hpp
  view/SpaceMouseController.cpp
  view/SpaceMouseController.hpp
  view/SpaceMouseNavigation.hpp
  view/SpaceMouseTiming.hpp
  view/SpaceMouseView.cpp
  view/SpaceMouseWheelGuard.hpp)
if(WIN32)
  target_sources(mitcad PRIVATE
    platform/windows/SpaceMouseWindows.cpp
    platform/windows/SpaceMouse3DxWare.cpp
    platform/windows/SpaceMouse3DxWare.hpp)
  # HID device names and the system Program Files path for the optional driver.
  target_link_libraries(mitcad PRIVATE hid shell32)
elseif(APPLE)
  target_sources(mitcad PRIVATE
    platform/macos/SpaceMouseMac.cpp
    platform/macos/SpaceMouse3DxWare.cpp
    platform/macos/SpaceMouse3DxWare.hpp)
  if(MITCAD_3DXWARE_PREVIEW)
    target_compile_definitions(mitcad PRIVATE MITCAD_3DXWARE_PREVIEW)
  endif()
  target_link_libraries(mitcad PRIVATE "-framework IOKit" "-framework CoreFoundation")
else()
  target_sources(mitcad PRIVATE platform/unix/SpaceMouseSpnav.cpp)
  if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    target_sources(mitcad PRIVATE
      platform/unix/SpaceMouseMagellan.cpp
      platform/unix/SpaceMouseMagellan.hpp)
  endif()
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
# Check again's decisions (mitcad#1155): plain functions of the readiness.
target_sources(mitcad PRIVATE agent/AccountCheck.hpp)
# API endpoints and local models in the chat (phase 2 of mitcad#947): the
# Anthropic API, Ollama, LM Studio and a custom OpenAI-compatible endpoint,
# whose tool loop runs in MitCad (core/mcp/src/chat/api.rs) over Qt Network
# and the chat's own connection to the live listener; their keys in the
# keychain (QtKeychain, as files/Keychain.cpp).
target_sources(mitcad PRIVATE agent/ApiAgent.cpp agent/ApiAgent.hpp)
# A custom command over the Agent Client Protocol in the chat (mitcad#1198).
target_sources(mitcad PRIVATE agent/AcpAgent.cpp agent/AcpAgent.hpp)
# Sign in Again (mitcad#1585): the agent's own sign-out and sign-in in a
# terminal window, the terminal and the script chosen by plain functions.
target_sources(mitcad PRIVATE agent/SignInCommands.hpp agent/SignInTerminal.cpp agent/SignInTerminal.hpp)
# The AppKit frontend's Chat card (mitcad#1075): the same panel through the
# presenter bridge, in a card whose host keeps the conversation's end in
# sight.
if(APPLE AND MITCAD_MACOS_NATIVE_UI)
  target_sources(mitcad PRIVATE platform/macos/NativeChatCard.hpp platform/macos/NativeChatCard.mm)
  set_source_files_properties(platform/macos/NativeChatCard.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
endif()
if(BUILD_TESTING)
  # The chat's UI tests' scripted agent (tools/ui-chat-test.py): Claude
  # Code's stream-json (as `claude`), Codex's app server (as `codex`) or the
  # Agent Client Protocol (as `acp-test-agent`, mitcad#1198) on stdin and
  # stdout, the real bridge as its MCP server; no AI. It is never installed.
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
  # An API agent's key across a change in Preferences while a message runs
  # (mitcad#1309): ApiAgent against a local service and a stand-in of the
  # live listener, keys for the run only.
  add_executable(test_api_agent tests/api_agent.cpp agent/ApiAgent.cpp agent/ApiAgent.hpp agent/ChatAgent.hpp
    agent/ChatSettings.cpp agent/ChatSettings.hpp mcp/LocalEndpoint.cpp mcp/LocalEndpoint.hpp)
  if(WIN32)
    target_sources(test_api_agent PRIVATE mcp/WindowsEndpointSecurity.cpp mcp/WindowsEndpointSecurity.hpp)
    target_link_libraries(test_api_agent PRIVATE advapi32)
  endif()
  target_include_directories(test_api_agent PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
  target_link_libraries(test_api_agent PRIVATE mitcad_ffi Qt6::Core Qt6::Network qt6keychain mitcad_warnings)
  mitcad_utf8_code_page(test_api_agent)
  add_test(NAME app.api_agent COMMAND test_api_agent)
  set_tests_properties(app.api_agent PROPERTIES TIMEOUT 120
    ENVIRONMENT "MITCAD_TEST_KEYCHAIN=off"
    ENVIRONMENT_MODIFICATION "PATH=path_list_prepend:$<TARGET_FILE_DIR:Qt6::Core>")
endif()

# A stored view's height set as OCCT's camera scale through the view's
# aspect (mitcad#959): OcctViewer::setCamera restores named views and Home
# at any aspect; the unit tests use OCCT's cameras (TKService).
target_sources(mitcad PRIVATE view/CameraHeight.hpp)
# Create Sketch and the Construct commands ask whether to activate the
# component of a picked face, plane or edge (mitcad#960, app/COMMANDS.md
# "Activating the picked component"): the decision and the question, and
# the main window's side.
target_sources(mitcad PRIVATE
  browser/ActivateQuestion.cpp
  browser/ActivateQuestion.hpp
  MainWindowActivate.cpp)
# A feature that changes the picked body (Fillet, Chamfer, Shell, ...) on
# another component's body activates that component without a question
# (mitcad#1634, app/COMMANDS.md "Activating the picked component"): the
# command table and the decisions, plain Qt Core.
target_sources(mitcad PRIVATE browser/ActivateOnPick.cpp browser/ActivateOnPick.hpp)
# A feature's build state on the timeline and the selection card
# (mitcad#961): the cell's fill, its status ring inside the selection's and
# the status badge that stays in every state; plain Qt Gui.
target_sources(mitcad PRIVATE browser/FeatureStatus.cpp browser/FeatureStatus.hpp)
# The notices' levels and their one presentation per level (mitcad#1040,
# docs/ui-conventions.md "Messages and severity"): word, icon, color role,
# highlight box fill and lifetime, shared by Qt and AppKit; plain Qt Gui.
target_sources(mitcad PRIVATE framework/NoticeLevel.cpp framework/NoticeLevel.hpp)
# The sketch points' markers and the overlay's highlights with the markers over
# them (mitcad#946): plain Qt Gui, painted into images in the test.
target_sources(mitcad PRIVATE sketch/PointMarkers.cpp sketch/PointMarkers.hpp)
# Commands pinned to the toolbar (mitcad#1001, app/COMMANDS.md "Pinned
# commands"): the user's pins per tab, kept in the settings; the Ribbon
# shows each as a button in its own group (mitcad#1084).
target_sources(mitcad PRIVATE framework/ToolbarPins.cpp framework/ToolbarPins.hpp)
# Dogbone in the SOLID tab's MODIFY group (mitcad#1011): relief cuts at a
# body's inside corners.
target_sources(mitcad PRIVATE commands/DogboneCommands.cpp)
# Mortise and Tenon: a tenon on one body's end face and its mortise in
# another body (mitcad#1012).
target_sources(mitcad PRIVATE commands/MortiseTenonCommands.cpp)
# The part picker's stars (mitcad#1014): the tile's empty star on a disc of
# its own over the picture, visible on the selected tile; plain Qt Gui,
# painted into images in the test.
target_sources(mitcad PRIVATE library/PartStar.cpp library/PartStar.hpp)
# The browser's hover (mitcad#1022, app/COMMANDS.md "Browser"): a body's,
# component's or occurrence's row under the pointer highlights its bodies
# in the view, drawn again from their presentations (OcctViewer).
target_sources(mitcad PRIVATE MainWindowBrowserHover.cpp)
# The Sketch Palette's Create section (mitcad#1027, app/COMMANDS.md "The
# Sketch Palette's Create section"): the user's most used sketch tools,
# chosen from the Sketch tab's pins and the tools' starts counted in the
# settings.
target_sources(mitcad PRIVATE sketch/PaletteCreateTools.cpp sketch/PaletteCreateTools.hpp)
# Remembered values of command panels (mitcad#1028, app/COMMANDS.md
# "Remembered values"): a new Fillet or Chamfer starts with the sizes of
# the last OK, kept in the local settings.
target_sources(mitcad PRIVATE framework/RememberedValues.cpp framework/RememberedValues.hpp)
# The Configuration panel (mitcad#1020, app/COMMANDS.md "The left column"):
# the Configuration bar in a left panel of its own between Projects and the
# browser, a Floating card or a Docked dock.
target_sources(mitcad PRIVATE browser/MainWindowConfigurationPanel.cpp)

# Material ▸ in the view's and the Browser's context menus (mitcad#1046,
# app/COMMANDS.md "Material ▸ in the context menus"): the design's
# materials, the library materials assigned last (kept in the settings)
# and More…; a pick assigns as the Materials window's Assign does.
target_sources(mitcad PRIVATE browser/BrowserMaterialMenu.cpp print/MaterialMenuChoices.cpp
               print/MaterialMenuChoices.hpp)
# The Make tab (mitcad#1049, app/COMMANDS.md "The Make tabs"): the Annotate
# group's panels, Assign Material…, Export Face as DXF…, the selected
# body's card with its material and print settings, and the Browser's
# markers of the bodies that differ from their component.
target_sources(mitcad PRIVATE MainWindowMake.cpp print/MakeBodyCard.cpp print/MakeBodyCard.hpp)

# The Material and Appearance tools (mitcad#1059, app/COMMANDS.md "Material
# and Appearance tools"): pick it with the picker button, pick the bodies,
# OK; the fx binds it to an option. The Materials and Appearances windows
# only manage the libraries.
target_sources(mitcad PRIVATE commands/MaterialCommands.cpp commands/MaterialBinding.cpp
               commands/MaterialBinding.hpp browser/LibraryPicker.cpp browser/LibraryPicker.hpp)

# The left column (mitcad#1020): Projects, Configuration and Browser share
# its height (framework/ColumnShare.hpp, also FloatingLayout's); in the
# AppKit frontend the Configuration card and the dragged boundaries.
target_sources(mitcad PRIVATE framework/ColumnShare.hpp)
if(APPLE AND MITCAD_MACOS_NATIVE_UI)
  target_sources(mitcad PRIVATE platform/macos/NativeColumnSplit.hpp platform/macos/NativeColumnSplit.mm)
  set_source_files_properties(platform/macos/NativeColumnSplit.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
endif()

# The navigation bar at the bottom of the view (mitcad#1086, app/COMMANDS.md
# "The view"): Look At, Fit, Visual Style, Layout Grid and Snap to Grid, the
# View menu's own actions; a card in the Floating layout, in the status bar
# in the Docked layout.
target_sources(mitcad PRIVATE view/NavigationBar.cpp view/NavigationBar.hpp MainWindowNavigationBar.cpp)

# The message band and the session's message history (mitcad#1042,
# docs/ui-conventions.md "The message band"): what the band shows of the
# status line's messages and for how long, its stub and the history's
# panel; the Docked status bar's button opens the same history.
target_sources(mitcad PRIVATE framework/MessageLog.cpp framework/MessageLog.hpp framework/MessageBand.cpp
               framework/MessageBand.hpp)
# The AppKit frontend's band above the timeline and its history (mitcad#1098),
# drawing the same MessageLog.
if(APPLE AND MITCAD_MACOS_NATIVE_UI)
  target_sources(mitcad PRIVATE platform/macos/NativeMessageBand.hpp platform/macos/NativeMessageBand.mm)
  set_source_files_properties(platform/macos/NativeMessageBand.mm PROPERTIES COMPILE_OPTIONS -fobjc-arc)
endif()

# Convert Mesh's panel in the SOLID tab's MODIFY group (mitcad#1082): a mesh
# body made a faceted or an analytic solid.
target_sources(mitcad PRIVATE commands/ConvertMeshCommands.cpp)
# Mesh tools for reverse engineering (mitcad#1871).
target_sources(mitcad PRIVATE commands/MeshToolsCommands.cpp)

# File › Print… (mitcad#1089, app/COMMANDS.md "Print"): a drawing's sheets
# each on a page of its size, a sketch or a cut sheet at 1:1 tiled over
# pages or fitted to one, the BUILD lists as their PDF lays them out;
# QPrinter and the system's print dialog of Qt PrintSupport.
target_sources(mitcad PRIVATE
  printing/PaperPrint.cpp
  printing/PaperPrint.hpp
  printing/SketchPicture.cpp
  printing/SketchPicture.hpp
  printing/PrintPanel.cpp
  printing/PrintPanel.hpp
  printing/MainWindowPaperPrint.cpp)
target_link_libraries(mitcad PRIVATE Qt6::PrintSupport)

# The Center of Mass marker (mitcad#1094, app/COMMANDS.md "Center of
# Mass"): Center of Mass in the Inspect group, the browser's row under
# Origin, and the marker drawn in the view at the design's center of mass.
target_sources(mitcad PRIVATE MainWindowCenterOfMass.cpp view/CenterOfMass.cpp view/CenterOfMass.hpp
               view/CenterOfMassView.cpp)

# The quick-access group (mitcad#1090, app/COMMANDS.md "The quick-access
# group"): the commands the user chose from the File, Edit, View and Tools
# menus, in the Floating title row and the Docked toolbar; the File menu's
# Open, Close, Save, Save As and Exit as registry commands.
target_sources(mitcad PRIVATE framework/QuickAccess.cpp framework/QuickAccess.hpp
               framework/QuickAccessGroup.cpp framework/QuickAccessGroup.hpp
               files/FileCommands.cpp files/FileCommands.hpp)

# Send to Slicer's Printer and Process picker (mitcad#1108, app/COMMANDS.md
# "Send to Slicer"): the printers grouped by vendor and the processes by
# their printer, short names and the source of each row, in the panel's
# searchable picker (browser/LibraryPicker).
target_sources(mitcad PRIVATE print/ProfilePickerRows.cpp print/ProfilePickerRows.hpp)

# Comments for AI agents (mitcad#1119, app/COMMANDS.md "Comments"): Add
# Comment, the numbered pins over the view, the Comments panel and the
# Agents may resolve comments preference.
target_sources(mitcad PRIVATE MainWindowComments.cpp view/Comments.cpp view/Comments.hpp
               view/CommentPinOverlay.cpp view/CommentPinOverlay.hpp view/CommentsWindow.cpp
               view/CommentsWindow.hpp)

# Images on comments and replies (mitcad#2011, app/COMMANDS.md "Comments"):
# Attach Image…, Capture and Annotate's markup editor over the view, the
# thumbnails' viewer.
target_sources(mitcad PRIVATE view/CommentImages.cpp view/CommentImages.hpp view/CommentMarkup.cpp
               view/CommentMarkup.hpp)

# Send to Slicer's material rows (mitcad#1141, app/COMMANDS.md "Send to
# Slicer"): a row per filament, each a searchable material picker with the
# FDM plastics first, grouped by where the materials live.
target_sources(mitcad PRIVATE print/SlicerMaterialRows.cpp print/SlicerMaterialRows.hpp)

# Save Component as Design's guard (mitcad#1148): the file chosen is not the
# open design's own or a linked component's, however its path is spelled.
target_sources(mitcad PRIVATE files/SaveComponentTarget.cpp files/SaveComponentTarget.hpp)

# Lengths in the design's unit (mitcad#1165, app/COMMANDS.md "Length
# defaults in the design's unit"): commands' length defaults as a sensible
# value of the unit, the Browser's sheet-rules row and Measure's values.
target_sources(mitcad PRIVATE framework/LengthUnits.cpp framework/LengthUnits.hpp)

# Multiple documents in one process, presented as windows or document tabs
# (mitcad#1172). Each MainWindow retains its own DocumentSession.
target_sources(mitcad PRIVATE WindowManager.cpp WindowManager.hpp DocumentFrame.cpp DocumentFrame.hpp ViewerVisibility.cpp
               framework/TestDriverDocuments.cpp)
if(BUILD_TESTING)
  # Autosave's destructor marks the shutdown's steps (mitcad#2275).
  target_sources(test_app PRIVATE files/Autosave.cpp framework/AppSettings.cpp framework/ShutdownSteps.cpp
                 framework/ShutdownSteps.hpp)
endif()

# Insert another open document through the shared saved-design insertion workflow (mitcad#1172).
target_sources(mitcad PRIVATE files/MainWindowOpenDesign.cpp)

# Add Print Value… on the Make tab's body card (mitcad#1178, app/COMMANDS.md
# "The selected body's card"): the settings a body can take as its own,
# grouped and named as the Properties window shows them, in a searchable
# picker.
target_sources(mitcad PRIVATE print/PrintValueChoices.cpp print/PrintValueChoices.hpp print/PrintValuePicker.cpp
               print/PrintValuePicker.hpp)

# ui_screenshot's encoded-image budget (mitcad#1188): max_bytes, the
# bounded downscaling and the encodings.
target_sources(mitcad PRIVATE mcp/ScreenshotBudget.cpp mcp/ScreenshotBudget.hpp)

# Materials' mechanical, thermal and printing values in the design's units,
# the bundled material dataset's browser (mitcad#1213): the Materials
# window's General tab and New ▾ › Browse Material Dataset….
target_sources(mitcad PRIVATE print/MaterialProperties.cpp print/MaterialProperties.hpp)

# The layout grid in the design's length unit (mitcad#1210, app/COMMANDS.md
# "Layout grid"): the automatic spacing's metric and inch series, the major
# lines, Grid Settings' fixed spacing converted to and from mm.
target_sources(mitcad PRIVATE view/GridSpacing.cpp view/GridSpacing.hpp)

# The arrow keys turn the 3D view by Preferences › Navigation's step while
# it has the keyboard (mitcad#1211, app/COMMANDS.md "Arrow keys"): the
# camera's turn about the mouse orbit's centre.
target_sources(mitcad PRIVATE view/CameraTurn.hpp)

# Up and Down and the mouse wheel step a value field's number (mitcad#1254,
# app/COMMANDS.md "Stepping a value"): the command panel's value fields and
# the sketch's value fields, through framework/ParameterField.cpp.
target_sources(mitcad PRIVATE framework/ValueStep.cpp framework/ValueStep.hpp)

# The Start tab (mitcad#1258, GitHub mitcad/Mitcad#125, app/COMMANDS.md "The
# Start tab"): the ribbon's START tab shows the Start page over the 3D view:
# quick actions, the recent designs with the previews a save writes, their
# pins and search, the resources, Show start page at launch.
target_sources(mitcad PRIVATE MainWindowStart.cpp files/RecentDesigns.cpp files/RecentDesigns.hpp
               files/StartPage.cpp files/StartPage.hpp)

# ui_command's listing of the commands an agent may run and the nearest ids
# to an unknown one (mitcad#1269, docs/mcp.md): the command registry as
# live MCP shows it.
target_sources(mitcad PRIVATE mcp/UiCommandList.cpp mcp/UiCommandList.hpp)

# Envelope's panel in the SOLID tab's CREATE menu (mitcad#1276): a light
# outer skin of a component, an occurrence or bodies.
target_sources(mitcad PRIVATE commands/EnvelopeCommands.cpp)

# An orbit goes on past the edge of the screen (mitcad#1281, GitHub
# mitcad/Mitcad#139): the cursor jumps to the opposite edge, which the view
# does not see (OcctViewer::wrapOrbitCursor).
target_sources(mitcad PRIVATE view/OrbitWrap.hpp)

# The Start page's project browser (mitcad#1301, app/COMMANDS.md "The Start
# tab"): Recent, Pinned and the projects on the left, a project's breadcrumb,
# folders and design cards on the right, one search over every project and
# the recent list, the row and folder remembered.
target_sources(mitcad PRIVATE files/StartBrowser.cpp files/StartBrowser.hpp)

# The places of the model's notices (mitcad#1327, docs/ui-conventions.md
# "Messages and severity"): painted in the view while the pointer is over a
# notice (the command panel's message, a Timeline item, a Browser row, the
# message band, the Docked status line).
target_sources(mitcad PRIVATE framework/NoticePlaces.cpp framework/NoticePlaces.hpp MainWindowNoticeHover.cpp)

# Preferences › Display › Always show corners in drawings and its dot size
# (mitcad#1338, app/COMMANDS.md "Drawings"): every corner of an open
# drawing's views as a dot on the screen.
target_sources(mitcad PRIVATE drawings/DrawingPreferences.cpp drawings/DrawingPreferences.hpp)

# Preferences › Display › Small circles (mitcad#879, app/COMMANDS.md
# "Small circles"): the view's small circles meshed by their size
# (Adaptive) or with the view's angle (Full detail); a change re-meshes the
# open designs' shown bodies.
target_sources(mitcad PRIVATE view/SmallCirclesPreference.cpp view/SmallCirclesPreference.hpp
                              MainWindowDisplayMeshes.cpp)

# A read of the model while a job computes it, out of an event handler
# (mitcad#1360, GitHub mitcad/Mitcad#156, app/COMMANDS.md "Background
# computation"): the application's notify() logs the ModelBusy and drops
# the event instead of ending the application.
target_sources(mitcad PRIVATE framework/Application.cpp framework/Application.hpp framework/ModelBusy.hpp)

# The layout and pictures of the Preferences pages whose list takes the
# page's height, for the UI tests (mitcad#1394).
target_sources(mitcad PRIVATE framework/PreferencesPageLayout.hpp)

# The arcs the sketch Line tool draws by a drag from a curve's end
# (mitcad#1393, app/COMMANDS.md "The line tool's arcs and Midpoint Line"):
# tangent or normal by the drag, placed by the cursor and the typed radius
# and sweep.
target_sources(mitcad PRIVATE sketch/LineArcs.cpp sketch/LineArcs.hpp)

# Linked designs while their holder is open (mitcad#1416, app/COMMANDS.md
# "Linked designs"): Open Linked Design, Update Link and Update All Links,
# the notice of an edit refused on a linked component, and the message
# band's "<file> changed" with Update when a linked file changes on disk or
# is saved in another tab.
target_sources(mitcad PRIVATE files/MainWindowLinks.cpp files/LinkedFiles.cpp files/LinkedFiles.hpp
                              files/LinkWatcher.cpp files/LinkWatcher.hpp)

# Ctrl (Cmd on macOS) held alone hides a feature command's preview while its
# inputs are picked, and its release shows it again (mitcad#1421,
# app/COMMANDS.md "Holding Ctrl hides the preview").
target_sources(mitcad PRIVATE framework/CommandSessionHold.cpp framework/PreviewHold.cpp framework/PreviewHold.hpp)

# The step a dragged arrow handle rounds its value to (mitcad#1431,
# app/COMMANDS.md "withManipulator"): the zoom's step, or a finer one from
# the size of the geometry the handle works on; Place's click rounds by the
# same rule.
target_sources(mitcad PRIVATE framework/HandleStep.cpp framework/HandleStep.hpp)

# The Docked layout's docks hidden from their close button or the window's
# menu of its panels and shown again from the View menu (mitcad#1455): the
# test steps dock-title-context, expect-popup-entries, dock-close and
# expect-dock.
target_sources(mitcad PRIVATE framework/TestDriverDocks.cpp)

# The interface walked in another language (mitcad#1473): the test steps
# walk-commands, walk-menus and walk-context-menus.
target_sources(mitcad PRIVATE framework/TestDriverWalk.cpp)

# A pick on a feature's preview names the face, edge or vertex of the body
# before the feature that a changed one comes from; one the preview made is
# refused with a hint that Ctrl held shows the part's own (mitcad#1458,
# app/COMMANDS.md "Picks on the preview").
target_sources(mitcad PRIVATE framework/CommandSessionPreviewPicks.cpp)

# Preferences › Display › Interface size (mitcad#1495, GitHub
# mitcad/Mitcad#194, app/COMMANDS.md "Interface size"): the whole UI at
# 100 to 200 %, Qt's scale factor set before the application object at the
# next start; the test step expect-interface-size (TestDriverChrome.cpp).
target_sources(mitcad PRIVATE framework/InterfaceSize.cpp framework/InterfaceSize.hpp)

# Preferences › Display › Text size, Icon size and Font (mitcad#1556, GitHub
# mitcad/Mitcad#207, app/COMMANDS.md "Text size, Icon size and Font"): the
# application font at 80 to 200 % and in a chosen family, the fixed icon
# sizes through uiIconSize, at the next start; the test step
# expect-text-icon-size (TestDriverChrome.cpp).
target_sources(mitcad PRIVATE framework/UiSizes.cpp framework/UiSizes.hpp)
# Icon size for the icons Qt's style sizes (mitcad#1561): the application's
# style inside IconSizeStyle, a proxy that multiplies the style's icon pixel
# metrics; WindowStyle's own style and the macOS ones derive from it.
target_sources(mitcad PRIVATE framework/IconSizeStyle.cpp framework/IconSizeStyle.hpp)
# 3D sketches (mitcad#1524, app/COMMANDS.md "3D sketches"): Create
# Sketch's 2D or 3D choice, the 3D sketch's editing context (its frame,
# points, curves, references and badges in the view, the 3D Sketch
# Palette) and the 2D tools disabled in it.
target_sources(mitcad PRIVATE
  MainWindowSketch3d.cpp
  sketch/Sketch3dController.cpp
  sketch/Sketch3dController.hpp
  sketch/Sketch3dModel.cpp
  sketch/Sketch3dModel.hpp
  sketch/Sketch3dPalette.cpp
  sketch/Sketch3dPalette.hpp)
# The 3D sketch's drawing tools (mitcad#1525, app/COMMANDS.md "3D sketch
# drawing tools"): placement on the temporary planes, axes and normal,
# snaps, typed values, arcs and splines, committed as sketch3d commands.
target_sources(mitcad PRIVATE
  sketch/Sketch3dDraw.cpp
  sketch/Sketch3dDraw.hpp
  sketch/Sketch3dSnaps.cpp
  sketch/Sketch3dTools.cpp)
# The plane-bound drawing tools (mitcad#1550): circles, the ellipse,
# rectangles, polygons and the text on the active XY, YZ or ZX plane.
target_sources(mitcad PRIVATE sketch/Sketch3dShapes.cpp)
# Editing a 3D sketch (mitcad#1526, app/COMMANDS.md "3D sketch editing"):
# relations and dimensions from the selection and their palette list,
# Fix, Unfix, Detach, Attach and Replace Reference, dragging points,
# curves and tangent handles, Delete with Dependents.
target_sources(mitcad PRIVATE
  sketch/Sketch3dEdit.cpp
  sketch/Sketch3dEdit.hpp
  sketch/Sketch3dEditing.cpp)
# A 3D sketch's Fillet (mitcad#1548, app/COMMANDS.md "3D sketch Fillet"):
# corners picked by their point or two lines, a typed or dragged radius
# previewed, each corner one sketch3d.fillet.
target_sources(mitcad PRIVATE
  sketch/Sketch3dFillet.cpp
  sketch/Sketch3dFillet.hpp
  sketch/Sketch3dFilletTool.cpp)
# Pipe and Sweep along 3D sketches (mitcad#1527, app/COMMANDS.md "Pipe and
# Sweep along 3D sketches"): the typed path, the kept direction, path
# pieces, a whole-sketch pick's route and the panel's route listing.
target_sources(mitcad PRIVATE
  commands/SpatialPaths.cpp
  commands/SpatialPaths.hpp)

# A sketch edit refused for conflicting constraints (mitcad#1566,
# app/COMMANDS.md "Conflicting constraints"): the message shown without the
# model's ids line, the constraints and dimensions that block the change
# painted in the view while it shows.
target_sources(mitcad PRIVATE sketch/SketchConflict.cpp sketch/SketchConflict.hpp)

# Commands pinned to the navigation bar (mitcad#1589, app/COMMANDS.md
# "Pinned to the navigation bar"): the user's pins for every workspace, kept
# in the settings, Edit Parameters by default; the navigation bar shows each
# after a separator at its end.
target_sources(mitcad PRIVATE framework/NavigationBarPins.cpp framework/NavigationBarPins.hpp)

# Where the sketch overlay's constraint symbols go (mitcad#1594): a row of
# symbols moves clear of the dimensions' texts, the rows before it and the
# curves, so that a symbol never covers a curve and takes its clicks.
target_sources(mitcad PRIVATE sketch/GlyphRows.cpp sketch/GlyphRows.hpp)

# The hidden menu bar and Enter's repeat (mitcad#1620, app/COMMANDS.md "The
# hidden menu bar", "Enter repeats the last command"): Preferences › Display
# › Show the menu bar, the ☰ button before Undo and Redo, Alt showing the
# bar for its menus; the widgets that take Enter themselves.
target_sources(mitcad PRIVATE framework/MenuBarReveal.cpp framework/MenuBarReveal.hpp framework/EnterRepeat.cpp
               framework/EnterRepeat.hpp)

# The BUILD lists' tables as CSV and as an aligned plain-text table, with
# CRLF line ends (mitcad#1641, app/COMMANDS.md "BUILD"): Copy as Text and
# Save as Text… beside the CSV and PDF buttons.
target_sources(mitcad PRIVATE build/ListTables.cpp build/ListTables.hpp)
# Lists as pages (mitcad#1089, mitcad#2095): the BUILD lists and the
# Keyboard and Mouse Overview's cheat sheet as PDF and print.
target_sources(mitcad PRIVATE build/ListPages.cpp build/ListPages.hpp)

# The project picker's New Folder (mitcad#1649, app/COMMANDS.md "Project
# picker"): a folder made in the chosen project, named in place.
target_sources(mitcad PRIVATE projects/PickerNewFolder.cpp projects/PickerNewFolder.hpp)

# The Start page's rounded backdrop (mitcad#1439, app/COMMANDS.md "The Start
# tab"): inset from the view's edges with the floating cards' radius, the
# view's background around it.
target_sources(mitcad PRIVATE files/StartBackdrop.cpp files/StartBackdrop.hpp)

# The command search's ranking (mitcad#1681, framework/CommandSearchRank):
# a sketch tool that matches best outside a sketch brings a row saying how
# to open one, and no weaker match is selected.
target_sources(mitcad PRIVATE framework/CommandSearchRank.cpp framework/CommandSearchRank.hpp)

# Sketch mode's list of the sketch's points, curves, constraints and
# dimensions in creation order, in the timeline's place (mitcad#1492).
target_sources(mitcad PRIVATE
  sketch/SketchItemList.cpp
  sketch/SketchItemList.hpp)
# The default 3D view (mitcad#1698, app/COMMANDS.md "A new design shows
# its origin, seen from the default view"): Home and a new design
# (mitcad#1809) look a few degrees off the isometric view, so that
# the origin planes' edges do not lie on top of each other.
target_sources(mitcad PRIVATE view/DefaultView.hpp)

# The 3D view's pick list (mitcad#1703, app/COMMANDS.md "Selection"): a
# press held still lists everything under the pointer, nearest first.
target_sources(mitcad PRIVATE view/PickList.cpp view/PickList.hpp view/PickListView.cpp)
# The design window's size, place and state kept between starts (mitcad#1710,
# GitHub mitcad/Mitcad#283, app/COMMANDS.md "The window's size and place"):
# saved when a design window closes and at quit, restored for the first
# window, fitted to the connected screens; a new window at the last size,
# cascaded (WindowManager::createWindow); the test steps expect-window-place
# and expect-window-on-screen (TestDriverWindowFit.cpp).
target_sources(mitcad PRIVATE framework/WindowPlace.cpp framework/WindowPlace.hpp)

# Flex (mitcad#1713, app/COMMANDS.md "Flex"): the command that lays a wire,
# cable, hose or strip through the model by clicks, and the route it keeps
# while it is laid (nodes, end ports, the item of the panel); its unit
# tests in tests/unit/flex_route.cmake.
target_sources(mitcad PRIVATE commands/FlexCommands.cpp commands/FlexRoute.cpp commands/FlexRoute.hpp)

# Editing a Flex item directly in the 3D view (mitcad#1713, app/COMMANDS.md
# "Flex editing"): a selected item's nodes as handles, dragged along and
# between faces, onto end ports and into space with a live preview, nodes
# added at the ends and between, deleted, a flat section's twist ring.
target_sources(mitcad PRIVATE
  MainWindowFlexEdit.cpp
  flex/FlexEdit.cpp
  flex/FlexEdit.hpp
  flex/FlexEditController.cpp
  flex/FlexEditController.hpp
  flex/FlexRope.cpp
  flex/FlexRope.hpp)

# Let It Hang (mitcad#1713, app/COMMANDS.md "Hanging cables"): a panel with
# the design's direction of gravity, the length, the damping and the end
# pins, and a Flex item let hang in the view, its nodes pinned or let go,
# dragged by any point, settling until it rests and kept there on OK.
target_sources(mitcad PRIVATE
  MainWindowGravity.cpp
  commands/GravityCommands.cpp
  gravity/GravityController.cpp
  gravity/GravityController.hpp
  gravity/GravityLogic.cpp
  gravity/GravityLogic.hpp)

# CAM (mitcad#1013, app/COMMANDS.md "The CNC group"): the Make tab's CNC
# group (New Setup, the operations' panels, Generate, Simulate, Post
# Process…, Machines and Tools…), the Browser's CAM folder and the
# toolpaths in the 3D view.
target_sources(mitcad PRIVATE
  cam/CamMachinesDialog.cpp
  cam/CamMachinesDialog.hpp
  cam/CamSimulateDialog.cpp
  cam/CamSimulateDialog.hpp
  cam/CamToolpaths.cpp
  cam/CamToolpaths.hpp
  cam/MainWindowCam.cpp
  view/CamToolpathOverlay.cpp)
# Simulate's stock (mitcad#1013, phase 3): the height grid of cam_simulate
# as a surface shaded by the material left, and its overlay in the view.
target_sources(mitcad PRIVATE
  cam/CamStock.cpp
  cam/CamStock.hpp
  cam/MainWindowCamStock.cpp
  cam/MainWindowCamTurning.cpp
  view/CamStockOverlay.cpp)
# Live MCP's input tools (mitcad#1735, docs/mcp.md "Clicks, keys and
# dialogs"): ui_click and ui_type post mouse and key events to the window,
# ui_dialog opens a window ui_command refuses for viewing, reads it and
# closes it; only in a run started with --mcp-folder.
target_sources(mitcad PRIVATE MainWindowMcpInput.cpp mcp/DialogWatch.cpp mcp/DialogWatch.hpp
               mcp/UiKeys.cpp mcp/UiKeys.hpp)
# Preferences › Display › Origin planes and the resize handles of
# construction planes and axes (mitcad#1748, GitHub mitcad/Mitcad#273 and
# #279, app/COMMANDS.md "Origin planes and the size of construction
# geometry"): the origin at a size on the screen or in millimetres, in the
# Corner or Centred style; a selected construction plane's or axis's
# handles set the size the model keeps with it (set_datum_display_size).
target_sources(mitcad PRIVATE view/OriginPreference.cpp view/OriginPreference.hpp view/DatumHandles.cpp
                              view/DatumHandles.hpp MainWindowOrigin.cpp)
# Show Dimensions of a sketch's row (mitcad#1772, GitHub mitcad/Mitcad#306,
# app/COMMANDS.md "Show Dimensions outside sketch mode"): a shown sketch's
# dimensions drawn over the view outside sketch mode, as sketch mode draws
# them.
target_sources(mitcad PRIVATE sketch/ShownDimensions.cpp sketch/ShownDimensions.hpp)
# Forms (mitcad#1757, app/COMMANDS.md "Form"): Form in the SOLID tab's
# CREATE menu, a body shaped by an editable control cage; Edit Form, the
# panel a Form's double-click opens, with its cage drawn and picked over
# the view.
target_sources(mitcad PRIVATE commands/FormCommands.cpp commands/FormCage.cpp commands/FormCage.hpp
                              view/FormCageOverlay.cpp view/FormCageOverlay.hpp MainWindowForm.cpp)
# The orbit's center (mitcad#1791, GitHub mitcad/Mitcad#300,
# app/COMMANDS.md "The orbit's center"): Preferences › Navigation's Orbit
# center (Model or Cursor) and a fixed center set from the view's context
# menu, with its marker and its test steps.
target_sources(mitcad PRIVATE view/OrbitCenterView.cpp framework/TestDriverOrbitCenter.cpp)
# Preferences › Display › Selection color and Selected line width
# (mitcad#1803, GitHub mitcad/Mitcad#309, app/COMMANDS.md "Selection color
# and line width"): the colour the view and sketch mode draw the selection
# in, and the selection's and the hover's line widths, scaled by the
# device pixel ratio.
target_sources(mitcad PRIVATE view/SelectionLook.cpp view/SelectionLook.hpp)
# The Properties window (mitcad#1816, app/COMMANDS.md "Physical
# Properties"): several bodies as a summary and a table with equal bodies
# merged, one body's values in three groups, even units and Copy's text.
target_sources(mitcad PRIVATE commands/PropertiesReport.cpp commands/PropertiesReport.hpp)
# Fill Cavity in the SOLID tab's MODIFY group (mitcad#1818): a closed pocket
# of a body filled up to a plane.
target_sources(mitcad PRIVATE commands/FillCavityCommands.cpp)
# Sketches on cylindrical and conical faces (mitcad#1768, app/COMMANDS.md
# "Sketches on curved faces"): Create Sketch and Redefine Sketch Plane on
# such a face, the sketch's points, curves, previews and profiles drawn on
# the face by the model's map, and the pointer met with the face through
# the `sketch_surface` query's rays.
target_sources(mitcad PRIVATE sketch/SketchSurface.cpp sketch/SketchSurface.hpp)
# The Docked layout's Command panel's width (mitcad#1825, GitHub
# mitcad/Mitcad#336, app/COMMANDS.md "The Command panel's width"): at least
# 300 px times the larger of Text size and Icon size, dragged wider by its
# edge and remembered (commandPanel/dockWidth).
target_sources(mitcad PRIVATE framework/CommandDockWidth.cpp framework/CommandDockWidth.hpp)
# BUILD parts deleted as members (mitcad#1811, mitcad#1820, mitcad#1843):
# the Browser's Delete and Delete Component share the build_remove command
# list, which leaves a fastener to its connector when both go.
target_sources(mitcad PRIVATE browser/BuildMembers.cpp browser/BuildMembers.hpp)
# Joint, As-Built Joint, Rigid Group and Constraint Set on picks under
# another component than the active one (mitcad#1856, app/COMMANDS.md "The
# component that holds the picks"): the question that offers the component
# holding every pick, its preference, and the main window's side.
target_sources(mitcad PRIVATE browser/JointHolderQuestion.cpp browser/JointHolderQuestion.hpp
               MainWindowJointHolder.cpp)
# Edit Parameters sorted by a column (mitcad#1866, GitHub mitcad/Mitcad#338):
# the rows' keys (numbers in the common unit of their kind, texts in
# natural order, boxes on or off) and their order within a group.
target_sources(mitcad PRIVATE browser/ParameterSort.cpp browser/ParameterSort.hpp)
# Preferences › Export Settings and Import Settings (mitcad#1889, GitHub
# mitcad/Mitcad#357, app/COMMANDS.md "Exporting and importing settings"):
# the groups of settings in a `.mitcad-settings` file, and the dialogs,
# the question and the import applied as Preferences applies a change.
target_sources(mitcad PRIVATE framework/SettingsTransfer.cpp framework/SettingsTransfer.hpp
               view/PreferencesTransfer.cpp)

# One update for the whole application (mitcad#1890): the update's row,
# which every main window's bar shows; a window that closes takes only its
# own bar along.
target_sources(mitcad PRIVATE update/UpdateRow.cpp update/UpdateRow.hpp)

# Version work off the UI thread (mitcad#1895, app/COMMANDS.md "Remote
# repositories"): one queue per project folder for the tasks that reach its
# server, local version commands run aside, the sync state in words.
target_sources(mitcad PRIVATE files/VersionQueue.cpp files/VersionQueue.hpp files/SyncState.cpp
               files/SyncState.hpp)

# Every session's log on disk with its rotation, the hang marker and the
# crash dumps beside the logs (mitcad#1896).
target_sources(mitcad PRIVATE
  framework/HangWatchdog.cpp
  framework/HangWatchdog.hpp
  framework/SessionLog.cpp
  framework/SessionLog.hpp)

# The shutdown's steps in the session log, timed by the hang marker
# (mitcad#2275, GitHub mitcad/Mitcad#573); the web browser module reaches
# them through the header-only ShutdownModuleStep.hpp.
target_sources(mitcad PRIVATE framework/ShutdownSteps.cpp framework/ShutdownSteps.hpp
               framework/ShutdownModuleStep.hpp)

# The busy mark beside a command panel's OK and Cancel while its preview
# computes (mitcad#1907, GitHub mitcad/Mitcad#361), in place of a message
# line that resized the panel.
target_sources(mitcad PRIVATE framework/BusyMark.cpp framework/BusyMark.hpp)

# The unit Number of a parameter without a unit (mitcad#1920, GitHub
# mitcad/Mitcad#378): Add User Parameter's Unit lists and the parameters
# table's Unit column.
target_sources(mitcad PRIVATE framework/ParameterUnits.cpp framework/ParameterUnits.hpp)

# Preferences › Display › Toolbar labels (mitcad#1936, GitHub
# mitcad/Mitcad#356): the ribbon's buttons with a short label under each
# icon, the setting and its notifier.
target_sources(mitcad PRIVATE framework/RibbonLabels.cpp framework/RibbonLabels.hpp)

# The command layout (mitcad#1951, app/COMMANDS.md "The layout file"): the
# toolbar's tabs and groups and the menu bar's menus as one JSON file, read
# once at the start, with the user's layer over it; plain Qt Core.
target_sources(mitcad PRIVATE framework/CommandLayout.cpp framework/CommandLayout.hpp)
qt_add_resources(mitcad command_layout PREFIX /layout BASE resources/layout FILES resources/layout/default.json)

# Sketch curves picked in the Browser (mitcad#1978): a sketch's curves as
# rows under its row while a command's active input takes sketch curves,
# named in words, the pointer over one painting it in the view.
target_sources(mitcad PRIVATE MainWindowBrowserCurves.cpp browser/SketchCurveRows.cpp browser/SketchCurveRows.hpp)

# The Boost development button (mitcad#1990, app/COMMANDS.md "Boost
# development"): a pill in the Floating top bar and the Docked tabs' row
# that opens the project's Ko-fi page, its × and Preferences' setting.
target_sources(mitcad PRIVATE framework/BoostButton.cpp framework/BoostButton.hpp)

# The Drawing workspace's Drawings card (mitcad#1990, app/COMMANDS.md "The
# Drawings card"): the design's drawings and their sheets in place of the
# Browser's card while a drawing is open (Floating).
target_sources(mitcad PRIVATE drawings/DrawingsPanel.cpp drawings/DrawingsPanel.hpp)

# Sheet parts by material and process (mitcad#1939): the process profile's
# command filter, Finger Joint, T-Nut Slot, Kerf Bend, Flat Pack, Glue Flap
# and Standard Box.
target_sources(mitcad PRIVATE commands/SheetProcessCommands.cpp commands/SheetProcess.hpp)

# A sketch text wrapped in its frame and typed in lines (mitcad#2029,
# GitHub mitcad/Mitcad#453): the Text tool's frame, Edit Text's Wrap in
# Frame and the line breaks of its Text box.
target_sources(mitcad PRIVATE sketch/TextCommands.cpp sketch/TextCommands.hpp)

# app.unit's test files (mitcad#1925): each tests/<file>.cpp registers its
# entry point (tests/UnitTests.hpp) and its sources in tests/unit/<file>.cmake,
# included here, after every variable they may use, in sorted order: a new
# test file edits no shared list.
if(BUILD_TESTING)
  file(GLOB mitcad_unit_test_files CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/tests/unit/*.cmake")
  list(SORT mitcad_unit_test_files)
  foreach(mitcad_unit_test_file IN LISTS mitcad_unit_test_files)
    include("${mitcad_unit_test_file}")
  endforeach()
endif()

# Panelize in the SOLID tab's MODIFY group (mitcad#2005, app/COMMANDS.md
# "Panelize"): a body's planar faces split off as panels of one thickness.
target_sources(mitcad PRIVATE commands/PanelizeCommands.cpp)

# A selection input's picks listed by name on demand, each highlighted in
# the view on hover (mitcad#2064, GitHub mitcad/Mitcad#473): the lists'
# open state, the picks' words and the hovered row.
target_sources(mitcad PRIVATE framework/InputPicks.cpp framework/InputPicks.hpp)

# The Docked layout's docks kept between starts (mitcad#2070, GitHub
# mitcad/Mitcad#451, app/COMMANDS.md "The window's size and place"): their
# arrangement and the floating docks' frames saved at quit, restored in the
# first window (a floating dock whose screen is gone back in its dock), a
# new window's floating docks docked, Restore Default Layout; the test steps
# dock-float, dock-area and expect-dock-area (TestDriverDocks.cpp).
target_sources(mitcad PRIVATE MainWindowDocks.cpp framework/DockState.cpp framework/DockState.hpp)

# Preferences' search (mitcad#2074, GitHub mitcad/Mitcad#450): the field
# above the page list, the index made from the pages' widgets, the counts
# and the matches painted.
target_sources(mitcad PRIVATE framework/PreferencesSearch.cpp framework/PreferencesSearch.hpp)

# The sketch's value fields over the view (mitcad#2086, GitHub
# mitcad/Mitcad#485): their style sheet, selected text in the accent with
# AccentText, read by the unit test sketch_field_style.
target_sources(mitcad PRIVATE sketch/SketchFieldStyle.cpp sketch/SketchFieldStyle.hpp)

# A sketch constraint or dimension refused for a conflict with one blocking
# item (mitcad#2090, GitHub mitcad/Mitcad#475, app/COMMANDS.md "Conflicting
# constraints"): the message's Replace button, which swaps that item out in
# one undo step while the sketch is as it was refused.
target_sources(mitcad PRIVATE sketch/SketchReplace.cpp)

# Linux natively on Wayland (mitcad#2031, GitHub mitcad/Mitcad#434): Qt's
# platform chosen at start (Wayland in a Wayland session, X11 elsewhere,
# Qt's EGL integration on X11) and the start-up's Display platform: line.
# OCCT's port draws through EGL on Linux (third_party/vcpkg-ports, its
# config sets OpenCASCADE_WITH_EGL): the 3D view then attaches to Qt's EGL
# context and surface (MITCAD_OCCT_EGL, OcctViewer.cpp). An OCCT built for
# GLX keeps the hidden GLX window and X11 everywhere.
target_sources(mitcad PRIVATE framework/DisplayPlatform.cpp framework/DisplayPlatform.hpp)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND OpenCASCADE_WITH_EGL)
  find_package(OpenGL REQUIRED COMPONENTS EGL)
  target_compile_definitions(mitcad PRIVATE MITCAD_OCCT_EGL)
  target_link_libraries(mitcad PRIVATE OpenGL::EGL)
endif()

# The Rib's and Web's Draft Angle (mitcad#2102, GitHub mitcad/Mitcad#488):
# its input right after Thickness and `draft_angle` written only when not 0.
target_sources(mitcad PRIVATE commands/RibDraft.cpp commands/RibDraft.hpp)

# Folders of the Browser's component and body rows (mitcad#2103, GitHub
# mitcad/Mitcad#491, app/COMMANDS.md "Folders in the Browser"): the
# occurrences of a component by folder, New Folder's name and the folder
# commands the row menus and drops run.
target_sources(mitcad PRIVATE browser/BrowserOrder.cpp browser/BrowserOrder.hpp)

# The Discord button (mitcad#2123, app/COMMANDS.md "Discord"): a pill right
# of the Boost development button that opens MitCad's Discord server, the
# action help.discord (Help › MitCad on Discord); since mitcad#2159 in
# Discord's blurple with Discord's logo mark (third_party/brand-marks, its
# source and terms in its README; the resource :/brand/discord-mark.svg),
# its × and Preferences' setting.
target_sources(mitcad PRIVATE framework/DiscordButton.cpp framework/DiscordButton.hpp)
# Boundary Fill (mitcad#2149, app/COMMANDS.md "Boundary Fill"): the space
# solids, surfaces and planes close off filled, its closed regions picked
# by their dots in the view; home Solid › Create, again in Surface › Modify.
target_sources(mitcad PRIVATE commands/BoundaryFillCommands.cpp)

set(MITCAD_DISCORD_MARK "${PROJECT_SOURCE_DIR}/third_party/brand-marks/discord-mark.svg")
set_source_files_properties("${MITCAD_DISCORD_MARK}" PROPERTIES QT_RESOURCE_ALIAS "discord-mark.svg")
qt_add_resources(mitcad brand_marks PREFIX /brand FILES "${MITCAD_DISCORD_MARK}")


# The design window's native windows (mitcad#2152, GitHub mitcad/Mitcad#529):
# the test steps expect-design-native and expect-screen-beside-cards, which
# app.ui-windows-space-mouse-3dxware runs with the 3DxWare runtime's
# stand-in (MITCAD_TEST_3DXWARE).
target_sources(mitcad PRIVATE framework/TestDriverNativeWindows.cpp)

# The hatch over Section Analysis' caps (mitcad#2146, app/COMMANDS.md
# "Section Analysis"): lines in the section plane itself, cut at the caps'
# triangulation, read by the unit test section_hatch.
target_sources(mitcad PRIVATE view/SectionHatch.cpp view/SectionHatch.hpp)

# Preferences › General › Extrude starts with (mitcad#2164, GitHub
# mitcad/Mitcad#496, app/COMMANDS.md "Extrude"): Automatic or New Body, read
# when a new Extrude starts (CommandSession::startAutoOperation).
target_sources(mitcad PRIVATE framework/ExtrudeStartPreference.cpp framework/ExtrudeStartPreference.hpp)

# Extend to Meet (mitcad#2178, app/COMMANDS.md "Extend to Meet"): two lines
# or arcs extended or shortened to one shared corner point, the results the
# model chooses from for the tool's preview and its Tab.
target_sources(mitcad PRIVATE sketch/ExtendToMeet.cpp sketch/ExtendToMeet.hpp)

# Draft Analysis (mitcad#2166, GitHub mitcad/Mitcad#535, app/COMMANDS.md
# "Draft Analysis"): the definition, the draft_analysis query and its answer
# in words, and the three classes the view draws over the bodies.
target_sources(mitcad PRIVATE commands/DraftAnalysis.cpp commands/DraftAnalysis.hpp)

# Selection sets (mitcad#2192, GitHub mitcad/Mitcad#493, app/COMMANDS.md
# "Selection Sets"): what of a selection or of Browser rows a set takes, as
# the model's members, for Create Selection Set and a set's row menu.
target_sources(mitcad PRIVATE browser/SelectionSets.cpp browser/SelectionSets.hpp)

# The UI test driver's steps by area (mitcad#2173): each file has its steps'
# specs and their chain, TestDriver.cpp the common steps and the helpers the
# areas share (TestDriverSupport.hpp). The check's UI test selection gives a
# test that declares TestDriver.cpp only the area files whose steps it uses.
target_sources(mitcad PRIVATE
  framework/TestDriverSupport.hpp
  framework/TestDriverDocumentWindows.cpp
  framework/TestDriverModel.cpp
  framework/TestDriverViewShapes.cpp
  framework/TestDriverViewPixels.cpp
  framework/TestDriverViewCamera.cpp
  framework/TestDriverNativeControls.cpp
  framework/TestDriverSketch.cpp
  framework/TestDriverPointer.cpp
  framework/TestDriverPicks.cpp
  framework/TestDriverWidgets.cpp
  framework/TestDriverCommands.cpp
  framework/TestDriverEnvironment.cpp
  framework/TestDriverFiles.cpp)

# Save of a new design (mitcad#2183, app/COMMANDS.md "Save"): On this
# computer or To a project, asked when a remote project is known, with
# Remember my choice and Preferences › General › Save of a new design;
# the question is read by the unit test new_design_save.
target_sources(mitcad PRIVATE files/NewDesignSave.cpp files/NewDesignSave.hpp)

# An expression checked while it is typed (mitcad#2181): Add User
# Parameter's field and Edit Parameters' Expression cells show the model's
# words and mark the unknown name in the field.
target_sources(mitcad PRIVATE framework/ExpressionCheck.hpp)

# Cell Grid in the SOLID tab's CREATE group (mitcad#2238): sketch profiles
# filled with cells and cut from bodies.
target_sources(mitcad PRIVATE commands/CellGridCommands.cpp)

# Scripts and macros (mitcad#2245, app/COMMANDS.md "Macros and scripts"):
# File › Export Timeline as Script…, Tools › Macro's recorder (the commands
# DocumentSession notes), Save Macro… and Run Script…; the recorder's rules
# are read by the unit test macro_recorder.
target_sources(mitcad PRIVATE framework/MacroRecorder.cpp framework/MacroRecorder.hpp files/MainWindowScripts.cpp)

# Favorite Parameters (mitcad#2203, GitHub mitcad/Mitcad#547, app/COMMANDS.md
# "Favorite Parameters"): the starred parameters' rows (read by the unit test
# favorite_parameter_rows), the card's content with its fields and sliders,
# and the main window's card, dock and MANAGE toggle.
target_sources(mitcad PRIVATE
  browser/FavoriteParameterRows.cpp
  browser/FavoriteParameterRows.hpp
  browser/FavoriteParametersCard.cpp
  browser/FavoriteParametersCard.hpp
  browser/MainWindowFavoriteParameters.cpp)

# Cancel Sketch, and every sketch edit as one undo step at Finish Sketch
# (mitcad#2225).
target_sources(mitcad PRIVATE MainWindowSketchCancel.cpp)

# Point clouds and Point Cloud Deviation (mitcad#2240, app/COMMANDS.md
# "Point Clouds", "Point Cloud Deviation"): Insert > Point Cloud and Move
# Point Cloud, the clouds drawn in the view, and the analysis' definition,
# query and words with its coloured points over the bodies.
target_sources(mitcad PRIVATE MainWindowPointClouds.cpp view/PointClouds.cpp view/PointClouds.hpp
                              view/PointCloudView.cpp commands/PointCloudDeviation.cpp
                              commands/PointCloudDeviation.hpp)

# Decals (mitcad#2244, app/COMMANDS.md "Decals"): Insert › Decal's panel,
# the decals drawn on their faces in the view, and the meshes and
# materials the renderer draws them with.
target_sources(mitcad PRIVATE
  MainWindowDecals.cpp
  render/RenderDecals.cpp
  render/RenderDecals.hpp
  view/DecalView.cpp
  view/Decals.cpp
  view/Decals.hpp)

# Work that arrives while a command is open (mitcad#2281,
# docs/ui-conventions.md): a Component from Web download or files dropped on
# the window, at once beside an inspection, else waiting until the open
# command closes.
target_sources(mitcad PRIVATE files/MainWindowArrivals.cpp)

# Exploded views (mitcad#2113, app/COMMANDS.md "Exploded views"): Explode's
# panel with its steps, Auto Explode and the play bar, Explode Step, the
# Browser's Exploded Views and the view shown exploded with its trail lines.
target_sources(mitcad PRIVATE
  MainWindowExplode.cpp
  commands/ExplodeCommands.cpp
  motion/ExplodeViewDialog.cpp
  motion/ExplodeViewDialog.hpp
  view/ExplodeTrailOverlay.cpp)

# Move/Copy of a component picked inside a sub-assembly (mitcad#2302,
# app/COMMANDS.md "Move/Copy"): the component the move works in and the
# occurrence whose placement is its frame.
target_sources(mitcad PRIVATE commands/MoveTarget.cpp commands/MoveTarget.hpp)

# The graphics check at start on Linux (mitcad#2292, GitHub mitcad/Mitcad#586,
# framework/GraphicsStart.hpp): the window's OpenGL context and surface
# checked before any window, a lower format where the driver refuses the
# viewport's, and a start again on X11 with GLX where none works, so that
# the window and its panels draw instead of a black window.
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND OpenCASCADE_WITH_EGL)
  target_sources(mitcad PRIVATE framework/GraphicsStart.cpp framework/GraphicsStart.hpp)
endif()

# The accessibility tree as a client reads it, and the log line that names
# the process reading it through UI Automation (mitcad#2227).
target_sources(mitcad PRIVATE framework/AccessibleTree.cpp framework/AccessibleTree.hpp
               framework/AccessibilityClients.cpp framework/AccessibilityClients.hpp)
if(WIN32)
  # CoGetCallerTID; the processes with UI Automation loaded.
  target_link_libraries(mitcad PRIVATE ole32 psapi)
endif()
if(BUILD_TESTING AND WIN32)
  # The UI Automation client of app.ui-windows-accessible-tree: one bulk
  # read of the window's whole tree, timed.
  add_executable(mitcad-uia-walk tests/uia_walk.cpp)
  target_link_libraries(mitcad-uia-walk PRIVATE ole32 oleaut32)
endif()

# Drawings over the whole view that the view draws itself in its overlay
# layer, not widgets over it (mitcad#2296): sketch mode's overlay and the
# dimensions shown outside sketch mode.
target_sources(mitcad PRIVATE view/ViewPaintLayer.cpp view/ViewPaintLayer.hpp)

# Split Body's tools of other components (mitcad#2299, GitHub mitcad/Mitcad#588,
# app/COMMANDS.md "Split Body"): a face or body picked in another
# component's occurrence goes into `tool_links`, and an edit shows it there.
target_sources(mitcad PRIVATE commands/SplitToolLinks.cpp commands/SplitToolLinks.hpp)

# The import progress's timeline item (mitcad#2331): the items done so far
# plus one against the file's count, never going back, with the current
# item's name and component; read by the unit test import_item_progress.
target_sources(mitcad PRIVATE files/ImportItemProgress.cpp files/ImportItemProgress.hpp)

# Edit in Place (mitcad#2321, GitHub mitcad/Mitcad#598, app/COMMANDS.md
# "Linked designs"): a linked component's design edited inside the assembly
# that holds it, saved to its own file.
target_sources(mitcad PRIVATE files/MainWindowEditInPlace.cpp)
