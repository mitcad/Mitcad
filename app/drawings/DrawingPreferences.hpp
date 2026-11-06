// SPDX-License-Identifier: MIT
#pragma once

// Preferences › Display › "Always show corners in drawings" and its
// "Corner dot size" (mitcad#1338): while it is on, the drawing editor draws
// every corner its sheet's views show (the model's `drawing_corners`, the
// points a click picks) as a filled dot. An aid on the screen only: never
// printed, exported or kept in the document. Off by default, Medium.
// Plain Qt Core.

#include <optional>

#include <QObject>
#include <QSettings>
#include <QString>

namespace mitcad::drawings {

enum class CornerDotSize { Small, Medium, Large };

struct CornerDots {
  bool always = false;
  CornerDotSize size = CornerDotSize::Medium;

  bool operator==(const CornerDots& other) const { return always == other.always && size == other.size; }
  bool operator!=(const CornerDots& other) const { return !(*this == other); }

  // The settings "drawing/alwaysShowCorners" and "drawing/cornerDotSize"
  // ("small", "medium", "large"); an unknown size is Medium.
  static CornerDots load(const QSettings& settings);
  void save(QSettings& settings) const;
};

// The id kept in the settings and logged, and back; nullopt for another.
QString cornerDotSizeId(CornerDotSize size);
std::optional<CornerDotSize> cornerDotSizeFromId(const QString& id);

// A dot's radius on the screen, logical pixels (which follow the display's
// scale: 1.5 device pixels each at 150 %); all smaller than the marker of
// the corner under the pointer (DrawingSheetView::kCornerMarkerRadius).
double cornerDotRadius(CornerDotSize size);

// The application's choice (QSettings of the application).
CornerDots savedCornerDots();
// Saves it and tells CornerDotsNotifier; false when it was already so.
bool saveCornerDots(const CornerDots& dots);

// Tells an open drawing that the choice changed (saveCornerDots), so that
// its dots come or go at once.
class CornerDotsNotifier : public QObject {
  Q_OBJECT

public:
  static CornerDotsNotifier& instance();

signals:
  void changed();
};

} // namespace mitcad::drawings
