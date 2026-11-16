# SPDX-License-Identifier: MIT
# The design's and the recent rows, the check mark, the recent list's
# order and length, read back from the settings, and when Replace
# Appearance asks (mitcad#1067); the Appearance tool's recent appearances
# (mitcad#1685): plain Qt Core.
target_sources(test_app PRIVATE tests/material_menu.cpp print/MaterialMenuChoices.cpp
               print/MaterialMenuChoices.hpp)
