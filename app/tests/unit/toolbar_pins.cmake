# SPDX-License-Identifier: MIT
# Pin, reorder, unpin, each pin's group and the settings read back
# (also an earlier version's per-tab lists): plain Qt Core.
target_sources(test_app PRIVATE tests/toolbar_pins.cpp framework/ToolbarPins.cpp framework/ToolbarPins.hpp)
