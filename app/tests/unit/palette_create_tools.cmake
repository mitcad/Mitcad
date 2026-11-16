# SPDX-License-Identifier: MIT
# Defaults, counts, pins first, the ribbon's order, at most 14: plain Qt Core.
target_sources(test_app PRIVATE tests/palette_create_tools.cpp sketch/PaletteCreateTools.cpp
               sketch/PaletteCreateTools.hpp)
