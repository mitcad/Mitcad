# SPDX-License-Identifier: MIT
# The list's rows, the unit sent and selected, the column's editor:
# plain Qt Widgets. framework/ParameterField.cpp in test_app calls it.
target_sources(test_app PRIVATE tests/parameter_units.cpp framework/ParameterUnits.cpp
               framework/ParameterUnits.hpp)
