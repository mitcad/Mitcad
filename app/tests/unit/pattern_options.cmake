# SPDX-License-Identifier: MIT
# The mapping and the words, and the Pattern Type a preselection picks
# (mitcad#1290) on a command state.
target_sources(test_app PRIVATE tests/pattern_options.cpp commands/PatternOptions.cpp
               commands/PatternOptions.hpp framework/Command.cpp framework/Command.hpp)
