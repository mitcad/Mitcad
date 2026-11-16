# SPDX-License-Identifier: MIT
# The color roles (mitcad#1041, docs/ui-conventions.md "Colors"): the role
# table's contrast in both appearances and its reserved status hues; the
# table is Theme.cpp's, already in test_app.
target_sources(test_app PRIVATE tests/color_roles.cpp)
