// SPDX-License-Identifier: MIT
#pragma once

#include <QStringList>
#include <QWidget>

class QCheckBox;
class QLabel;

namespace mitcad {

class CommandRegistry;

namespace sketch {

class SketchController;

// The sketch palette, in the command panel's place while sketching:
// the sketch's degrees of freedom, display and snap options, the
// constraints and Finish Sketch.
class SketchPalette : public QWidget {
  Q_OBJECT

public:
  // `constraints` are the ids of the constraint commands, in palette order.
  SketchPalette(SketchController& controller, CommandRegistry& registry, const QStringList& constraints,
                QWidget* parent = nullptr);

  // Logs where the options are, for UI tests.
  void logLayout() const;

private:
  void refresh();

  SketchController& m_c;
  QLabel* m_title = nullptr;
  QLabel* m_status = nullptr;
  QLabel* m_tool = nullptr;
  QCheckBox* m_construction = nullptr;
  QCheckBox* m_grid = nullptr;
  QCheckBox* m_dimensions = nullptr;
  QCheckBox* m_constraints = nullptr;
  QCheckBox* m_profiles = nullptr;
  QCheckBox* m_hideAbove = nullptr;
};

} // namespace sketch
} // namespace mitcad
