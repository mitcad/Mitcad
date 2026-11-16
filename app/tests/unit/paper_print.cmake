# SPDX-License-Identifier: MIT
# Printing to PDF files through QPrinter: a two-sheet drawing's pages at
# their sizes, a 1:1 sketch's tiles and a line's length on them, fit to
# page on one page; plain Qt Widgets, Svg and PrintSupport.
target_sources(test_app PRIVATE tests/paper_print.cpp printing/PaperPrint.cpp printing/PaperPrint.hpp
               printing/SketchPicture.cpp printing/SketchPicture.hpp)
target_link_libraries(test_app PRIVATE Qt6::PrintSupport)
