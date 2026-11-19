# SPDX-License-Identifier: MIT
# Pin, reorder, unpin, each pin's group and the settings read back
# (also an earlier version's per-tab lists), kept in the command layout's
# user layer (mitcad#1951): plain Qt Core.
target_sources(test_app PRIVATE tests/toolbar_pins.cpp framework/ToolbarPins.cpp framework/ToolbarPins.hpp
               framework/CommandLayout.cpp framework/CommandLayout.hpp)
