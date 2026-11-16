# SPDX-License-Identifier: MIT
# The default, pin, reorder, unpin and the settings read back: plain Qt
# Core.
target_sources(test_app PRIVATE tests/navigation_bar_pins.cpp framework/NavigationBarPins.cpp
               framework/NavigationBarPins.hpp)
