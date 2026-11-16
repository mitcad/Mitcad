# SPDX-License-Identifier: MIT
# The command panel's message within narrow panels (mitcad#198): plain Qt
# Widgets.
target_sources(test_app PRIVATE tests/command_message.cpp framework/CommandMessage.cpp framework/CommandMessage.hpp)
