// SPDX-License-Identifier: MIT
// The shown bodies meshed again for display when Preferences > Display >
// Small circles changes (mitcad#879, framework/DisplayMeshes,
// view/SmallCirclesPreference.hpp).
#include "MainWindow.hpp"

#include "OcctViewer.hpp"
#include "framework/DisplayMeshes.hpp"
#include "framework/DocumentSession.hpp"

namespace mitcad {

void MainWindow::remeshDisplay() {
  // At the model's idle point: a job under way meshes what it changes with
  // the new choice already, and the bodies are read on the worker only
  // while no other job writes them.
  m_documentSession->whenIdle(this, [this] {
    const bool started = m_documentSession->remeshDisplayAsync(
        this, [this](std::size_t given, const DocumentSession::AsyncResult& result) {
          if (!result.succeeded() || given == 0) {
            return; // the bodies keep the meshes they have
          }
          // The bodies' faces have the new meshes: drawn anew from them.
          m_viewer->redrawBodies();
          showBodies();
        });
    if (!started) {
      remeshDisplay(); // another job came first: after it
    }
  });
}

} // namespace mitcad
