# SPDX-License-Identifier: MIT
# The Docked layout's floating panels at the next start (mitcad#2070): kept
# where their kept frame is wholly on their connected screen, docked when
# the screen is gone, the frame half off it or across two screens: plain Qt
# Core and Gui.
target_sources(test_app PRIVATE tests/floating_panel_screens.cpp framework/WindowPlace.cpp framework/WindowPlace.hpp)
