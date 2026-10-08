// SPDX-License-Identifier: MIT
#pragma once

#include <cstddef>
#include <cstdint>
#include <functional>
#include <map>
#include <string>
#include <unordered_map>
#include <vector>

#include <QElapsedTimer>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QTemporaryDir>

#include <TopoDS_Shape.hxx>

class QJsonObject;
#include "render/RenderClient.hpp"
#include "render/RenderImage.hpp"

class QLabel;
class QTimer;

namespace mitcad {

class OcctViewer;


// View > Rendered (docs/rendering.md): the 3D view shows the render
// worker's progressively refined image of the visible bodies instead of
// drawing them. The worker follows the view's camera and size; the bodies
// go to it whenever they change, while the model is idle, as a scene
// update (docs/rendering.md, "Scene updates"): every body by id with its
// mesh's content hash, placement and material, and only the meshes the
// worker does not hold yet. If the worker dies, the view says so and the
// mode ends.
class RenderMode : public QObject {
  Q_OBJECT

public:
  // `whenIdle` runs a call once no job computes the model (the scene is
  // read from the bodies' triangulations).
  RenderMode(OcctViewer& viewer, std::function<void(QObject*, std::function<void()>)> whenIdle,
             QObject* parent = nullptr);
  ~RenderMode() override;

  void setEnabled(bool enabled);
  bool isEnabled() const { return m_enabled; }

  // The document's render settings (the model's `render_settings` query,
  // mitcad#47) and the folder of its file (relative image paths are in
  // it; empty for an unsaved one). The environment, the background's
  // visibility and the ground go to the worker, which renders again; the
  // exposure, the view transform and the background colour change only
  // how the last frame is shown.
  void setSettings(const QJsonObject& settings, const QString& documentFolder);
  // Shows the last frame anew (the view's background changed).
  void redisplay() { displayFrame(); }
  // The render device's choice in Preferences changed (mitcad#50): a
  // running worker starts again on it.
  void deviceChanged();

signals:
  // The mode ended by itself (the worker died).
  void stopped();

private:
  void workerReady(const QString& renderer);
  void workerFailed(const QString& reason);
  // The bodies changed: a new scene once the model is idle.
  void scheduleScene();
  void sendScene();
  // The worker applied a scene: logs what changed, removes its mesh file.
  void sceneApplied(const QJsonObject& event);
  // Forgets the meshes of shapes and in the worker that the last scenes
  // did not use, beyond kKept* (oldest first); returns the worker's.
  std::vector<std::string> forgetMeshes();
  // Sends the view when the camera or the size changed (at most every
  // kViewInterval ms while it keeps changing).
  void viewChanged();
  void sendView();
  void showFrame();
  // Shows m_frame with the settings' look over the background they ask for.
  void displayFrame();
  // The worker's "environment" command for the settings.
  QJsonObject environmentCommand() const;
  void sendEnvironment();
  void showMessage(const QString& text);

  OcctViewer& m_viewer;
  std::function<void(QObject*, std::function<void()>)> m_whenIdle;
  render::RenderClient* m_client;
  QTemporaryDir m_dir;
  QPointer<QLabel> m_message;
  QTimer* m_messageTimer;
  QTimer* m_viewTimer;
  bool m_enabled = false;
  bool m_ready = false;
  bool m_scenePending = false;
  // Scene updates: their sequence number, the mesh files the worker has not
  // read yet, the content hash of each shape's mesh (so that an unchanged
  // body is not meshed again), and the meshes the worker holds.
  struct ShapeEqual {
    bool operator()(const TopoDS_Shape& a, const TopoDS_Shape& b) const { return a.IsEqual(b); }
  };
  struct ShapeMesh {
    std::string key; // empty: no triangles
    std::size_t bytes = 0;
    std::uint64_t used = 0; // the last scene that showed it
  };
  struct WorkerMesh {
    std::size_t bytes = 0;
    std::uint64_t used = 0;
  };
  std::uint64_t m_scene = 0;
  std::map<std::uint64_t, QString> m_meshFiles;
  std::unordered_map<TopoDS_Shape, ShapeMesh, std::hash<TopoDS_Shape>, ShapeEqual> m_shapeMeshes;
  std::map<std::string, WorkerMesh> m_workerMeshes;
  QString m_lastView;      // the camera and size last sent
  std::uint64_t m_view = 0; // its sequence number
  QElapsedTimer m_sinceView; // since it was sent
  QElapsedTimer m_sinceSent; // for the interval
  bool m_firstFrameLogged = false;
  bool m_firstDenoisedLogged = false;
  QJsonObject m_settings;   // the document's render settings
  QString m_documentFolder;
  QString m_sentEnvironment; // the last "environment" command sent
  QString m_missingTextures; // the textures last said to be missing
  render::Look m_look;
  render::Frame m_frame; // the last frame shown, for a new look
};

} // namespace mitcad
