// SPDX-License-Identifier: MIT
#pragma once

// What the Text tool and Edit Text send for a text's place, its wrap and
// its lines (mitcad#2029): on the sketch model's data only, plain Qt Core.

#include <optional>

#include <QJsonObject>
#include <QString>

#include "SketchModel.hpp"

namespace mitcad::sketch {

// The Text tool's place of a new text into its `sketch.add_text`: a drag
// from `from` to `to` (`framed`) is a frame, from its lower left corner to
// its upper right, the text at its top and wrapped to its width; a click
// is the text's `at` (`from`), one line unless the text breaks it.
void insertNewTextPlace(QJsonObject& cmd, const V2& from, const V2& to, bool framed);

// Edit Text's Wrap in Frame is shown for a text in a frame that is not on
// a path (`onPath`: the panel's Path, as it is now).
bool wrapInFrameShown(const TextData& text, bool onPath);

// The `wrap` Edit Text sends: the check box's state when it is shown and
// differs from the text's own, so that an edit that leaves it alone keeps
// the wrap a text had (an imported one's); none otherwise.
std::optional<bool> changedTextWrap(const TextData& text, bool onPath, bool checked);

// A text as typed or pasted with its line breaks as `\n`, the model's
// (Windows' `\r\n`, a lone `\r`, and the Unicode line and paragraph
// separators a text box may give).
QString textWithLineBreaks(const QString& typed);

} // namespace mitcad::sketch
