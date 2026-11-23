# SPDX-License-Identifier: MIT
# Move/Copy of a component picked inside a sub-assembly (mitcad#2302): the
# component the move works in and its frame's occurrence, plain Qt Core.
target_sources(test_app PRIVATE tests/move_target.cpp commands/MoveTarget.cpp commands/MoveTarget.hpp)
