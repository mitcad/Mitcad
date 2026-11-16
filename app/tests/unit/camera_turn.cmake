# SPDX-License-Identifier: MIT
# The axis, the angle, the centre and the zoom on OCCT's cameras.
target_sources(test_app PRIVATE tests/camera_turn.cpp view/CameraTurn.hpp)
target_link_libraries(test_app PRIVATE TKService)
