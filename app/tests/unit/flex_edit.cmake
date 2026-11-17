# SPDX-License-Identifier: MIT
# Editing a Flex item in the view (mitcad#1713): the flex query read, the
# command a release sends, the definitions the previews evaluate, places on
# the route and the twist: plain Qt Core.
target_sources(test_app PRIVATE tests/flex_edit.cpp flex/FlexEdit.cpp flex/FlexEdit.hpp)
