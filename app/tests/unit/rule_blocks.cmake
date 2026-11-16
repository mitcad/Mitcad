# SPDX-License-Identifier: MIT
# The expression <-> block round trip, the edits, and the page on a design
# of the test's own: Qt Widgets on the offscreen platform.
target_sources(test_app PRIVATE tests/rule_blocks.cpp
               configurations/RuleBlocks.cpp configurations/RuleBlocks.hpp
               configurations/RuleCanvas.cpp configurations/RuleCanvas.hpp
               configurations/RulesPage.cpp configurations/RulesPage.hpp
               framework/FormulaEditor.cpp framework/FormulaEditor.hpp
               framework/ParameterField.cpp framework/ParameterField.hpp
               framework/ParameterSuggest.cpp framework/ParameterSuggest.hpp)
