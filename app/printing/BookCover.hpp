// SPDX-License-Identifier: MIT
#pragma once

// The drawing book's cover page (mitcad#2448, File › Drawing Book…): a
// page of the first sheet's size with a frame, the book's title large in
// its upper half and its contents under it, one line per drawing in the
// book's order. Printed first when the book's Cover page is checked.

#include <QSizeF>
#include <QString>
#include <QStringList>

#include "PaperPrint.hpp"

namespace mitcad::printing {

// The frame's margin and the title's and the contents' letter heights (mm).
constexpr double kCoverMargin = 10.0;
constexpr double kCoverTitleHeight = 14.0;
constexpr double kCoverLineHeight = 4.5;

// The cover as a picture of `size` (mm). The title shrinks to fit the
// frame's width; contents that do not fit end with a line "…".
PrintPicture bookCover(const QString& title, const QStringList& contents, const QSizeF& size);

} // namespace mitcad::printing
