# SPDX-License-Identifier: MIT
# Let It Hang (mitcad#1713): the model's answers read, pins toggled, what the
# panel wants, one step in flight, the commands, where a press goes and the
# marker's arrow: plain Qt Core.
target_sources(test_app PRIVATE tests/gravity_logic.cpp gravity/GravityLogic.cpp gravity/GravityLogic.hpp)
