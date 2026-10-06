// SPDX-License-Identifier: MIT
#pragma once

// The Updates group of Preferences (mitcad#9): whether Mitcad checks for a
// newer release at start-up (at most once a day) and on which channel;
// disabled, with the reason, when an administrator turned checks off.

#include <QGroupBox>

#include "UpdateSettings.hpp"

class QCheckBox;
class QComboBox;

namespace mitcad {

class UpdatePreferencesBox : public QGroupBox {
  Q_OBJECT

public:
  explicit UpdatePreferencesBox(QWidget* parent = nullptr);

  // The settings with the box's choices, to save when Preferences is
  // accepted.
  UpdateSettings chosen() const;

private:
  UpdateSettings m_settings;
  QCheckBox* m_automatic = nullptr;
  QComboBox* m_channel = nullptr;
};

} // namespace mitcad
