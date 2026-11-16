# SPDX-License-Identifier: MIT
# The options, the rows of a binding and their expression, the targets'
# summary: plain Qt Core.
target_sources(test_app PRIVATE tests/material_tool.cpp commands/MaterialBinding.cpp
               commands/MaterialBinding.hpp browser/OptionMapping.cpp)
