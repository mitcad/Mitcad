# SPDX-License-Identifier: MIT
# The setting, a watched bar's height, the menus' names and the focus:
# Qt Core, Gui and Widgets.
target_sources(test_app PRIVATE tests/menu_bar.cpp framework/MenuBarReveal.cpp framework/MenuBarReveal.hpp
               framework/EnterRepeat.cpp framework/EnterRepeat.hpp)
