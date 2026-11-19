# SPDX-License-Identifier: MIT
# The command layout (mitcad#1951): the default file read, the user's layer
# applied and kept, and the file read only at the start and on a change (a
# load counter); plain Qt Core.
target_sources(test_app PRIVATE tests/command_layout.cpp framework/CommandLayout.cpp framework/CommandLayout.hpp)
qt_add_resources(test_app test_command_layout PREFIX /layout BASE resources/layout FILES resources/layout/default.json)
