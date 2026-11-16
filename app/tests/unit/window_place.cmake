# SPDX-License-Identifier: MIT
# A kept frame fitted to the screens (a removed monitor, half
# off-screen, larger than the screen, the title bar above the top), the
# cascade, and the place in the settings: plain Qt Core and Gui.
target_sources(test_app PRIVATE tests/window_place.cpp framework/WindowPlace.cpp framework/WindowPlace.hpp)
