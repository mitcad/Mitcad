// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QString>

class QPushButton;

namespace mitcad {

class DocumentHost;
class OcctViewer;
class RenderImagePanel;

// Render Image (mitcad#48, docs/rendering.md "Final render"): the final
// render in a window of its own, RenderImagePanel with a preview of the
// image as it refines, and Close. Not modal: the application stays usable
// while it renders. The Render workspace's Output stage (mitcad#2013)
// shows the same panel docked, the image in the view instead.
class RenderImageDialog : public QDialog {
  Q_OBJECT

public:
  RenderImageDialog(DocumentHost& host, OcctViewer& viewer, QWidget* parent = nullptr);

  // The settings as the model has them now (after any change of the
  // document: undo, redo, opening).
  void refresh();
  // The design's folder (relative environment images, where Save starts)
  // and its name (the image's default file name).
  void setDocument(const QString& folder, const QString& name);
  bool isRendering() const;
  void reject() override;

protected:
  void showEvent(QShowEvent* event) override;

private:
  void logLayout();

  OcctViewer& m_viewer;
  RenderImagePanel* m_panel = nullptr;
  QPushButton* m_close = nullptr;
  bool m_placed = false;
  QString m_logged;
};

} // namespace mitcad
