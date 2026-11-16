# SPDX-License-Identifier: MIT
# The parameter table's numbers on a shared decimal axis (mitcad#566):
# the number split, the column layout and the delegate's placement; plain
# Qt Core and Widgets.
target_sources(test_app PRIVATE tests/decimal_align.cpp browser/DecimalAlign.cpp browser/DecimalAlign.hpp
               browser/DecimalAlignDelegate.cpp browser/DecimalAlignDelegate.hpp)
