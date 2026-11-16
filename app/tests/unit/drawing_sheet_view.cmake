# SPDX-License-Identifier: MIT
# The sheet's zoom kept through the model's answers (mitcad#751) and its
# picture kept while the pointer moves (mitcad#759): plain Qt Widgets on
# the offscreen platform.
target_sources(test_app PRIVATE tests/drawing_sheet_view.cpp drawings/DrawingSheetView.cpp
                                drawings/DrawingSheetView.hpp)
