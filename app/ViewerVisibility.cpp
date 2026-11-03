// SPDX-License-Identifier: MIT
#include "MainWindow.hpp"
#include "platform/MacChrome.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeMacWindow.hpp"
#endif

namespace mitcad {
void MainWindow::setDocumentVisible(bool visible) {
  if (m_viewer) m_viewer->setDocumentVisible(visible);
}
void MainWindow::minimizeDocumentWindow() {
#ifdef Q_OS_MACOS
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
