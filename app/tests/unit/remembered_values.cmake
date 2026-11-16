# SPDX-License-Identifier: MIT
# Kept on a new feature's OK only, read back, a value that does not
# evaluate falling back: plain Qt Core.
target_sources(test_app PRIVATE tests/remembered_values.cpp framework/RememberedValues.cpp
               framework/RememberedValues.hpp)
