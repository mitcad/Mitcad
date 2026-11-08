// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <vector>

#include <QCoreApplication>
#include <QStringList>
#include <QWidget>

class QLabel;
class QVBoxLayout;

namespace mitcad::sketch {

// The Sketch Palette's warnings under its status (mitcad#264): one label
// per warning of the sketch's evaluation. A warning the model gives
// entities for (the `sketch` query's `warning_entities`) looks like a link,
// with a pointing hand, and a click on it calls `activated` with its index;
// the others are plain text.
class PaletteWarnings : public QWidget {
  Q_DECLARE_TR_FUNCTIONS(mitcad::sketch::PaletteWarnings)

public:
  explicit PaletteWarnings(QWidget* parent = nullptr);

  // Shows the warnings, `entities` one list per warning.
  void setWarnings(const QStringList& warnings, const std::vector<QStringList>& entities);
  // Called with the index of a clicked warning that has entities.
  std::function<void(int)> activated;
  // Logs where each linked warning is ("Palette warning 0 at x,y"), for UI
  // tests.
  void logLayout() const;

private:
  QVBoxLayout* m_layout = nullptr;
  std::vector<QLabel*> m_labels;
  std::vector<bool> m_linked;
  QStringList m_shown;
  std::vector<QStringList> m_entities;
};

} // namespace mitcad::sketch
