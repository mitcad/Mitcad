// SPDX-License-Identifier: MIT
#pragma once

// An input demonstration of a guide step (mitcad#2366, GitHub
// mitcad/Mitcad#634; docs/tutorials.md "Demonstrations"): the tutorial
// page's own pictures (a mouse, a keycap, a pointer; SVG, PNG, JPEG) and
// their timing, which TutorialGuide plays on a layer over the step's target
// that takes no input, following the target as the window and its panes
// move. MitCad draws nothing of its own and never moves the real pointer:
// the page says what is shown, where in the target's rectangle and for how
// long, and when it starts and stops.
//
// What a demonstration is, read from its JSON, and where each of its
// frames is at a moment, in plain Qt Core, Gui and Svg
// (app/tests/guide_demonstration.cpp).

#include <functional>

#include <QByteArray>
#include <QImage>
#include <QJsonObject>
#include <QPointF>
#include <QRectF>
#include <QSizeF>
#include <QString>
#include <QVector>

namespace mitcad {

struct GuideDemonstrationFrame {
  QImage image;          // drawn at its size, at the window's pixel ratio
  QSizeF size;           // in logical pixels
  QPointF at{0.5, 0.5};  // the place in the target's rectangle, 0..1 of its width and height
  QPointF offset;        // added to it, in logical pixels
  QPointF anchor;        // the picture's point put there, 0..1 of its size (0, 0: its top left)
  int ms = 500;          // how long it is shown
  bool move = false;     // glides from the previous frame's place to its own meanwhile
};

struct GuideDemonstration {
  QVector<GuideDemonstrationFrame> frames;
  int repeat = 1;    // times played; 0: until stopped
  int pauseMs = 0;   // nothing shown between two plays
  QString label;     // what it shows, in words (the layer's accessible name, the log)

  // Reads a page's demonstration (docs/tutorials.md "Demonstrations").
  // `load` gives the bytes of a picture named by its path in the tutorial's
  // folder (false and the reason when it cannot); a "data:" address is read
  // here. Pictures are drawn at `pixelRatio`. No frames and the reason in
  // `error` when it is not one.
  using Loader = std::function<bool(const QString& path, QByteArray* bytes, QString* error)>;
  static GuideDemonstration parse(const QJsonObject& json, const Loader& load, qreal pixelRatio, QString* error);

  // One play's length in ms (its frames').
  int cycleMs() const;

  // What is shown `elapsed` ms after it started, over `target` (the step's
  // target's rectangle): the frame (-1 between two plays, or once it ended)
  // and the point its anchor is at.
  struct Pose {
    int frame = -1;
    QPointF point;
    bool ended = false;
  };
  Pose poseAt(qint64 elapsed, const QRectF& target) const;
  // Where frame `index` is drawn with its anchor at `point`.
  QRectF frameRect(int index, const QPointF& point) const;
  // Where a demonstration over `target` may draw (mitcad#2615): the target
  // and kMargin logical pixels around it, so that a page's picture cannot
  // cover or relabel another control of the window.
  static constexpr double kMargin = 64;
  static QRectF allowedArea(const QRectF& target);
  // The place of frame `index` itself (its at and offset) over `target`.
  QPointF framePoint(int index, const QRectF& target) const;
};

// The bytes of a "data:" address (base64 or percent-encoded); false and
// the reason when it is not one.
bool guideDataUrl(const QString& address, QByteArray* bytes, QString* mimeType, QString* error);

// A picture from its bytes: SVG (by its content or `svg`), else what Qt
// reads (PNG, JPEG, …), drawn at `size` logical pixels (empty: its own) and
// `pixelRatio`; its logical size in `drawnSize`. Null and the reason when it
// cannot be read.
QImage guideDemonstrationImage(const QByteArray& bytes, bool svg, const QSizeF& size, qreal pixelRatio,
                               QSizeF* drawnSize, QString* error);

} // namespace mitcad
