# SPDX-License-Identifier: MIT
# The Hole panel's fields that follow a screw size (mitcad#980): Qt Core
# and the command state, with a model of the size tables.
target_sources(test_app PRIVATE tests/hole_sizes.cpp commands/HoleSizes.cpp commands/HoleSizes.hpp)
