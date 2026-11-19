# SPDX-License-Identifier: MIT
# What the Text tool and Edit Text send for a text's frame, its wrap and
# its lines (mitcad#2029): plain Qt Core.
target_sources(test_app PRIVATE tests/sketch_text_commands.cpp sketch/TextCommands.cpp)
