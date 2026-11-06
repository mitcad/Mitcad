// SPDX-License-Identifier: MIT
#pragma once

// The Textures group of Preferences (mitcad#315): the local folders Edit
// Appearances searches for texture maps, and whether texture sources that
// use the network may be asked (shown only when one is registered).

#include <QGroupBox>
#include <QWidget>

#include "TextureSource.hpp"

class QCheckBox;
class QListWidget;
class QPushButton;

namespace mitcad {

// A group with a frame and a title, except on macOS, where the settings
// window's panes have none: a plain widget.
#ifdef MITCAD_MAC_UI
using TexturePreferencesBase = QWidget;
#else
using TexturePreferencesBase = QGroupBox;
#endif

class TexturePreferencesBox : public TexturePreferencesBase {
  Q_OBJECT

public:
  explicit TexturePreferencesBox(QWidget* parent = nullptr);

  // The settings as the box shows them, to save when Preferences is
  // accepted.
  TextureSettings chosen() const;

signals:
  void changed();

protected:
  void showEvent(QShowEvent* event) override;

private:
  void addFolder(const QString& folder);

  TextureSettings m_settings;
  QListWidget* m_folders = nullptr;
  QPushButton* m_add = nullptr;
  QPushButton* m_remove = nullptr;
  QCheckBox* m_network = nullptr;
};

} // namespace mitcad
