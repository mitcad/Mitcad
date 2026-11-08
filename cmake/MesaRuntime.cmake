# SPDX-License-Identifier: MIT
# Windows development builds only: copies Mesa's software OpenGL (llvmpipe)
# from MITCAD_MESA_DIR next to mitcad.exe. Windows loads opengl32.dll from
# the application's directory first, so Qt and OCCT then render with Mesa,
# e.g. in a VM without a GPU that offers only OpenGL 1.1. The DLLs come from
# tools/dev-env/setup-mesa-windows.ps1. Off by default; never in a release.
set(MITCAD_MESA_DIR "" CACHE PATH
  "Windows development builds: copy Mesa's opengl32.dll and libgallium_wgl.dll from this directory next to mitcad.exe")

set(mesa_runtime_dlls opengl32.dll libgallium_wgl.dll)

if(MITCAD_MESA_DIR)
  if(NOT WIN32)
    message(FATAL_ERROR "MITCAD_MESA_DIR is only for Windows builds")
  endif()
  set(mesa_runtime_files "")
  foreach(dll IN LISTS mesa_runtime_dlls)
    if(NOT EXISTS "${MITCAD_MESA_DIR}/${dll}")
      message(FATAL_ERROR "MITCAD_MESA_DIR: ${MITCAD_MESA_DIR}/${dll} not found; "
                          "build it with tools/dev-env/setup-mesa-windows.ps1")
    endif()
    list(APPEND mesa_runtime_files "${MITCAD_MESA_DIR}/${dll}")
  endforeach()
  message(STATUS "Mesa software OpenGL from ${MITCAD_MESA_DIR} is copied next to mitcad.exe")
  # A target of its own: add_custom_command(TARGET mitcad) only works in
  # app/CMakeLists.txt.
  add_custom_target(mitcad_mesa_runtime ALL
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different ${mesa_runtime_files} "$<TARGET_FILE_DIR:mitcad>"
    VERBATIM)
  add_dependencies(mitcad_mesa_runtime mitcad)
  if(BUILD_TESTING)
    # With a renderer the app can run in ctest: screenshots of a demo block
    # and of a project file opened from the command line.
    add_test(NAME app.ui-windows
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    # Input-driven: sketch, extrude, fillet, save, open, STEP export and
    # import with mouse and keyboard messages posted to the window.
    add_test(NAME app.ui-windows-workflow
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-workflow-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-workflow"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-workflow PROPERTIES TIMEOUT 300)
    # An .f3d design of components of other documents (mitcad#196): the
    # file dialog for each document not found, skipped or chosen.
    add_test(NAME app.ui-windows-references
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-references-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-references"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-references PROPERTIES TIMEOUT 300)
    # Sheet metal (mitcad#210): Face, Flange, rule edits, Flat Pattern, its
    # DXF export, an edit from the timeline, the Floating layout.
    add_test(NAME app.ui-windows-sheet
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sheet PROPERTIES TIMEOUT 300)
    # Sheet-metal operations (mitcad#125): Fold, Bend, Corner Round and
    # Chamfer, Corner Relief, Sheet Cut, Unfold and Refold, each with its
    # picks, preview, a timeline edit and Undo.
    add_test(NAME app.ui-windows-sheet-ops
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-ops-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-ops"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sheet-ops PROPERTIES TIMEOUT 600)
    # Design policy (mitcad#220): the new design's Component1, a root that
    # refuses sketches, the policy dialog and its presets, undo, and
    # sketch planes of other components left out of picking.
    add_test(NAME app.ui-windows-policy
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-policy-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-policy"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-policy PROPERTIES TIMEOUT 300)
    # Clean Up Projections (mitcad#232): a face projected twice into a
    # sketch on it, the repeats listed by the preview and removed; a
    # coincident that replaces a Horizontal and a refused snap (mitcad#287).
    add_test(NAME app.ui-windows-projections
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-projections-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-projections"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-projections PROPERTIES TIMEOUT 420)
    # Picks on clipped bodies (mitcad#233): Project with Hide Above Sketch
    # on, Fillet under Section Analysis, by the bodies' own names.
    add_test(NAME app.ui-windows-section-pick
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-section-pick-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-section-pick"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-section-pick PROPERTIES TIMEOUT 300)
    # Dimension names (mitcad#231): the tooltip, the name in the value
    # editor, a dimension picked into it, a rename, Dimension text, and a
    # feature panel's field with its parameter's name.
    add_test(NAME app.ui-windows-dimension-names
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-dimension-names-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-dimension-names"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-dimension-names PROPERTIES TIMEOUT 300)
    # The gap between two concentric circles from Sketch Dimension
    # (mitcad#445).
    add_test(NAME app.ui-windows-concentric-dimension
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-concentric-dimension-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-concentric-dimension"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-concentric-dimension PROPERTIES TIMEOUT 300)
    # Tangent Plane (mitcad#446): the reference plane and angle filled in
    # from the click, the preview drawn over the picked plane.
    add_test(NAME app.ui-windows-tangent-plane
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-tangent-plane-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-tangent-plane"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-tangent-plane PROPERTIES TIMEOUT 300)
    # Sketch mode's view: fully constrained lines recoloured when the
    # background turns dark or light (mitcad#253), the sketch origin's place
    # and the pointer's sketch coordinates (mitcad#254).
    add_test(NAME app.ui-windows-sketch-view
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-view-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-view"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-view PROPERTIES TIMEOUT 420)
    # The sketch's own origin and axes drawn in a new design whose Origin
    # folder's light bulb is off, on XY, XZ and YZ and in an edited sketch,
    # the light bulb untouched, the origin snapped to (mitcad#739).
    add_test(NAME app.ui-windows-sketch-origin
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-origin-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-origin"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-origin PROPERTIES TIMEOUT 300)
    # Extrude's preview in components placed off the origin (mitcad#266):
    # hidden bodies and occurrences stay hidden, shown and new bodies are
    # where their occurrences put them, a linked component's too.
    add_test(NAME app.ui-windows-preview-placement
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-preview-placement-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-preview-placement"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-preview-placement PROPERTIES TIMEOUT 300)

    # Live MCP for debugging (mitcad#265): the actual application and
    # mitcad-cli.exe mcp --connect; queries and sketch commands with a
    # sketch open, ui_state, ui_select, ui_screenshot, ui_command, Save,
    # ui_log.
    add_test(NAME app.ui-windows-mcp
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-mcp-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-mcp"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-mcp PROPERTIES TIMEOUT 300)
    # The debug log (mitcad#269): --debug-log, its header and lines,
    # Preferences' switch on and off without a restart, Open Log Folder;
    # Create Sketch by its default Alt+S (mitcad#267).
    add_test(NAME app.ui-windows-debug-log
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-debug-log-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-debug-log"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-debug-log PROPERTIES TIMEOUT 300)

    # Body edges and vertices in the sketch plane (mitcad#256, mitcad#258):
    # Sketch Dimension from a block edge in one undo step and its projection
    # reused; the Point tool on a corner and at an edge's middle, the points
    # following the block.
    add_test(NAME app.ui-windows-body-picks
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-body-picks-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-body-picks"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-body-picks PROPERTIES TIMEOUT 300)

    # Light bulbs in every mode (mitcad#295): a body hidden while Create
    # Sketch asks for a plane lets the face behind it be picked; during
    # Extrude a body hidden stays so on Cancel and is no part of OK's undo
    # step; an edit's roll-back keeps it too. Create Sketch's origin planes
    # in a design with components too, behind its faces (mitcad#844).
    add_test(NAME app.ui-windows-visibility
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-visibility-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-visibility"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-visibility PROPERTIES TIMEOUT 300)

    # Extrude's automatic operation (mitcad#289): Join on a body's face,
    # Cut into it and Join out again, New Body away from bodies, a hand
    # picked operation that stays, and an edit that keeps its operation.
    add_test(NAME app.ui-windows-auto-operation
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-auto-operation-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-auto-operation"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-auto-operation PROPERTIES TIMEOUT 300)

    # The browser's scrolling with a selection (mitcad#318): a pick in the
    # view scrolls to its row out of sight; a Ctrl+click on a row below the
    # first selected one, scrolled away, keeps the scroll position.
    add_test(NAME app.ui-windows-browser
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-browser"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-browser PROPERTIES TIMEOUT 300)
    # The Browser's Rows background in the Floating layout (mitcad#419): a
    # click beside the rows picks the plate behind them, a middle drag there
    # orbits, a row still takes its click; Card again takes the click.
    add_test(NAME app.ui-windows-browser-rows
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-rows-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-rows"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-browser-rows PROPERTIES TIMEOUT 300)

    # The library browser's Update (mitcad#317): empty until Update, which
    # fetches every enabled source (a local git repository) and lists its
    # parts; a broken source reported in one warning, the selection kept.
    add_test(NAME app.ui-windows-library
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-library-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-library"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-library PROPERTIES TIMEOUT 300)

    # The rendered view from below the ground (mitcad#320): orbited under
    # the ground and back, orthographic and perspective, the box rendered
    # in every view, the ground left out below and built again above.
    add_test(NAME app.ui-windows-render-ground
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-render-ground-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-render-ground"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-render-ground PROPERTIES TIMEOUT 600)

    # The BUILD tab (mitcad#375): a library of synthetic parts fetched from
    # a local git repository; a frame of four profiles (a mitred end, a
    # bound length), a connector, a panel, a rail with a carriage; saved,
    # reopened, the cut list's CSV and PDF; the part picker (mitcad#418).
    add_test(NAME app.ui-windows-build
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-build"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-build PROPERTIES TIMEOUT 900)
    # The BUILD tab's TIMBER group (mitcad#478, mitcad#520), synthetic
    # timber, a slot profile and fasteners fetched from a local git
    # repository (tools/ui-windows-timber-lib.ps1), in six tests that run
    # side by side (mitcad#834; once one of over nine minutes):
    #   timber      the Timber tool: the pickers apart, a post, a member on
    #               its side, a member by two points, a spacing set and its
    #               edit, a timber junction screwed, the cut list's timber,
    #               a tile per size;
    #   -frames     frames of four members and of slot profiles, mitred, and
    #               a member standing on a board;
    #   -wall       Wall: butt and lapped corners, a wall between two, and
    #               Timber's Two points between studs;
    #   -openings   the Openings window, walls started on a solid wall and
    #               across a long room;
    #   -fasteners  a wall's and a spacing set's fasteners, Swap Fastener;
    #   -floor-roof a floor, frames as components, a gable roof's nails.
    foreach(timber_test IN ITEMS timber timber-frames timber-wall timber-openings timber-fasteners
                                 timber-floor-roof)
      add_test(NAME app.ui-windows-${timber_test}
        COMMAND powershell -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-${timber_test}-test.ps1"
                -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-${timber_test}"
                -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
      set_tests_properties(app.ui-windows-${timber_test} PROPERTIES TIMEOUT 300)
    endforeach()

    # The window's layout chosen in Preferences (mitcad#419): Floating
    # asks to restart, Later and Restart Now (unsaved changes asked about,
    # the restart's command logged), the next start Floating, --chrome
    # overriding it; the Floating cards below the top bar at the default
    # size and in a narrow and a wide window.
    add_test(NAME app.ui-windows-chrome-layout
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-chrome-layout-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-chrome-layout"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-chrome-layout PROPERTIES TIMEOUT 300)
    # Everything outside the active component as ghosts (mitcad#420): off
    # in the settings none; on, a ghost's face picked for a sketch and a
    # projection; the root activated, undone, redone; the active component
    # first in picks, through a ghost in front of it.
    add_test(NAME app.ui-windows-inactive-ghost
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-inactive-ghost-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-inactive-ghost"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-inactive-ghost PROPERTIES TIMEOUT 300)
    # A project's web viewer (mitcad#423): File > Project > Web Viewer
    # writes both designs' pages and the index in one version; a Save
    # writes the design's page again; Project Settings' Update on each Save
    # off leaves it.
    add_test(NAME app.ui-windows-web-viewer
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-web-viewer-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-web-viewer"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-web-viewer PROPERTIES TIMEOUT 600)
    # File > Save to Project (mitcad#503): after Save As, no Move to a
    # Project in the menu; the old command id's saved shortcut still starts
    # it; a loose design saved in a project under a typed name (the
    # original stays); Save with Description's question opens the picker.
    add_test(NAME app.ui-windows-save-to-project
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-save-to-project-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-save-to-project"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-save-to-project PROPERTIES TIMEOUT 300)
    # A design as it opens (mitcad#426): the root active whatever the file
    # had active, unmodified; a tree longer than the browser folded below
    # the root, a small one expanded; Collapse All keeps the root open.
    add_test(NAME app.ui-windows-browser-open
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-open-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-open"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-browser-open PROPERTIES TIMEOUT 300)
    # File > Export Drawing (mitcad#471): the dialog with its defaults, a
    # typed SVG file, A3 chosen, the drawing written (3 views, 2 bodies,
    # A3, 1:1), the choices kept.
    add_test(NAME app.ui-windows-drawing
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-drawing PROPERTIES TIMEOUT 300)
    # Drawings kept in the design (mitcad#707): Create Drawing, a base, a
    # projected, a section and a detail view, the model changed under them,
    # saved and opened again, a PDF exported.
    add_test(NAME app.ui-windows-drawings
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawings-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawings"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-drawings PROPERTIES TIMEOUT 300)
    # A drawing keeps what was put into it (mitcad#1293): a component hidden
    # in the browser and one pasted later change no view; its Shows adds
    # one, Update from Model takes what the 3D view shows, each one undo
    # step; an older file's view is pinned when it opens.
    add_test(NAME app.ui-windows-drawing-keeps-content
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-keeps-content-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-keeps-content"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-drawing-keeps-content PROPERTIES TIMEOUT 300)

    # Gear (mitcad#472): a spur gear from the panel, a rack meshing it.
    add_test(NAME app.ui-windows-gear
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-gear-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-gear"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-gear PROPERTIES TIMEOUT 300)
    # Pulley (mitcad#562): a GT2 pulley from the panel, its teeth and
    # flanges changed, edited, a second pulley on the belt (its length).
    add_test(NAME app.ui-windows-pulley
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-pulley-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-pulley"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-pulley PROPERTIES TIMEOUT 300)
    # Bearing (mitcad#577): a 608 from the panel, too many elements, edited,
    # then a raceway cut into a shaft made by mitcad-cli.
    add_test(NAME app.ui-windows-bearing
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-bearing-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-bearing"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-bearing PROPERTIES TIMEOUT 300)
    # Gear Pair and Belt Drive (mitcad#656): both parts of a ratio between
    # two shafts made by mitcad-cli, a row of the candidates chosen.
    add_test(NAME app.ui-windows-drives
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drives-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drives"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-drives PROPERTIES TIMEOUT 300)
    # Placement by picked geometry (mitcad#656): earlier placements opened
    # through Place On, a box on a face from its edges, turned in the view.
    add_test(NAME app.ui-windows-place
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-place-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-place"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-place PROPERTIES TIMEOUT 300)
    add_test(NAME app.ui-windows-print-fits
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-fits-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-print-fits" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-print-fits PROPERTIES TIMEOUT 360)
    # Configurations (mitcad#807): the window opens and a choice in the
    # Configuration bar changes the body (the test driver's
    # script, shared with the macOS test).
    add_test(NAME app.ui-windows-configurations
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-configurations-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-configurations" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-configurations PROPERTIES TIMEOUT 360)
    # Help > User Guide (mitcad#1032), Docked and Floating: the menu entry
    # and the command each would open the guide (the test driver's script,
    # shared with the Linux test).
    add_test(NAME app.ui-windows-user-guide
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-user-guide-test.ps1"
              -App "$<TARGET_FILE:mitcad>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-user-guide" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-user-guide PROPERTIES TIMEOUT 300)
    # The file dialogs' folders per group (mitcad#498): Import, Export and
    # Open each start in their own last folder, also after a restart.
    add_test(NAME app.ui-windows-dialog-folders
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-dialog-folders-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-dialog-folders"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-dialog-folders PROPERTIES TIMEOUT 300)
    # Property Sets (mitcad#496): an occurrence's and its component's
    # levels, a value edited in place, an own type's value as a choice,
    # Find in Design by property and the Parts List's property columns.
    add_test(NAME app.ui-windows-property-sets
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-property-sets-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-property-sets"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-property-sets PROPERTIES TIMEOUT 300)
    # The sketch's axes and origin as picks of Sketch Dimension and the
    # constraint tools (mitcad#500): a distance from the x axis, an angle to
    # it and a point on the y axis in a sketch with nothing at the origin.
    add_test(NAME app.ui-windows-sketch-axes
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-axes-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-axes"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-axes PROPERTIES TIMEOUT 300)
    # An .iam assembly's Parts with their design history (mitcad#507): the
    # test project imported with and without, the parts' progress and
    # outcomes, the volumes both ways, the choice remembered; with
    # MITCAD_IAM_UI_ASSEMBLY or MITCAD_IPT_CORPUS the test files' seven-part
    # assembly too (skipped without).
    add_test(NAME app.ui-windows-iam-history
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-iam-history-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-iam-history"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-iam-history PROPERTIES TIMEOUT 3000)
    # A sketch on a face of an occurrence turned and moved (mitcad#505):
    # its frame in the design on the face, Look At normal to it and a drawn
    # line on the face, with the component active and with the root.
    add_test(NAME app.ui-windows-occurrence-face-sketch
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-occurrence-face-sketch-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-occurrence-face-sketch"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-occurrence-face-sketch PROPERTIES TIMEOUT 300)
    # Reorient Design (mitcad#502): the whole design turned 90 degrees about
    # X, previewed in its dialog, kept by OK and taken back by Undo.
    add_test(NAME app.ui-windows-reorient-design
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-reorient-design-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-reorient-design"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-reorient-design PROPERTIES TIMEOUT 300)
    # Mount frames of any part and Place Part (mitcad#525): synthetic
    # Gridfinity bins on a baseplate's cells (a taken cell refused), a lamp
    # free on a face, an own screw's frame made, published and placed into a
    # hole, a drop-in joined by its frame; a library in a local git
    # repository.
    add_test(NAME app.ui-windows-mount
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-mount-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-mount"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-mount PROPERTIES TIMEOUT 900)
    # The joint highlight (mitcad#523): a face-to-face joint and a circular
    # edge's joint selected in the browser and the timeline, each side's
    # geometry, frames and occurrences logged, another joint replacing it,
    # a click on nothing clearing it, a row's hover previewing it; a BUILD
    # joint's mount frames with their parts.
    add_test(NAME app.ui-windows-joint-highlight
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-joint-highlight-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-joint-highlight"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-joint-highlight PROPERTIES TIMEOUT 300)
    # MODIFY > Move Face (mitcad#531): a block's end face pushed out, its
    # timeline cell's card and edit, its top tilted about an edge, a motion
    # the neighbouring faces cannot follow refused on the panel.
    add_test(NAME app.ui-windows-move-face
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-face-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-move-face"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-move-face PROPERTIES TIMEOUT 300)
    # The Projects panel's rows (mitcad#529): a 55-character design name
    # elided in the middle in the Floating card, its lock (Editing
    # elsewhere) as a mark, the full name in the tooltip; the card widened by
    # its edge shows the whole name and keeps its width after a restart.
    add_test(NAME app.ui-windows-projects-rows
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-projects-rows-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-projects-rows"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-projects-rows PROPERTIES TIMEOUT 300)
    # File > Open from Project (mitcad#530): in the File menu after Open;
    # two projects listed, a folder and its sub-folder gone into and back,
    # the search finding the other project's design, Open and Open
    # Read-Only, the place shown last after a restart.
    add_test(NAME app.ui-windows-open-from-project
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-open-from-project-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-open-from-project"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-open-from-project PROPERTIES TIMEOUT 300)
    # Extrude with a planar body face as the profile (mitcad#535): a face
    # of an imported cube picked in the command and joined, a sketched
    # cube's side selected before it (not the unused square beside it) cut
    # in.
    add_test(NAME app.ui-windows-extrude-face
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-face-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-face"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-extrude-face PROPERTIES TIMEOUT 300)
    # Combine with the same body in two occurrences as tools (mitcad#544):
    # Part's block picked where Part:1 and where Part:2 show it, listed
    # "Part:1, Part:2", both cut from the root's block; the design file
    # keeps a tool in each occurrence and an edit takes both back. Extrude
    # To Object to a face of another component (mitcad#762), linked.
    add_test(NAME app.ui-windows-component-combine
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-component-combine-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-component-combine"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-component-combine PROPERTIES TIMEOUT 480)

    # The patterns' Direction Type and Distance Type (mitcad#545): a
    # rectangular pattern symmetric along X and by extent along Y, a
    # circular one symmetric, the stored definitions and the copies' places,
    # the panels of the reopened patterns showing their options.
    add_test(NAME app.ui-windows-pattern-options
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-pattern-options-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-pattern-options"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-pattern-options PROPERTIES TIMEOUT 300)

    # The sketches a command shows for its time (mitcad#546): an edited
    # Extrude's hidden sketch shown, its square picked, hidden again after
    # OK and Cancel; a sketch shown by its light bulb stays; the saved
    # design without a visibility change; a new Extrude's profile of a
    # hidden sketch; the light bulb turned on during the edit.
    add_test(NAME app.ui-windows-edit-sketches
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-edit-sketches-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-edit-sketches"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-edit-sketches PROPERTIES TIMEOUT 300)

    # The Docked layout's window height (mitcad#560): a design of 30
    # bodies, Root/Bodies opened and the pattern's 29 bodies selected from
    # the timeline; the Browser's rows scroll, the command panel's
    # selection lines scroll, and the window, the view and the timeline's
    # cells keep their sizes and places; a plain double-click edits.
    add_test(NAME app.ui-windows-docked-height
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-height-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-height"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-docked-height PROPERTIES TIMEOUT 300)

    # The selection card's Create row (mitcad#619): a profile selected
    # offers Extrude, Revolve and Sweep, the card's Extrude starts with
    # the profile, a cylinder's round face offers Thread first, a cleared
    # selection nothing.
    add_test(NAME app.ui-windows-selection-create
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-selection-create-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-selection-create"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-selection-create PROPERTIES TIMEOUT 300)

    # Measure (mitcad#647): a round face's axis shown and drawn, parallel
    # axes 50 apart, a round face square to a plane, two rims' axes and
    # centres.
    add_test(NAME app.ui-windows-measure
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-measure-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-measure"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    # Its Copy and Copy All write the system clipboard, which every test of
    # one ssh logon shares: not at once with the other tests that use it
    # (app.ui-windows-sketch-clipboard, app.ui-windows-assistant-setup).
    set_tests_properties(app.ui-windows-measure PROPERTIES TIMEOUT 300 RESOURCE_LOCK system_clipboard)

    # 3D Print's Import from slicer... (mitcad#654, mitcad#655): a Bambu Studio
    # project whose filaments are slowed for cooling on overhangs from 95%
    # imported; the line under the button counts by kind, Details... shows
    # the whole report.
    add_test(NAME app.ui-windows-print-settings
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-settings-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-print-settings" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-print-settings PROPERTIES TIMEOUT 300)

    # My library without a design (mitcad#704, mitcad#1135): the managers
    # opened with no design open; a built-in material duplicated into My
    # library, renamed, its density changed and deleted, a printer
    # duplicated, renamed and deleted, each in the library's files; the
    # window's design unchanged.
    add_test(NAME app.ui-windows-my-library
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-my-library-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-my-library" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-my-library PROPERTIES TIMEOUT 300)

    # The print managers (mitcad#1135): Printers and Processes apart, the
    # groups by place, a project item edited for the project's other
    # design, a built-in material's Duplicate asking where, no Tools › My
    # Library.
    add_test(NAME app.ui-windows-print-managers
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-managers-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-print-managers" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-print-managers PROPERTIES TIMEOUT 300)

    # Tools › Libraries (mitcad#1250): the menu in its groups; Material and
    # Printer Library… on My library with no design open, an item edited
    # in its file; Process Library…; the Material tool's Material
    # Library…, whose new material the tool offers, its bodies kept.
    add_test(NAME app.ui-windows-libraries-menu
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-libraries-menu-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-libraries-menu" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-libraries-menu PROPERTIES TIMEOUT 300)

    # One filament, its variants per printer and nozzle (mitcad#1054): 3D
    # Print's Materials row, Import Presets grouped by filament, the
    # Materials window's Variants tab (the driver script shared with the
    # Linux and macOS tests).
    add_test(NAME app.ui-windows-filament-variants
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-filament-variants-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-filament-variants" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-filament-variants PROPERTIES TIMEOUT 360)

    # A filament's colour is its appearance (mitcad#1055): Material ▸
    # material ▸ appearance, the Materials window's appearances and the
    # appearance picker (the driver script shared with the Linux and macOS
    # tests).
    add_test(NAME app.ui-windows-filament-appearance
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-filament-appearance-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-filament-appearance" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-filament-appearance PROPERTIES TIMEOUT 360)

    # Measurement objects (mitcad#649): kept from the Measure panel and its
    # history (annotations since mitcad#702), listed in the browser's
    # Annotations folder, a 4 x transparent snapshot, saved and opened
    # again.
    add_test(NAME app.ui-windows-measurement-objects
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-measurement-objects-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-measurement-objects"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-measurement-objects PROPERTIES TIMEOUT 300)

    # Annotations (mitcad#702): a tolerance from the editor opened in the
    # view, the text dragged, an H7 fit, several changed together in the
    # Annotations window, a group hidden, a snapshot of one group.
    add_test(NAME app.ui-windows-annotations
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-annotations-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-annotations"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-annotations PROPERTIES TIMEOUT 300)

    # Preferences' size (mitcad#690): with git found, MITCAD_GIT naming a
    # missing file and git not installed, the window fits a 1366 x 768
    # screen, is resizable, its frame lies on the screen and the page list
    # shows its entries whole.
    add_test(NAME app.ui-windows-preferences
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-preferences-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-preferences"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-preferences PROPERTIES TIMEOUT 300)
    # Inactive sketch constraints (mitcad#721): drawn inactive, Activate
    # refused and applied, undo, Delete.
    add_test(NAME app.ui-windows-sketch-inactive
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-inactive-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-inactive"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-inactive PROPERTIES TIMEOUT 300)

    # Keys per context (mitcad#720): Keyboard Shortcuts driven by its keys
    # (Command Search F3 in Sketch, Fit Point Spline S, OK; then a conflict
    # OK refuses); S draws a spline in a sketch and opens the command search
    # in the model, F3 searches in the sketch.
    add_test(NAME app.ui-windows-shortcut-contexts
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-shortcut-contexts-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-shortcut-contexts"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-shortcut-contexts PROPERTIES TIMEOUT 300)
    # Purge volumes (mitcad#705): 3D Print's Purge volumes row on two
    # bodies of a white and a black filament, the table opened, a cell
    # typed (set by hand, saved in the design) and reset to the computed
    # volume.
    add_test(NAME app.ui-windows-purge-volumes
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-purge-volumes-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-purge-volumes" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-purge-volumes PROPERTIES TIMEOUT 300)
    # Print bed face (mitcad#731): set from a face's context menu, shown
    # from another face's, cleared and undone; 3D Print turns the box onto
    # it; Print Analysis starts from it; a face that is gone is reported.
    add_test(NAME app.ui-windows-print-bed
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-bed-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-print-bed" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-print-bed PROPERTIES TIMEOUT 300)
    # Move/Copy in the context menus (mitcad#735): on a body's row and on
    # two occurrences' rows in the browser, and at the top of the view's
    # menu for a body selected in the browser; none for a face alone.
    add_test(NAME app.ui-windows-move-copy-menu
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-copy-menu-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-move-copy-menu" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-move-copy-menu PROPERTIES TIMEOUT 300)
    # Save As Mesh (mitcad#734): from a body's, a component's and the
    # root's browser rows, 3MF on the print bed face, text STL in inches,
    # OBJ a file per body with a longest edge; disabled for a surface body.
    add_test(NAME app.ui-windows-save-as-mesh
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-save-as-mesh-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-save-as-mesh" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-save-as-mesh PROPERTIES TIMEOUT 300)
    # The view cube in a sketch with a constraint selected (mitcad#773):
    # an active, an inactive and a drawn line's inferred constraint
    # selected in turn, every arrow and Home clicked; the app lives on,
    # writes no crash report and logs no failed view operation.
    add_test(NAME app.ui-windows-sketch-cube
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-cube-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-cube"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-cube PROPERTIES TIMEOUT 300)
    # Copy, Cut and Paste of sketch geometry (mitcad#788): a line cut from
    # one sketch with its constraints, pasted into another by a click and
    # by typed X and Y, undo and Esc.
    add_test(NAME app.ui-windows-sketch-clipboard
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-clipboard-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-clipboard"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    # Cut, then Paste seconds later: another test writing the shared
    # system clipboard in between would replace the fragment (see
    # app.ui-windows-measure).
    set_tests_properties(app.ui-windows-sketch-clipboard PROPERTIES TIMEOUT 300 RESOURCE_LOCK system_clipboard)
    # Redefine Sketch Plane (mitcad#787): from a sketch's browser row onto
    # a face with an offset, undone; a face of a later feature refused.
    add_test(NAME app.ui-windows-sketch-redefine-plane
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-redefine-plane-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-redefine-plane" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-redefine-plane PROPERTIES TIMEOUT 300)
    # Image references (mitcad#782): an image file placed on a plate's top
    # face, calibrated by two clicks on it and a typed distance, edited in
    # its panel (opacity, Through Bodies), hidden, saved and shown again.
    add_test(NAME app.ui-windows-image-references
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-image-references-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-image-references"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-image-references PROPERTIES TIMEOUT 300)
    # Closed fitted splines and tangents at a chosen end (mitcad#783): a
    # spline tangent to two lines at its two ends, each picked near its
    # end; a fit point spline closed by a click on its first point and
    # extruded as one profile; the saved file has both.
    add_test(NAME app.ui-windows-sketch-spline
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-spline-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-spline" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-spline PROPERTIES TIMEOUT 300)
    # Tables in Change Parameters (mitcad#667): a table made and typed in
    # (a wrong cell refused), a dimension through XLOOKUP with the Formula
    # Editor's completion of Screws., a cell edit the block follows, a CSV
    # imported through its preview.
    add_test(NAME app.ui-windows-tables
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-tables-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-tables"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-tables PROPERTIES TIMEOUT 300)
    # A 3D mouse (mitcad#951): no device in the VM logs "3D mouse: none";
    # the test driver's 6-DOF input turns, pans and zooms the view, and the
    # first button runs Fit (the script shared with the Linux test).
    add_test(NAME app.ui-windows-space-mouse
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-space-mouse-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-space-mouse"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-space-mouse PROPERTIES TIMEOUT 300)
    # Dogbone (mitcad#1011): the command on a pocket's body picked in the
    # browser, OK, four reliefs cut and Dogbone1 in the timeline.
    add_test(NAME app.ui-windows-dogbone
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-dogbone-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-dogbone"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-dogbone PROPERTIES TIMEOUT 300)
    # Mortise and Tenon (mitcad#1012): a tenon on a rail's end face and its
    # mortise in a post, both bodies changed, then edited.
    add_test(NAME app.ui-windows-mortise-tenon
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-mortise-tenon-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-mortise-tenon"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-mortise-tenon PROPERTIES TIMEOUT 300)
    # Constraints the drawn lines already satisfy, added when safe
    # (mitcad#1024): a quadrant's coincident and centre alignment, a
    # horizontal between level points, a tangent where a line touches a
    # circle, and a rectangle afterwards with nothing refused.
    add_test(NAME app.ui-windows-sketch-auto-constraints
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-auto-constraints-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-auto-constraints" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-auto-constraints PROPERTIES TIMEOUT 300)
    # The Sketch Palette's Create section (mitcad#1027): the defaults, the
    # constraints on two rows, nothing clipped, a tool started three times
    # there when the sketch opens again, its button starting it checked.
    add_test(NAME app.ui-windows-sketch-palette-create
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-palette-create-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-palette-create" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-palette-create PROPERTIES TIMEOUT 300)
    # Remembered values (mitcad#1028): a fillet and a chamfer with 3 mm,
    # OK; Fillet and Chamfer again start with 3 mm, also after a new start;
    # Cancel changes nothing. And Thread's settings kept by size
    # (mitcad#1298): two modelled threads, so a longer timeout.
    add_test(NAME app.ui-windows-remembered-values
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-remembered-values-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-remembered-values"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-remembered-values PROPERTIES TIMEOUT 900)
    # A construction plane of the active component on a face of another
    # component's turned occurrence (mitcad#1029): kept in the active one,
    # linked, its preview where the finished plane is shown.
    add_test(NAME app.ui-windows-construct-occurrence
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-construct-occurrence-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-construct-occurrence"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-construct-occurrence PROPERTIES TIMEOUT 300)
    # Default units for new designs (mitcad#1036): set to in, New is in
    # inches (Units: in, a typed extrude distance of 1 is an inch), the
    # untitled design at a new start too, an opened mm design keeps mm;
    # back to mm, New is in mm.
    add_test(NAME app.ui-windows-default-units
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-default-units-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-default-units"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-default-units PROPERTIES TIMEOUT 300)
    # The Configuration panel and the left column (mitcad#1020): the
    # Configuration card above the browser's, the browser's card down to the
    # timeline, the boundary dragged and remembered after a restart, a
    # double-click for the automatic split, the card collapsed.
    add_test(NAME app.ui-windows-configuration-panel
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-configuration-panel-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-configuration-panel"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-configuration-panel PROPERTIES TIMEOUT 300)
    # A material's filament preset per slicer (mitcad#1062): 3D Print warns
    # under the Material row of a material that names no preset of the
    # slicer, Bambu Studio's typed on Materials' Filament tab (Edit
    # materials… of the row's picker, mitcad#1141), the 3MF sent names
    # the filament by it and inherits from it.
    add_test(NAME app.ui-windows-filament-preset
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-filament-preset-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-filament-preset" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-filament-preset PROPERTIES TIMEOUT 300)
    # Filling a closed table (mitcad#1044): the Rows tab's Import CSV…
    # through its preview, Add Option… on the closed table with rows, and a
    # size on the Configuration bar that sets a parameter column (the test
    # driver's script).
    add_test(NAME app.ui-windows-configuration-rows
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-configuration-rows-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-configuration-rows" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-configuration-rows PROPERTIES TIMEOUT 360)
    # The Make tab (mitcad#1049): its groups and menus, the moved commands
    # from it, the Annotate group's kinds, the body card and the Browser's
    # markers; Floating too.
    add_test(NAME app.ui-windows-make-tab
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-make-tab-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-make-tab"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-make-tab PROPERTIES TIMEOUT 300)
    # Save Component As Design (mitcad#1057): the entry on the rows of the
    # design's own components only (not the root's, not a linked one's),
    # the dialog listing a projection of another component as frozen, the
    # file written under the component's name, and Replace with a link.
    add_test(NAME app.ui-windows-save-component
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-save-component-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-save-component" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-save-component PROPERTIES TIMEOUT 300)
    # The Material and Appearance tools (mitcad#1059): a library material
    # chosen in the picker for two bodies, OK and Ctrl+Z; a component's
    # default material; the fx bound to an option; the Appearance tool's
    # picker on a body and on a face; the windows without Assign; the
    # pickers' Edit Materials… and Edit Appearances… (mitcad#1415).
    add_test(NAME app.ui-windows-material-tool
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-material-tool-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-material-tool"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-material-tool PROPERTIES TIMEOUT 480)
    # The navigation bar at the bottom of the view (mitcad#1086), in the
    # Floating and the Docked layout: shown; Snap to Grid and Layout Grid
    # in step with the View menu's; Fit; Look At in a sketch looks at its
    # plane; the palette's Snap to grid; Preferences hides and shows it.
    add_test(NAME app.ui-windows-view-nav-bar
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-nav-bar-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-view-nav-bar"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-view-nav-bar PROPERTIES TIMEOUT 300)
    # The message band and the message history (mitcad#1042): an error in
    # the band above the timeline stays until it is closed or the next
    # command starts, a hint fades to the stub, the history's row of a
    # feature selects it; the Docked status bar's history button.
    add_test(NAME app.ui-windows-message-band
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-message-band-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-message-band"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-message-band PROPERTIES TIMEOUT 300)
    # Convert Mesh (mitcad#1082): an inserted cube's mesh body made a
    # faceted solid by the panel's defaults, the browser listing the solid.
    add_test(NAME app.ui-windows-convert-mesh
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-convert-mesh-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-convert-mesh"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-convert-mesh PROPERTIES TIMEOUT 300)
    # File > Print... (mitcad#1089): disabled with nothing to print, enabled
    # in a sketch, its panel opened and cancelled; a 300 x 200 mm sketch at
    # 1:1 printed to a PDF file in place of the system's dialog (two A4
    # pages), fit to page (one).
    add_test(NAME app.ui-windows-paper-print
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-paper-print-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-paper-print"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-paper-print PROPERTIES TIMEOUT 300)
    # Emboss by projection (mitcad#1091): two letters of a text projected
    # onto a ball and raised, previewed and made; a rectangle wrapped onto a
    # rod moved by its Along Axis arrow and by Along Axis, Around and
    # Rotation typed (mitcad#1259).
    add_test(NAME app.ui-windows-emboss
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-emboss-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-emboss"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-emboss PROPERTIES TIMEOUT 420)
    # The Center of Mass marker (mitcad#1094): drawn at the properties
    # query's total, moved with a component, hidden by its row's light bulb,
    # kept hidden in the saved design, deleted from the row's menu.
    add_test(NAME app.ui-windows-center-of-mass
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-center-of-mass-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-center-of-mass" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-center-of-mass PROPERTIES TIMEOUT 300)
    # The quick-access group (mitcad#1090): the default commands in the
    # Docked toolbar and the Floating title row, Save disabled for an
    # unchanged saved design and enabled after an edit, View > Look At
    # added from the group's menu, read back, Reset to Default.
    add_test(NAME app.ui-windows-quick-access
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-quick-access-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-quick-access" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-quick-access PROPERTIES TIMEOUT 300)
    # Comments for AI agents (mitcad#1119): Add Comment on a face, its pin
    # and its row in the Comments panel, addressed by an agent through the
    # live MCP bridge, resolved in the panel, the pins hidden and shown,
    # opened again in the Floating layout.
    add_test(NAME app.ui-windows-comments
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-comments-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-comments" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-comments PROPERTIES TIMEOUT 300)
    # Face-bounded profiles without Project and the ordinary whole-face
    # picker, from a synthetic fixture with queried region keys (mitcad#1138).
    if(TARGET test_face_profiles)
      add_test(NAME app.ui-windows-face-bounded-profiles
        COMMAND powershell -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-face-bounded-profiles-test.ps1"
                -App "$<TARGET_FILE:mitcad>" -Fixture "$<TARGET_FILE:test_face_profiles>"
                -Out "${CMAKE_BINARY_DIR}/ui-windows-face-bounded-profiles"
                -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
      set_tests_properties(app.ui-windows-face-bounded-profiles PROPERTIES TIMEOUT 600)
    endif()
    # Features on a fixed plane (mitcad#1137): every type Place On places
    # opens for editing with the plane listed "(fixed)"; a new size keeps
    # the plane, a face picked in Place On replaces it.
    add_test(NAME app.ui-windows-fixed-plane
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fixed-plane-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-fixed-plane"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-fixed-plane PROPERTIES TIMEOUT 300)
    # Typed polygon radii/first-edge lengths, sides-only input and atomic
    # Undo/Redo (mitcad#1136), shared with the Linux and macOS driver.
    add_test(NAME app.ui-windows-sketch-polygon-dimensions
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-polygon-dimensions-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-polygon-dimensions"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-polygon-dimensions PROPERTIES TIMEOUT 300)
    # Selection Delete, dependency union, Cancel and one-step Undo (mitcad#1139).
    add_test(NAME app.ui-windows-delete-feature-selection
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-delete-feature-selection-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-delete-feature-selection"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-delete-feature-selection PROPERTIES TIMEOUT 300)
    # Send to Slicer's material rows (mitcad#1141): a body without a
    # material says so, a material chosen in the row's picker is the
    # body's at once (one undo step), a row per filament changed alone,
    # Edit materials… opens the Materials window on the row's material.
    add_test(NAME app.ui-windows-print-material-picker
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-material-picker-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-print-material-picker"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-print-material-picker PROPERTIES TIMEOUT 300)
    # A click on a manipulator's knob over the orientation cube (mitcad#1105):
    # Fillet's arrow panned onto the cube's top face and clicked without a
    # drag keeps the click, the camera unchanged; without the handle the
    # cube turns the view.
    add_test(NAME app.ui-windows-knob-over-cube
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-knob-over-cube-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-knob-over-cube"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-knob-over-cube PROPERTIES TIMEOUT 300)
    # A design a newer MitCad wrote (mitcad#1132): the warning on opening
    # it in both layouts, and the question before Save writes over it
    # (Cancel, Save As, Save and Drop Them).
    add_test(NAME app.ui-windows-unknown-fields
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-unknown-fields-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-unknown-fields"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-unknown-fields PROPERTIES TIMEOUT 300)
    # Send to Slicer's Use the open slicer window (mitcad#1162): with fake
    # slicers, two sends of a design start one window and hand it the
    # second file; another design starts its own; the box off starts a
    # window per send; a program without a hand-over is brought forward.
    add_test(NAME app.ui-windows-slicer-window
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-slicer-window-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-slicer-window"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-slicer-window PROPERTIES TIMEOUT 300)
    # Same synthetic planar Measure workflow as Linux/macOS (mitcad#1161).
    add_test(NAME app.ui-windows-measure-planar
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-measure-planar-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-measure-planar"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-measure-planar PROPERTIES TIMEOUT 180)
    # Sketch Move/Copy starts at 0 (mitcad#1163): opened on a selected line
    # it leaves the line where it is until a distance is typed; the Linux
    # test's shared input script.
    add_test(NAME app.ui-windows-sketch-move-zero
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-move-zero-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-move-zero"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-move-zero PROPERTIES TIMEOUT 300)
    # Lengths in the design's unit (mitcad#1165), the steps of
    # tools/ui-units-inch-test.sh: in a design in inches the sheet-rules
    # row, Extrude's default distance and Measure in inches.
    add_test(NAME app.ui-windows-units-inch
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-units-inch-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-units-inch"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-units-inch PROPERTIES TIMEOUT 180)
    # Measure picks a face that carries an annotation, drawn opaque
    # (mitcad#1166), as on Linux.
    add_test(NAME app.ui-windows-measure-annotated-face
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-measure-annotated-face-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-measure-annotated-face"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-measure-annotated-face PROPERTIES TIMEOUT 180)
    # Another open document selected through its real list; save/cancel,
    # independent insertion undo and self-insertion exclusion (mitcad#1172).
    add_test(NAME app.ui-windows-insert-open-design
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-insert-open-design-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-insert-open-design"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-insert-open-design PROPERTIES TIMEOUT 420)
    # Dimension spacing (mitcad#1173), the steps of
    # tools/ui-drawing-dimension-spacing-test.sh: a dimension placed or
    # dragged snaps to the spacing of the one before and its multiples, Alt
    # places freely.
    add_test(NAME app.ui-windows-drawing-spacing
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-spacing-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-spacing"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-drawing-spacing PROPERTIES TIMEOUT 300)
    # Hole's keyboard and its dimensions in the view (mitcad#1181), as
    # tools/ui-hole-entry-test.sh: Tab from Distance 2 to Diameter and back,
    # Distance 2 and Depth typed in their value boxes (Enter, Esc, a text
    # that is not a value refused), and a reference picked in its row
    # (mitcad#1285: the distances evaluated, the hole where it was).
    add_test(NAME app.ui-windows-hole-entry
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-hole-entry-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-hole-entry"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-hole-entry PROPERTIES TIMEOUT 450)
    # Sketch Fillet at a corner of a line and an arc picked by its corner
    # point (mitcad#1192); the Linux test's shared input script.
    add_test(NAME app.ui-windows-sketch-fillet-arc
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-fillet-arc-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-fillet-arc"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-fillet-arc PROPERTIES TIMEOUT 300)
    # Plates… (mitcad#1050), the Linux test's shared input script: two
    # plates added, a body picked onto Plate 2 in vase mode, the plate
    # drawn beside the design; Send to Slicer writes one 3MF of both
    # plates, the spiral mode on Plate 2 only.
    add_test(NAME app.ui-windows-print-plates
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-plates-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-print-plates"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-print-plates PROPERTIES TIMEOUT 420)
    # The sketch of a component whose first occurrence is hidden, edited
    # where its shown one places it, its curves drawn (mitcad#1194); the
    # Linux test's shared input script.
    add_test(NAME app.ui-windows-hidden-occurrence-sketch
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-hidden-occurrence-sketch-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-hidden-occurrence-sketch"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-hidden-occurrence-sketch PROPERTIES TIMEOUT 300)
    # Materials' mechanical and thermal properties (mitcad#1213), as the Linux
    # test: a value typed in psi in an inch design, a material of the
    # bundled dataset created and saved, a CSV imported with its invalid
    # row reported.
    add_test(NAME app.ui-windows-material-properties
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-material-properties-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-material-properties"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-material-properties PROPERTIES TIMEOUT 300)

    # A My library material whose file lacks its own color (mitcad#1242):
    # the Materials window renames it, the new name listed and its own
    # color made again in the file (the Linux test's shared driver script).
    add_test(NAME app.ui-windows-library-material-own-colour
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-library-material-own-colour-test.ps1"
              -App "$<TARGET_FILE:mitcad>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-library-material-own-colour"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-library-material-own-colour PROPERTIES TIMEOUT 300)

    # Processes grouped by printer (mitcad#1216): the Processes manager's
    # sub-groups, Send to Slicer's Process picker with the chosen printer's
    # processes first, the body card's drop-down; the Linux test's shared
    # input script.
    add_test(NAME app.ui-windows-processes-by-printer
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-processes-by-printer-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-processes-by-printer"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-processes-by-printer PROPERTIES TIMEOUT 360)
    # The arrow keys turn the view while it has the keyboard, not from a
    # value field (mitcad#1211); the Linux test's shared input script.
    add_test(NAME app.ui-windows-view-arrow-keys
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-arrow-keys-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-view-arrow-keys"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-view-arrow-keys PROPERTIES TIMEOUT 180)
    # Look At in the view's context menu for a planar face of a component,
    # not for a cylindrical face (mitcad#1212); the Linux test's shared
    # input script.
    add_test(NAME app.ui-windows-context-look-at
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-context-look-at-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-context-look-at"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-context-look-at PROPERTIES TIMEOUT 180)
    # MitCad's version in the main window (mitcad#1209): the watermark's text
    # and the window title, its place in the view's lower right (above the
    # timeline in the Floating layout), a click there reaches the view, and
    # Preferences › General hides and shows it.
    add_test(NAME app.ui-windows-version-watermark
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-version-watermark-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-version-watermark"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>" -Version "${PROJECT_VERSION}")
    set_tests_properties(app.ui-windows-version-watermark PROPERTIES TIMEOUT 300)
    # A sketch region inside a body picked before the body's face in front
    # of it while an input takes profiles (mitcad#1222); the Linux test's
    # shared input script.
    add_test(NAME app.ui-windows-region-pick-inside-body
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-region-pick-inside-body-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-region-pick-inside-body"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-region-pick-inside-body PROPERTIES TIMEOUT 240)
    # Auto Fix in the timeline's context menu (mitcad#1223): an extrusion
    # whose region a sketch change split is fixed as one undo step and its
    # warning goes; the Linux test's shared input script.
    add_test(NAME app.ui-windows-timeline-auto-fix
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-timeline-auto-fix-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-timeline-auto-fix"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-timeline-auto-fix PROPERTIES TIMEOUT 180)
    # A Center to Center Slot's typed length and width become its
    # dimensions, so that only its place is free (mitcad#1225); the Linux
    # test's shared input script and mitcad-cli's check.
    add_test(NAME app.ui-windows-sketch-slot-dimensions
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-slot-dimensions-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-slot-dimensions"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-slot-dimensions PROPERTIES TIMEOUT 240)
    # Extrude (E) from a zoomed sketch on a face: the sketch and its body in
    # the free area (mitcad#1227), a drawn profile rather than the face's
    # leftover region (mitcad#1226); the Linux test's shared input script.
    add_test(NAME app.ui-windows-extrude-from-sketch
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-from-sketch-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-from-sketch"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-extrude-from-sketch PROPERTIES TIMEOUT 240)
    # The sketch constraints' Shift keys (mitcad#1229): Shift+P adds
    # Parallel to two selected lines, Shift+V makes a line vertical, and
    # Shift+P in a dimension's value field types P; the Linux test's shared
    # input script.
    add_test(NAME app.ui-windows-sketch-constraint-shortcuts
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-constraint-shortcuts-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-constraint-shortcuts"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-constraint-shortcuts PROPERTIES TIMEOUT 240)
    # A section that shows a body whole because its clip failed is reported
    # in the message band and the Analysis folder's row, withdrawn when the
    # analysis is hidden; a plane that cuts nothing is told (mitcad#1230);
    # the Linux test's shared input script.
    add_test(NAME app.ui-windows-section-clip-report
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-section-clip-report-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-section-clip-report"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-section-clip-report PROPERTIES TIMEOUT 240)
    # A cable gland body with modelled threads cut by Section Analysis in
    # its profile's plane and 1 mm off it, with caps (mitcad#1230); the
    # Linux test's shared input script.
    add_test(NAME app.ui-windows-section-gland
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-section-gland-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-section-gland"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-section-gland PROPERTIES TIMEOUT 360)
    # Sketch Offset of a hexagon whose 2 mm corner arcs vanish at 2 mm
    # inward (mitcad#1234): the preview and OK without an error, the note
    # naming the arcs; the Linux test's shared input script.
    add_test(NAME app.ui-windows-sketch-offset-vanishing
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-offset-vanishing-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-offset-vanishing"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-offset-vanishing PROPERTIES TIMEOUT 300)
    # A Join made in the Extrude panel on a plate's end: every face of the
    # joined body meshed where the view draws it, the plate's top (its mesh
    # taken from the plate as shown) pickable (GitHub #126, mitcad#1243);
    # the Linux test's shared input script.
    add_test(NAME app.ui-windows-join-mesh-reuse
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-join-mesh-reuse-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-join-mesh-reuse"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-join-mesh-reuse PROPERTIES TIMEOUT 240)
    # The Docked toolbar's colours in the dark and the light theme: the
    # Classic ribbon's body and the bar's Undo, Redo, separator and Chat
    # buttons on the toolbar's surface (GitHub #127, mitcad#1245); the Linux
    # test's shared input script.
    add_test(NAME app.ui-windows-docked-toolbar-colors
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-toolbar-colors-test.ps1"
              -App "$<TARGET_FILE:mitcad>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-toolbar-colors"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-docked-toolbar-colors PROPERTIES TIMEOUT 180)
    # The Docked layout's Command and Projects docks hidden from the
    # window's menu of its panels and with their close buttons, shown again
    # from the View menu; no Configuration in that menu (mitcad#1455); the
    # Linux test's shared input script.
    add_test(NAME app.ui-windows-docked-panels
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-panels-test.ps1"
              -App "$<TARGET_FILE:mitcad>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-panels"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-docked-panels PROPERTIES TIMEOUT 180)
    # Up, Down and the mouse wheel step a value field's number (GitHub
    # mitcad/Mitcad#129, mitcad#1254): an expression left alone, Shift+Up
    # by 10 mm, Circular Pattern's Quantity 4 to 5 previewed, the wheel only
    # over the field with the keyboard; the Linux test's shared input script.
    add_test(NAME app.ui-windows-value-field-step
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-value-field-step-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-value-field-step"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-value-field-step PROPERTIES TIMEOUT 240)
    # The Start tab (GitHub mitcad/Mitcad#125, mitcad#1258): three saved
    # designs with their previews on the Start page at launch, a card that
    # opens its design, the star and Pinned, the search, New Design in the
    # default units, and a start without the page once Show start page at
    # launch is off; the Linux test's shared input scripts.
    add_test(NAME app.ui-windows-start-page
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-start-page-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-start-page"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-start-page PROPERTIES TIMEOUT 300)
    # Editing an extrusion whose profile was deleted from its sketch: the
    # missing profile is left out and a new one picked (mitcad#1264).
    add_test(NAME app.ui-windows-deleted-profile-edit
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-deleted-profile-edit-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-deleted-profile-edit"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-deleted-profile-edit PROPERTIES TIMEOUT 300)
    # Browser eyes while a command is open (mitcad#1263): Section1's eye
    # during a modelled Thread clips its preview and OK adds the thread (the
    # eye an undo step of its own before it); Esc after the eye keeps the
    # section; the bodies' eyes during Fillet keep its selection; the Linux
    # test's shared input script.
    add_test(NAME app.ui-windows-browser-eyes-mid-command
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-eyes-mid-command-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-eyes-mid-command"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-browser-eyes-mid-command PROPERTIES TIMEOUT 600)
    # Notice places (mitcad#1327): a rounding's warning in plain words, its
    # places painted while the pointer is over it on the Timeline item, in
    # the message band and in the command panel's message; the Linux
    # test's shared input script.
    add_test(NAME app.ui-windows-notice-places
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-notice-places-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-notice-places"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-notice-places PROPERTIES TIMEOUT 360)
    # The rolled-back features evaluated in the background after an edit
    # before the marker, so that the marker dragged to the end evaluates
    # nothing; an edit that opens cancels a run at once (mitcad#1267); the
    # Linux test's shared input script.
    add_test(NAME app.ui-windows-background-evaluation
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-background-evaluation-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-background-evaluation"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-background-evaluation PROPERTIES TIMEOUT 300)
    # A linked component shown by its envelope (mitcad#1271): Show as
    # Envelope... at 20 mm in a linked machine's browser menu, one body
    # shown instead of three; Show in Full, the three again; undo.
    add_test(NAME app.ui-windows-envelope
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-envelope-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-envelope"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-envelope PROPERTIES TIMEOUT 300)
    # The Dimension tool's picks choosing the dimension, the Centerline of
    # a curved slot and the Arc tool's snap on a plate like the report's
    # (mitcad#1272).
    add_test(NAME app.ui-windows-drawing-dimension-picks
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-dimension-picks-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-dimension-picks"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-drawing-dimension-picks PROPERTIES TIMEOUT 300)
    # Corners on drawings (mitcad#1338): a corner dimensioned to a hole's
    # centre line in both pick orders, Always show corners' dots from the
    # settings, the PDF and SVG the same with them on and off.
    add_test(NAME app.ui-windows-drawing-corners
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-corners-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-corners"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-drawing-corners PROPERTIES TIMEOUT 300)
    # A section's cut edge dimensioned alone (mitcad#1361): the piece beside
    # a hole picked alone reads its own length, a corner waits for a second
    # pick.
    add_test(NAME app.ui-windows-drawing-section-dimension
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-section-dimension-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-section-dimension"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-drawing-section-dimension PROPERTIES TIMEOUT 300)
    # A section's line and a break's sides snap (mitcad#1362): onto a hole's
    # centre and its centre line, a second click turned to 45 degrees,
    # Shift as clicked, a break's sides onto points.
    add_test(NAME app.ui-windows-drawing-section-snap
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-section-snap-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-section-snap"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-drawing-section-snap PROPERTIES TIMEOUT 300)
    # Names typed in the Projects panel (mitcad#1268): the proposed name free
    # on disk, also while a remote fetch runs; a refused name keeps the
    # editor with its message; the collapsed card expanded and the hidden
    # dock shown for the name, then put back; the Linux test's shared input
    # scripts.
    add_test(NAME app.ui-windows-projects-naming
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-projects-naming-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-projects-naming"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-projects-naming PROPERTIES TIMEOUT 420)
    # The Envelope command (mitcad#1276): Envelope of a component at 20 mm
    # in the Docked layout, edited and undone, and in the Floating layout;
    # Show as Envelope... on one of two occurrences of a linked machine
    # leaves the other in full.
    add_test(NAME app.ui-windows-envelope-command
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-envelope-command-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-envelope-command"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-envelope-command PROPERTIES TIMEOUT 420)
    # Hidden built-ins (mitcad#1270): a built-in printer hidden in the
    # Printers manager is gone from it and from Send to Slicer's Printer
    # box, comes back grayed with Show hidden and Unhide restores it; a
    # design that uses it keeps it; with a printer in My library Built-in
    # starts folded. The Linux test is tools/ui-hide-built-ins-test.sh.
    add_test(NAME app.ui-windows-hide-built-ins
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-hide-built-ins-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-hide-built-ins"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-hide-built-ins PROPERTIES TIMEOUT 300)
    # A pattern of bodies drawn from its body's meshes, and window
    # selections over many bodies decided by their boxes first
    # (mitcad#1275).
    add_test(NAME app.ui-windows-window-bodies
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-window-bodies-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-window-bodies"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-window-bodies PROPERTIES TIMEOUT 300)
    # Windows 10's dark app mode (mitcad#1282, GitHub mitcad/Mitcad#138):
    # with the Windows 10 path forced over its default style, the Start
    # page and the Browser dark in the dark app mode and the Start page
    # light in the light one, Docked and Floating; without it, Windows 11's
    # own style as before.
    add_test(NAME app.ui-windows-windows10-dark
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-windows10-dark-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-windows10-dark"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-windows10-dark PROPERTIES TIMEOUT 300)
    # Browse Components (mitcad#1283, GitHub #136): Tools › Libraries lists
    # it once; Library Sources… from the browser; a component of a library
    # an index names shows Get Library, stays selected after the fetch and
    # is inserted; Installed, Community and Favorites; Insert from Library
    # and Community Library open the same window; the Floating layout. The
    # Linux test is tools/ui-browse-components-test.sh.
    add_test(NAME app.ui-windows-browse-components
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browse-components-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-browse-components"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-browse-components PROPERTIES TIMEOUT 360)
    # A body's Browser row and View › Show (mitcad#1287, GitHub
    # mitcad/Mitcad#142): Delete on a row of an imported base feature's
    # body removes only that body (Docked); View › Show › Construction,
    # Sketches and Origins hide and show them in two components without
    # changing a light bulb (Floating). The Linux test is
    # tools/ui-view-show-test.sh.
    add_test(NAME app.ui-windows-view-show
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-show-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-view-show" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-view-show PROPERTIES TIMEOUT 300)
    # Delete on the Start page (mitcad#1313): with Sketch1 selected, the
    # Start tab clears the selection, and Delete and Backspace on the page
    # and on its New Design card delete nothing of the design behind it, in
    # the Floating and the Docked layout. The Linux test is
    # tools/ui-start-page-delete-test.sh.
    add_test(NAME app.ui-windows-start-page-delete
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-start-page-delete-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-start-page-delete"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-start-page-delete PROPERTIES TIMEOUT 300)
    # Preferences > Display > Theme (mitcad#1330, GitHub mitcad/Mitcad#151):
    # started with Theme Dark on the light app mode, the Start page, the
    # Browser's card, the Browser and the timeline dark; Light and Dark from
    # Preferences at once; System follows the app mode again; Windows 11's
    # style (Floating, Docked) and Windows 10's path forced. The Linux test
    # is tools/ui-theme-preference-test.sh.
    add_test(NAME app.ui-windows-theme-preference
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-theme-preference-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-theme-preference"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-theme-preference PROPERTIES TIMEOUT 600)
    # The interface language (mitcad#1460): English by default and with
    # --language en, Finnish with --language fi (the tabs, the command search
    # by the English name, the Language preference), the pseudo-language
    # with no group caption cut. The Linux test is tools/ui-language-test.sh.
    add_test(NAME app.ui-windows-language
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-language-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-language"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-language PROPERTIES TIMEOUT 600)
    # Keys while a pasted component is still computed (mitcad#1360, GitHub
    # mitcad/Mitcad#156, a crash of 0.47.0 here): Paste New opens
    # Move/Copy, and Shift and Up in its X Distance field while the preview
    # computes neither read the document nor end the application; OK places
    # the copy once the job has ended. The Linux test's shared input script.
    add_test(NAME app.ui-windows-paste-busy-keys
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-paste-busy-keys-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-paste-busy-keys"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-paste-busy-keys PROPERTIES TIMEOUT 240)
    # Edit Text from inside a letter (mitcad#1369, GitHub mitcad/Mitcad#158):
    # zoomed in, a right-click and a double-click inside the I's stroke and
    # the o's stroke and counter offer and open the text's Edit Text, and a
    # QR code's finder pattern (its dark centre and the light ring round it)
    # Edit QR Code. The Linux test is
    # tools/ui-text-region-edit-test.sh.
    add_test(NAME app.ui-windows-text-region-edit
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-text-region-edit-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-text-region-edit"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-text-region-edit PROPERTIES TIMEOUT 300)
    # The Make card's Send to Slicer (mitcad#1377), the Linux test's shared
    # input script: with a face of one of two shown bodies selected, the
    # card's button opens Send to Slicer's panel with that body alone, and
    # Send writes its file only.
    add_test(NAME app.ui-windows-make-card-send
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-make-card-send-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-make-card-send"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-make-card-send PROPERTIES TIMEOUT 300)
    # The ASTM A500 tubes in BUILD's part picker (mitcad#1372, GitHub
    # mitcad/Mitcad#159): the parts library's A500 tubes made by
    # make-profile-library.py and fetched; the search A500 lists them all,
    # HSS 3x3x3/16, 2x2x1/8 and 4x2x3/16 found. The Linux test is
    # tools/ui-tube-picker-test.sh. Needs Python for the generator.
    find_package(Python3 COMPONENTS Interpreter QUIET)
    if(Python3_Interpreter_FOUND)
      add_test(NAME app.ui-windows-tube-picker
        COMMAND powershell -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-tube-picker-test.ps1"
                -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>" -Python "${Python3_EXECUTABLE}"
                -Out "${CMAKE_BINARY_DIR}/ui-windows-tube-picker"
                -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
      set_tests_properties(app.ui-windows-tube-picker PROPERTIES TIMEOUT 600)
    endif()
    # The Line tool's arcs and the Midpoint Line (mitcad#1393): a drag along
    # a line from its end draws a tangent arc from it, a drag across a
    # chain's line a normal arc with a typed radius dimensioned, and a
    # Midpoint Line drawn with two clicks has its middle at the first. The
    # Linux test is tools/ui-sketch-line-arcs-test.sh.
    add_test(NAME app.ui-windows-sketch-line-arcs
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-line-arcs-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-line-arcs" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-line-arcs PROPERTIES TIMEOUT 300)
    # The mouse wheel over Fillet's Radius while its preview computes
    # (mitcad#1407, GitHub mitcad/Mitcad#162): each notch steps the field
    # without reading the document, the last value is previewed once the
    # job ends and OK makes that fillet. The Linux test's shared input
    # script (tools/ui-fillet-wheel-busy-test.sh).
    add_test(NAME app.ui-windows-fillet-wheel-busy
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-wheel-busy-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-wheel-busy"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-fillet-wheel-busy PROPERTIES TIMEOUT 240)
    # The part of GitHub mitcad/Mitcad#173 (mitcad#1422) built through the
    # UI (a slot, a circle on its end, two extrusions) and the plate's top
    # edge rounded into the boss with Tangent Chain: a failing radius's error
    # in the panel, 3 mm valid; then a fillet crashing inside OCCT
    # (MITCAD_TEST_OCCT_CRASH=fillet) is an error and the application goes
    # on. The Linux test's shared input scripts
    # (tools/ui-fillet-boss-tangent-test.sh).
    add_test(NAME app.ui-windows-fillet-boss-tangent
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-boss-tangent-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-boss-tangent"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-fillet-boss-tangent PROPERTIES TIMEOUT 600)
    # Linked designs while the holder is open (mitcad#1416): the notice with
    # Open Linked Design on an edit of a linked component, the linked design
    # opened in a tab of its own, "<file> changed" with Update when it is
    # saved in its tab, Update Link of the context menu. The Linux test is
    # tools/ui-linked-design-test.sh.
    add_test(NAME app.ui-windows-linked-design
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-linked-design-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-linked-design" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-linked-design PROPERTIES TIMEOUT 420)
    # The Flange panel's Corner (mitcad#1410): Open loaded and kept, Half
    # Open chosen, saved and read back, None showing Measured again. The
    # Linux test is tools/ui-sheet-flange-corner-test.sh.
    add_test(NAME app.ui-windows-sheet-corner
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-corner-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-corner" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sheet-corner PROPERTIES TIMEOUT 300)
    # Preferences > Display > Small circles (mitcad#879): Adaptive by
    # default; Full Detail kept in the settings and the open design's body
    # meshed again with more triangles; back to Adaptive, fewer. The Linux
    # test is tools/ui-small-circles-test.sh.
    add_test(NAME app.ui-windows-small-circles
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-small-circles-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-small-circles"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-small-circles PROPERTIES TIMEOUT 300)
    # Ctrl held alone hides a command's preview while its inputs are picked
    # (mitcad#1421): a new Extrude gains a fourth circle and Fillet1 edited
    # from the timeline an edge while Ctrl hides their previews; Ctrl+Z and
    # Ctrl+S show the preview again at once. The Linux test is
    # tools/ui-ctrl-hides-preview-test.sh.
    add_test(NAME app.ui-windows-ctrl-hides-preview
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-ctrl-hides-preview-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-ctrl-hides-preview" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-ctrl-hides-preview PROPERTIES TIMEOUT 300)
    # A failed preview's message opens at its first line (mitcad#1430): a
    # cube's Extrude edited with its distance 0 in a 1280 x 800 window,
    # Docked and Floating, the panel's error box not scrolled down its text.
    # The Linux test's shared input script
    # (tools/ui-panel-message-top-test.sh).
    add_test(NAME app.ui-windows-panel-message-top
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-panel-message-top-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-panel-message-top"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-panel-message-top PROPERTIES TIMEOUT 300)
    # Fillet on the slot-and-boss part (mitcad#1429, GitHub mitcad/Mitcad#173):
    # the plate's top edge that runs tangentially into the boss, previewed at
    # 3, 6 and 1 mm, Tangent Chain off and on, the edges hovered, Enter and
    # Escape; the application stays up whatever the fillet gives.
    add_test(NAME app.ui-windows-fillet-boss-tangent-crash
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-boss-tangent-crash-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-boss-tangent-crash"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-fillet-boss-tangent-crash PROPERTIES TIMEOUT 600)
    # The render worker while a section analysis is shown and hidden in the
    # rendered view (mitcad#1432): twelve times while Cycles traces on the
    # CPU, the worker never stopping. The Linux test's shared input script
    # (tools/ui-render-section-toggle-test.sh).
    add_test(NAME app.ui-windows-render-section-toggle
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-render-section-toggle-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-render-section-toggle"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-render-section-toggle PROPERTIES TIMEOUT 420)
    # The ribbon, the Browser and the navigation bar with the design in
    # front (mitcad#1451, GitHub mitcad/Mitcad#185): a member placed with
    # Build > Profile, a second design and back, Docked and Floating; the
    # window's menu of its panels offers no Toolbar. The Linux test's shared
    # input script (tools/ui-docked-ribbon-test.sh).
    add_test(NAME app.ui-windows-docked-ribbon
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-ribbon-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-ribbon"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-docked-ribbon PROPERTIES TIMEOUT 600)
    # Anaglyph 3D (mitcad#1449): View > Anaglyph 3D's stereo camera, a pick
    # in it, Preferences > Display's stereo depth and convergence, and the
    # projection back when it ends. The Linux test's shared input script
    # (tools/ui-view-anaglyph-test.sh).
    add_test(NAME app.ui-windows-view-anaglyph
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-anaglyph-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-view-anaglyph"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-view-anaglyph PROPERTIES TIMEOUT 180)
    # A pick on Fillet's preview names the edge before the fillet it comes
    # from (mitcad#1458, GitHub mitcad/Mitcad#177): Fillet1, rounding a
    # block's end foot edges on a plate, edited; the front's foot edge
    # between the roundings, clicked in the preview without Ctrl, joins
    # Edges and the three roundings warn of nothing hanging out. The Linux
    # test's shared input script (tools/ui-fillet-preview-pick-test.sh).
    add_test(NAME app.ui-windows-fillet-preview-pick
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-fillet-preview-pick-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-fillet-preview-pick"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-fillet-preview-pick PROPERTIES TIMEOUT 300)
    # Lengths typed in a design in inches are stored as inches (mitcad#1452,
    # GitHub mitcad/Mitcad#187): Add User Parameter, a dimension made in mm,
    # a rectangle and a circle by typed values. The Linux test's shared
    # input script (tools/ui-sketch-inch-entry-test.sh).
    add_test(NAME app.ui-windows-sketch-inch-entry
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-inch-entry-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-inch-entry"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-inch-entry PROPERTIES TIMEOUT 180)
    # A sketch dimension's value that uses its own parameter keeps that
    # parameter as a user parameter (mitcad#1535): BuckleDiameter * 2 in the
    # dimension of BuckleDiameter, and Move to User Parameters from the
    # context menu. The Linux test's shared input script
    # (tools/ui-sketch-own-parameter-test.sh).
    add_test(NAME app.ui-windows-sketch-own-parameter
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-own-parameter-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-own-parameter"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-sketch-own-parameter PROPERTIES TIMEOUT 180)
    # A command panel value that uses the field's own parameter keeps that
    # parameter as a user parameter (mitcad#1536): depth * 2 in Edit
    # Extrude's Distance of depth. The Linux test's shared input script
    # (tools/ui-feature-own-parameter-test.sh).
    add_test(NAME app.ui-windows-feature-own-parameter
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-feature-own-parameter-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-feature-own-parameter"
              -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-feature-own-parameter PROPERTIES TIMEOUT 180)
  endif()
elseif(WIN32 AND CMAKE_RUNTIME_OUTPUT_DIRECTORY)
  # Turned off again: remove earlier copies, or they would stay in use.
  foreach(dll IN LISTS mesa_runtime_dlls)
    file(REMOVE "${CMAKE_RUNTIME_OUTPUT_DIRECTORY}/${dll}")
  endforeach()
endif()
