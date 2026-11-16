# SPDX-License-Identifier: MIT
# Kinds of selection to the buttons, the commands' first inputs, the
# row's buttons changed in place: Qt Widgets on the offscreen platform.
target_sources(test_app PRIVATE tests/selection_create.cpp framework/SelectionCreate.cpp
               framework/SelectionCreate.hpp framework/Command.cpp framework/Command.hpp)
