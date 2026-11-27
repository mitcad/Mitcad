// SPDX-License-Identifier: MIT
#pragma once

// File › Export Turntable (mitcad#2523; app/COMMANDS.md "Export
// Turntable"): the dialog of a turntable's file, axis, turn, frames, size
// and style, which starts with the choices of the settings (group
// `turntable`) and saves them back on Export.

#include <optional>

#include <QSize>
#include <QString>

#include "view/Turntable.hpp"

class QWidget;

namespace mitcad {

struct TurntableChoice {
  TurntableOutput output = TurntableOutput::Mp4;
  QString video;        // the video file (empty for frames only)
  QString framesFolder; // where the frames go (empty when they are not kept)
  // The axis: z, y, x (the design's, through the middle of the shown
  // bodies), up (the view's up through it) or selection (the selected
  // item's axis).
  QString axis = QStringLiteral("z");
  double angle = 360.0; // degrees, counterclockwise seen from the axis's tip
  int frames = 72;
  double fps = 24.0;
  QString sizeKey = QStringLiteral("view"); // view, 720p, 1080p, 2160p, custom
  QSize size;                               // the frames' pixels
  bool rendered = false;                    // by the render worker, else the view's style
  QString ffmpeg;                           // the FFmpeg program (with a video)
};

struct TurntableDialogInput {
  QString start;         // the suggested video file (with .mp4)
  QSize view;            // the 3D view's size in pixels
  bool renderer = false; // the render worker is there: Rendered can be chosen
  QString selectedAxis;  // the selected item in words when it can have an axis; empty otherwise
};

std::optional<TurntableChoice> askTurntable(QWidget* parent, const TurntableDialogInput& input);

} // namespace mitcad
