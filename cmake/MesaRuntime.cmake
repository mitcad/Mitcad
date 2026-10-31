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
    # browser's Configuration bar changes the body (the test driver's
    # script, shared with the macOS test).
    add_test(NAME app.ui-windows-configurations
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-configurations-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-configurations" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-configurations PROPERTIES TIMEOUT 360)
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

    # My Library (mitcad#704): a built-in material duplicated into My
    # library, renamed, its density changed and deleted, a printer
    # duplicated, renamed and deleted, each in the library's files; the
    # open design unchanged.
    add_test(NAME app.ui-windows-my-library
      COMMAND powershell -NoProfile -ExecutionPolicy Bypass
              -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-my-library-test.ps1"
              -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
              -Out "${CMAKE_BINARY_DIR}/ui-windows-my-library" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
    set_tests_properties(app.ui-windows-my-library PROPERTIES TIMEOUT 300)

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
  endif()
elseif(WIN32 AND CMAKE_RUNTIME_OUTPUT_DIRECTORY)
  # Turned off again: remove earlier copies, or they would stay in use.
  foreach(dll IN LISTS mesa_runtime_dlls)
    file(REMOVE "${CMAKE_RUNTIME_OUTPUT_DIRECTORY}/${dll}")
  endforeach()
endif()
