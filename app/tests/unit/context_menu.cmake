# SPDX-License-Identifier: MIT
# The view's context menu (mitcad#494): which commands a selection
# completes (first) and which need further picks (More), from the command
# definitions.
target_sources(test_app PRIVATE tests/context_menu.cpp framework/Command.cpp framework/Command.hpp)
