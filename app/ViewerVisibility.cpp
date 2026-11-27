// SPDX-License-Identifier: MIT
#include "MainWindow.hpp"
#include "platform/MacChrome.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeMacWindow.hpp"
#endif

namespace mitcad {
void MainWindow::setDocumentVisible(bool visible) {
  // Shown again (its tab chosen, mitcad#2471): the datums' pick places are
  // logged anew once the view rests, also where they stayed, since the
  // last ones logged may be another design's.
  if (visible && m_viewer && !m_viewer->documentVisible()) m_loggedDatumPlaces.clear();
  if (m_viewer) m_viewer->setDocumentVisible(visible);
}
void MainWindow::minimizeDocumentWindow() {
#ifdef MITCAD_MAC_UI
  mac::minimizeDocumentWindow(window());
#else
  window()->showMinimized();
#endif
}
bool MainWindow::closeDocumentWindow() {
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeWindow) return m_nativeWindow->close();
#endif
  return close();
}
} // namespace mitcad
