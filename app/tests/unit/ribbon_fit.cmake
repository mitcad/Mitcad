# SPDX-License-Identifier: MIT
# How a ribbon page's groups fit a narrow window (mitcad#1111, mitcad#1357):
# the rule the Qt ribbon and the AppKit ribbon share; plain C++.
target_sources(test_app PRIVATE tests/ribbon_fit.cpp framework/RibbonFit.cpp framework/RibbonFit.hpp)
