# SPDX-License-Identifier: MIT
# A renamed command's former id and saved shortcut (mitcad#503): the
# registry with the command definitions it keeps.
target_sources(test_app PRIVATE tests/command_alias.cpp framework/CommandRegistry.cpp framework/Command.cpp)
