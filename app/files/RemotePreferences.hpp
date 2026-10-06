// SPDX-License-Identifier: MIT
#pragma once

// The Version Control group of Preferences (P12 remote): the git program
// that remote repositories use (found automatically, or chosen), with the
// version found; how often the remote is checked for newer versions; and
// whether each saved version is sent to the remote at once.

#include <QGroupBox>

#include "../framework/AppSettings.hpp"

class QCheckBox;
class QLabel;
class QLineEdit;
class QSpinBox;

namespace mitcad {

class RemotePreferencesBox : public QGroupBox {
  Q_OBJECT

public:
  explicit RemotePreferencesBox(QWidget* parent = nullptr);

  // The settings with the box's choices, to save when Preferences is
  // accepted.
  RemoteSettings chosen() const;

private:
  // Shows the git program the field names (or the one found) and its
  // version.
  void detect();

  RemoteSettings m_settings;
  QLineEdit* m_git = nullptr;
  QLabel* m_found = nullptr;
  QSpinBox* m_minutes = nullptr;
  QCheckBox* m_push = nullptr;
};

} // namespace mitcad
