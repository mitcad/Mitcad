# SPDX-License-Identifier: MIT
# Key sequences in Keyboard Shortcuts (mitcad#2095): a key that starts
# another as a clash in words, a sequence in the settings and in an exported
# settings file, the Keyboard and Mouse Overview's cheat sheet tables.
target_sources(test_app PRIVATE tests/shortcut_sequences.cpp framework/CommandRegistry.cpp framework/Command.cpp
               framework/CommandSearchRank.cpp framework/CommandLayout.cpp framework/SettingsTransfer.cpp
               view/ShortcutOverview.cpp view/ShortcutOverview.hpp)
