# SPDX-License-Identifier: MIT
# The command search's other names and place line (mitcad#1983): the
# registry, the ranking and the layout with the shipped default; plain Qt
# Core.
target_sources(test_app PRIVATE tests/command_search_places.cpp framework/CommandSearchRank.cpp
               framework/CommandSearchRank.hpp framework/CommandRegistry.cpp framework/Command.cpp
               framework/CommandLayout.cpp framework/CommandLayout.hpp)
