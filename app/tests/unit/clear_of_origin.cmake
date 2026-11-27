# SPDX-License-Identifier: MIT
# An arrow kept clear of its origin (mitcad#2508): its knob's lift at zero,
# the fade, and the value back from where the knob is drawn.
target_sources(test_app PRIVATE tests/clear_of_origin.cpp framework/ClearOfOrigin.hpp)
