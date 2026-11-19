// SPDX-License-Identifier: MIT
#include "render/RenderImageDialog.hpp"

#include <algorithm>

#include <QDialogButtonBox>
#include <QPushButton>
#include <QScreen>
#include <QShowEvent>
#include <QVBoxLayout>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "framework/Icons.hpp"
#include "framework/TestSync.hpp"
#include "render/RenderImagePanel.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad {

RenderImageDialog::RenderImageDialog(DocumentHost& host, OcctViewer& viewer, QWidget* parent)
    : QDialog(parent), m_viewer(viewer) {
  setWindowTitle(tr("Render Image"));
  setWindowIcon(themeIcon(QStringLiteral("render-image")));
  auto* layout = new QVBoxLayout(this);
  m_panel = new RenderImagePanel(host, viewer);
  layout->addWidget(m_panel);
  // Close beside the panel's Render, Cancel and Save….
  QDialogButtonBox* buttons = m_panel->buttons();
  m_close = new QPushButton(tr("Close"));
  buttons->addButton(m_close, QDialogButtonBox::RejectRole);
  mac::styleDialogButtons(buttons, m_panel->renderButton());
  connect(buttons, &QDialogButtonBox::rejected, this, &RenderImageDialog::reject);
  mac::styleDialog(this);
}

void RenderImageDialog::refresh() {
  m_panel->refresh();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void RenderImageDialog::setDocument(const QString& folder, const QString& name) {
  m_panel->setDocument(folder, name);
}

bool RenderImageDialog::isRendering() const { return m_panel->isRendering(); }

void RenderImageDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  m_logged.clear();
  TestSync::singleShot(0, this, [this] { logLayout(); });
  if (m_placed || parentWidget() == nullptr) {
    return;
  }
  m_placed = true;
  // Beside the main window, where it leaves the view free, as far as the
  // screen allows.
  QWidget* window = parentWidget()->window();
  const QScreen* screen = window->screen();
  if (screen == nullptr) {
    return;
  }
  const QRect available = screen->availableGeometry();
  const QRect main = window->frameGeometry();
  const QSize size = frameGeometry().size();
  const int x = std::max(available.left(), std::min(main.right() + 8, available.right() + 1 - size.width()));
  const int y = std::clamp(main.top(), available.top(), std::max(available.top(), available.bottom() + 1 - size.height()));
  move(x, y);
}

void RenderImageDialog::reject() {
  // Escape and every close path stop a render: nothing would show or save it.
  if (m_panel->isRendering()) {
    m_panel->cancelRender();
  }
  QDialog::reject();
}

void RenderImageDialog::logLayout() {
  if (!isVisible()) {
    return;
  }
  const QPoint origin = m_viewer.window()->mapToGlobal(QPoint(0, 0));
  const QPoint at = m_close->mapToGlobal(m_close->rect().center()) - origin;
  const QString line = QStringLiteral("Render image close at %1,%2").arg(at.x()).arg(at.y());
  if (line != m_logged) {
    m_logged = line;
    qDebug().noquote() << line;
  }
}

} // namespace mitcad
