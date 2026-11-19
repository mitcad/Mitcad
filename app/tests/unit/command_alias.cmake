# SPDX-License-Identifier: MIT
# A renamed command's former id and saved shortcut (mitcad#503): the
# registry with the command definitions it keeps (and the other names it
# adds to their keywords, mitcad#1983).
target_sources(test_app PRIVATE tests/command_alias.cpp framework/CommandRegistry.cpp framework/Command.cpp
               framework/CommandSearchRank.cpp framework/CommandLayout.cpp)
