# SPDX-License-Identifier: MIT
# The settings' load and save and the widths: Qt Core and Gui (the
# selection role's colour, framework/Theme.cpp, is in test_app already).
target_sources(test_app PRIVATE tests/selection_look.cpp view/SelectionLook.cpp view/SelectionLook.hpp)
