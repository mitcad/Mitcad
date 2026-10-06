// SPDX-License-Identifier: MIT
#pragma once

// The Cache group of Preferences (P7d): whether computed results are kept
// on disk and how much of them, where, and Clear; how much of them the
// memory keeps; and the diagnostics.

#include <functional>

#include <QGroupBox>
#include <QWidget>

#include "ResultCache.hpp"

class QCheckBox;
class QLabel;
class QSpinBox;

namespace mitcad {

// A group with a frame and a title, except on macOS, where the settings
// window's panes have none: a plain widget.
#ifdef Q_OS_MACOS
using CachePreferencesBase = QWidget;
#else
using CachePreferencesBase = QGroupBox;
#endif

class CachePreferencesBox : public CachePreferencesBase {
  Q_OBJECT

public:
  // `diagnostics` opens Help > Diagnostics over the dialog.
  explicit CachePreferencesBox(std::function<void()> diagnostics, QWidget* parent = nullptr);

  // The settings as the box shows them, to save when Preferences is
  // accepted.
  CacheSettings chosen() const;

signals:
  // A setting was changed (not Clear).
  void changed();

private:
  void clearDisk();

  CacheSettings m_settings;
  QCheckBox* m_disk = nullptr;
  QSpinBox* m_diskSize = nullptr;
  QSpinBox* m_memorySize = nullptr;
  QLabel* m_cleared = nullptr;
};

} // namespace mitcad
