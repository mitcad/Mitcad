# SPDX-License-Identifier: MIT
# File › Drawing Book… (mitcad#2448): the dialog's groups, order, command
# and problems from a `drawing_book` answer, and the cover page's picture;
# plain Qt Widgets and the model messages' translation.
target_sources(test_app PRIVATE tests/drawing_book.cpp drawings/DrawingBookDialog.cpp
               drawings/DrawingBookDialog.hpp printing/BookCover.cpp printing/BookCover.hpp
               printing/PaperPrint.cpp printing/PaperPrint.hpp framework/ModelMessages.cpp
               framework/ModelMessages.hpp framework/ModelMessageTemplates.cpp)
target_link_libraries(test_app PRIVATE Qt6::PrintSupport)
