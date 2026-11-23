# SPDX-License-Identifier: MIT
# The Favorite Parameters card's rows (mitcad#2203): plain Qt Core
# (framework/Numbers.cpp is in test_app already).
target_sources(test_app PRIVATE tests/favorite_parameter_rows.cpp browser/FavoriteParameterRows.cpp
               browser/FavoriteParameterRows.hpp browser/ParameterSort.cpp browser/ParameterSort.hpp)
