# SPDX-License-Identifier: MIT
# The reports' parsing, the mapping to the camera and the controller with
# a backend of the test's own (plain Qt Core and Gui), and the camera's step on
# an OCCT camera as OcctViewer applies it (Graphic3d_Camera: TKService), and
# a motion's pacing and timing from given frame times (mitcad#1023).
target_sources(test_app PRIVATE tests/space_mouse.cpp view/SpaceMouseCamera.hpp view/SpaceMouseController.cpp
               view/SpaceMouseController.hpp view/SpaceMouseTiming.hpp)
target_link_libraries(test_app PRIVATE TKService)
