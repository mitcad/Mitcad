# SPDX-License-Identifier: MIT
# The sub-spindle as Simulate draws it (mitcad#2405): plain Qt Core, with
# cam_toolpaths.cmake's cam/CamToolpaths.cpp.
target_sources(test_app PRIVATE tests/cam_sub_drawn.cpp)
