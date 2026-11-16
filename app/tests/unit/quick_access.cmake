# SPDX-License-Identifier: MIT
# The File commands in the registry with their keys, the group's choice
# read from and written to the settings, its order and tooltips: Qt Core,
# Gui and Widgets (the registry's actions).
target_sources(test_app PRIVATE tests/quick_access.cpp framework/QuickAccess.cpp framework/QuickAccess.hpp
               files/FileCommands.cpp files/FileCommands.hpp)
