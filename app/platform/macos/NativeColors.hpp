// SPDX-License-Identifier: MIT
#pragma once

#import <AppKit/AppKit.h>

#include <QColor>

#include "../../framework/NoticeLevel.hpp"
#include "../../framework/Theme.hpp"

namespace mitcad::mac {

// The role table (framework/Theme.hpp, mitcad#1041) in the AppKit chrome:
// the one place where a role becomes an NSColor. Every color is dynamic and
// follows the appearance (light or dark), never a static conversion of the
// value the Qt palette had when it was made.
//
// - The error's text, rings and badge are the system's red, the warning's
//   badge the system's yellow, the success's and information's badges the
//   system's green and blue (the levels' symbols' tints).
// - The warning's and the success's text keep the table's values: the
//   system's yellow and green read below 4.5:1 as text on a light window.
// - The chrome's selection and accent are the system's accent (mitcad#852
//   C3; the 3D view keeps its fixed blue), the texts and lines the system's
//   label, secondary label and separator colors.
// - Every other role is the table's light or dark value.
NSColor* roleColor(ColorRole role);

// A level's tint: the level's badge role (none for hints and progress:
// the secondary label color).
NSColor* noticeTint(NoticeLevel level);

// A fixed sRGB color: for what is not a role (a user's or an appearance's
// color, a picture).
NSColor* srgbColor(const QColor& color);

} // namespace mitcad::mac
