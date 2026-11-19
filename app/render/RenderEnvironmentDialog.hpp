// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QDialog>
#include <QString>

class QCheckBox;
class QPushButton;

namespace mitcad {

class DocumentHost;
class OcctViewer;
class RenderEnvironmentPanel;

// Render Environment (mitcad#47, View menu): the document's render settings
// in a window of their own, RenderEnvironmentPanel whole (the environment
// and the lights as two pages), with Rendered view, Reset and Close. Not
// modal: it follows undo and redo. The Render workspace (mitcad#2013,
// RenderWorkspace) shows the same panel docked instead.
class RenderEnvironmentDialog : public QDialog {
  Q_OBJECT

public:
  // `showRendered` turns View > Rendered on or off.
  RenderEnvironmentDialog(DocumentHost& host, OcctViewer& viewer, std::function<void(bool)> showRendered,
                          QWidget* parent = nullptr);

  // Shows the settings as the model has them now.
  void refresh();
  // Whether View > Rendered is on (the dialog's check box follows it).
  void setRenderedShown(bool shown);

protected:
  void showEvent(QShowEvent* event) override;

private:
  void logLayout();

  OcctViewer& m_viewer;
  std::function<void(bool)> m_showRendered;
  RenderEnvironmentPanel* m_panel = nullptr;
  QCheckBox* m_rendered = nullptr;
  QPushButton* m_reset = nullptr;
  QPushButton* m_close = nullptr;
  bool m_building = false;
  bool m_placed = false;
  QString m_logged; // the logged places
};

} // namespace mitcad
