# SPDX-License-Identifier: MIT
# The step in the unit shown, the modifiers, the minimum, the texts left
# alone: plain Qt Core. ParameterField.cpp in test_app calls it.
target_sources(test_app PRIVATE tests/value_step.cpp framework/ValueStep.cpp framework/ValueStep.hpp)
