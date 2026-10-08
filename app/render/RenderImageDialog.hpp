// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>

#include <QDialog>
#include <QJsonObject>
#include <QJsonValue>
#include <QPointer>
#include <QString>
#include <QTemporaryDir>
#include <QVector>

class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QLabel;
class QProgressBar;
class QPushButton;
class QSpinBox;

namespace mitcad {

class DocumentHost;
class OcctViewer;

namespace render {
class FinalRender;
}

class RenderFrameOverlay;

// File > Render Image (mitcad#48, docs/rendering.md "Final render"): the
// render settings' `output` section (size, aspect, samples, time limit,
// denoising, transparency, format; each change a model command and an undo
// step, as in Render Environment), the camera (the current view or a named
// view), and the render itself: the shown bodies with the document's
// environment and look, rendered by the render worker in its batch mode
// (mitcad-render --batch), with progress, a preview, Cancel and Save. Not
// modal: the application stays usable while it renders. With a fixed
// aspect the view shows the image's frame while the dialog is open.
class RenderImageDialog : public QDialog {
  Q_OBJECT

public:
  RenderImageDialog(DocumentHost& host, OcctViewer& viewer, QWidget* parent = nullptr);
  ~RenderImageDialog() override;

  // The settings as the model has them now (after any change of the
  // document: undo, redo, opening).
  void refresh();
  // The design's folder (relative environment images, where Save starts)
  // and its name (the image's default file name).
  void setDocument(const QString& folder, const QString& name);
  bool isRendering() const;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;
  void closeEvent(QCloseEvent* event) override;

private:
  QJsonValue value(const QString& field) const; // of the output section
  void change(const QString& field, const QJsonValue& value);
  void choosePreset(int index);
  void startRender();
  void renderScene(); // once the model is idle
  void cancelRender();
  void save();
  void showProgress(int samples, int total, double seconds, double remaining);
  void showPreview(const QString& path);
  void setRendering(bool rendering);
  void updateEnabled();
  void updateFrame();
  double viewAspect() const;
  void logLayout();

  DocumentHost& m_host;
  OcctViewer& m_viewer;
  QJsonObject m_settings;
  QString m_documentFolder;
  QString m_documentName;
  QComboBox* m_camera = nullptr;
  QComboBox* m_preset = nullptr;
  QSpinBox* m_width = nullptr;
  QSpinBox* m_height = nullptr;
  QComboBox* m_aspect = nullptr;
  QSpinBox* m_samples = nullptr;
  QDoubleSpinBox* m_timeLimit = nullptr;
  QCheckBox* m_denoise = nullptr;
  QCheckBox* m_transparent = nullptr;
  QComboBox* m_format = nullptr;
  QSpinBox* m_quality = nullptr;
  QLabel* m_preview = nullptr;
  QProgressBar* m_progress = nullptr;
  QLabel* m_status = nullptr;
  QLabel* m_message = nullptr;
  QPushButton* m_render = nullptr;
  QPushButton* m_cancel = nullptr;
  QPushButton* m_save = nullptr;
  QPushButton* m_close = nullptr;
  QPointer<RenderFrameOverlay> m_frame;
  render::FinalRender* m_job = nullptr;
  std::unique_ptr<QTemporaryDir> m_dir; // the scene, the job, previews and the image
  QString m_image;      // the finished image, until the next render
  QString m_imageFormat; // its output.format
  bool m_waiting = false; // for the model to be idle
  bool m_building = false;
  bool m_placed = false;
  QString m_logged;
};

} // namespace mitcad
