// SPDX-License-Identifier: MIT
#pragma once

// The clipboard of sketch geometry (mitcad#788, SketchClipboard.cpp), shared
// by a 2D sketch's Copy, Cut and Paste and a 3D sketch's (mitcad#1661,
// Sketch3dClipboard.cpp): the model's `sketch_fragment` answer as JSON under
// the type application/x-mitcad-sketch-fragment+json, nothing else.

#include <QByteArray>
#include <QString>

namespace mitcad::sketch {

// Puts a `sketch_fragment` answer on the clipboard, and keeps it in the
// application for a session whose clipboard cannot be written.
void putSketchFragment(const QByteArray& json);
// The answer the clipboard holds, empty when it holds none; `source` says
// where it was found, for the log.
QByteArray sketchFragmentOnClipboard(QString* source = nullptr);

} // namespace mitcad::sketch
