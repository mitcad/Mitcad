# SPDX-License-Identifier: MIT
# A scroll area's own children changing places rebuild no native
# presenter, a widget added or removed or reordered elsewhere does
# (mitcad#986): plain Qt Widgets.
target_sources(test_app PRIVATE tests/native_structure.cpp platform/NativeStructure.cpp
               platform/NativeStructure.hpp)
