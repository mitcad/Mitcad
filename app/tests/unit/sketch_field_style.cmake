# SPDX-License-Identifier: MIT
# The sketch's value fields over the view (mitcad#2086): selected text in
# the accent with AccentText in their light values, valid and invalid, as
# the field's palette takes them from the style sheet (Qt Widgets on the
# offscreen platform; Theme.cpp is in test_app already).
target_sources(test_app PRIVATE tests/sketch_field_style.cpp sketch/SketchFieldStyle.cpp sketch/SketchFieldStyle.hpp)
