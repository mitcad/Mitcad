# SPDX-License-Identifier: MIT
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
