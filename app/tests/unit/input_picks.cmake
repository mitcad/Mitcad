# SPDX-License-Identifier: MIT
# A selection input's picks listed by name on demand (mitcad#2064): their
# words, the chip's tooltip, the lists' open state and the hovered row;
# plain Qt Widgets.
target_sources(test_app PRIVATE tests/input_picks.cpp framework/InputPicks.cpp framework/InputPicks.hpp)
