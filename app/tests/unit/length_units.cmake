# SPDX-License-Identifier: MIT
# The conversion and the nice values of mm, cm, m, in and ft, the texts
# Measure shows: plain Qt Core. BrowserPanel.cpp in test_app calls it.
target_sources(test_app PRIVATE tests/length_units.cpp framework/LengthUnits.cpp framework/LengthUnits.hpp)
