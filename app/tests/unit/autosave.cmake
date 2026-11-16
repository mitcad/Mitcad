# SPDX-License-Identifier: MIT
# Atomic autosave generations through normal recovery (mitcad#110).
target_sources(test_app PRIVATE tests/autosave.cpp files/AutosaveFiles.cpp files/Recovery.cpp framework/Dialogs.cpp)
