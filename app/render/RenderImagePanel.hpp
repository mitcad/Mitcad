// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>

#include <QJsonObject>
#include <QJsonValue>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QWidget>

#include "framework/EventFilterGuard.hpp"

class QBoxLayout;
class QCheckBox;
class QComboBox;
class QDialogButtonBox;
class QDoubleSpinBox;
class QGroupBox;
class QLabel;
class QLineEdit;
class QListWidget;
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
class RenderViewPreview;

// The final render to an image file (mitcad#48, docs/rendering.md "Final
// render") as a panel: the render settings' `output` section (size,
// aspect, samples, time limit, denoising, transparency, format; each
// change a model command and an undo step), the camera (the current view,
// a named view or a shot of the Render workspace, mitcad#2013), and the
// render itself: the shown bodies with the document's environment and
// look, rendered by the render worker in its batch mode (mitcad-render
// --batch), with progress, Render, Cancel and Save…. With a fixed aspect
// the view shows the image's frame while the panel shows.
//
// Render Image (RenderImageDialog) shows the image as it refines in a
// preview of its own; the Render workspace's Output stage (setInView) shows
// it in the view itself, in the image's frame, and adds Render All Shots'
// folder, its "running · waiting" count and its results (renderAllShots,
// run by the workspace's Output group: each shot rendered one after the
// other and saved into the output folder).
//
// Places are logged as "Render image <part> at x,y" in the main window's
// coordinates.
class RenderImagePanel : public QWidget {
  Q_OBJECT

public:
  RenderImagePanel(DocumentHost& host, OcctViewer& viewer, QWidget* parent = nullptr);
  ~RenderImagePanel() override;

  // The settings as the model has them now (after any change of the
  // document: undo, redo, opening), the named views and the shots too.
  void refresh();
  // The design's folder (relative environment images, where Save starts,
  // the output folder's default) and its name (the image's default name).
  void setDocument(const QString& folder, const QString& name);
  bool isRendering() const;
  // The image in the view instead of a preview of its own, with the
  // output folder and Render All Shots (the Render workspace).
  void setInView(bool inView);
  // The render's camera: a shot's id (mitcad#2013); empty: the current
  // view. Logged as "Render image camera <shot name>".
  void setShot(const QString& id);
  // The folder Render All Shots saves into: the user's choice, else the
  // design's folder (empty for an unsaved design).
  QString outputFolder() const;
  // The image in the view (setInView) taken away: the live view shows again.
  void clearViewImage();
  // Asked before Render All Shots starts (the workspace's Ready to
  // Render); false stops it.
  std::function<bool()> beforeRenderAll;
  // Render, Cancel and Save…, for a window that adds its own buttons.
  QDialogButtonBox* buttons() const { return m_buttons; }
  QPushButton* renderButton() const { return m_render; }

public slots:
  void startRender();
  void cancelRender();
  void save();
  // Every shot rendered one after the other (mitcad#2013) and saved into
  // the output folder as "<design>-<shot>.<extension>".
  void renderAllShots();
  void setOutputFolder(const QString& folder);

signals:
  // A render started (true) or ended: finished, failed or cancelled.
  void renderingChanged(bool rendering);
  // The image shows in the view (setInView), or no longer does.
  void viewImageShown(bool shown);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  QJsonValue value(const QString& field) const; // of the output section
  void change(const QString& field, const QJsonValue& value);
  void choosePreset(int index);
  void renderScene(); // once the model is idle
  void showProgress(int samples, int total, double seconds, double remaining);
  void showPreview(const QString& path);
  void setRendering(bool rendering);
  void updateEnabled();
  void updateFrame();
  double viewAspect() const;
  void logLayout();
  // Writes the finished image to `path`, with the shown annotations over
  // it when asked; false with `error` set when it cannot.
  bool writeImage(const QString& path, QString& error);
  // Render All Shots: the next shot of the queue, or the end.
  void renderNextShot();
  void finishShot(const QString& outcome);
  void showBatch();
  QString shotName(const QString& id) const;

  DocumentHost& m_host;
  OcctViewer& m_viewer;
  EventFilterGuard m_viewerFilter{this}; // its resizes; the view may go first (mitcad#226)
  QJsonObject m_settings;
  QString m_documentFolder;
  QString m_documentName;
  bool m_inView = false;
  QBoxLayout* m_columns = nullptr;
  QComboBox* m_camera = nullptr;
  QComboBox* m_preset = nullptr;
  QSpinBox* m_width = nullptr;
  QSpinBox* m_height = nullptr;
  QComboBox* m_aspect = nullptr;
  QSpinBox* m_samples = nullptr;
  QDoubleSpinBox* m_timeLimit = nullptr;
  QCheckBox* m_denoise = nullptr;
  QCheckBox* m_transparent = nullptr;
  QCheckBox* m_measurements = nullptr; // the shown measurements over the saved image (mitcad#649)
  QJsonObject m_finalView;             // the last render's camera (render::finalView)
  QComboBox* m_format = nullptr;
  QSpinBox* m_quality = nullptr;
  QLabel* m_preview = nullptr;
  QProgressBar* m_progress = nullptr;
  QLabel* m_status = nullptr;
  QLabel* m_message = nullptr;
  QDialogButtonBox* m_buttons = nullptr;
  QPushButton* m_render = nullptr;
  QPushButton* m_cancel = nullptr;
  QPushButton* m_save = nullptr;
  // Render All Shots (mitcad#2013).
  QGroupBox* m_batchBox = nullptr;
  QLineEdit* m_folder = nullptr;
  QPushButton* m_browseFolder = nullptr;
  QLabel* m_batchCount = nullptr;
  QListWidget* m_results = nullptr;
  QStringList m_queue;   // the shots still waiting
  QString m_batchShot;   // the shot rendering now
  int m_batchTotal = 0;
  int m_batchDone = 0;
  QPointer<RenderFrameOverlay> m_frame;
  QPointer<RenderViewPreview> m_viewImage;
  render::FinalRender* m_job = nullptr;
  QJsonObject m_jobSpec;      // the last render's job, for the shipped worker after a picked one (mitcad#1332)
  QString m_worker;           // the worker it started
  bool m_workerPicked = false; // the one picked in Preferences
  std::unique_ptr<QTemporaryDir> m_dir; // the scene, the job, previews and the image
  QString m_image;      // the finished image, until the next render
  QString m_imageFormat; // its output.format
  bool m_waiting = false; // for the model to be idle
  bool m_building = false;
  QString m_logged;
};

} // namespace mitcad
