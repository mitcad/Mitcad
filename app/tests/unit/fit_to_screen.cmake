# SPDX-License-Identifier: MIT
# A scroll area as tall as its contents and a dialog sized to them within
# the screen (mitcad#631): plain Qt Widgets.
target_sources(test_app PRIVATE tests/fit_to_screen.cpp framework/FitToScreen.cpp framework/FitToScreen.hpp)
