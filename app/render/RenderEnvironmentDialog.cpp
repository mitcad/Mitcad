// SPDX-License-Identifier: MIT
#include "render/RenderEnvironmentDialog.hpp"

#include <algorithm>
#include <utility>

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QPushButton>
#include <QScreen>
#include <QShowEvent>
#include <QVBoxLayout>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "framework/Icons.hpp"
#include "framework/TestSync.hpp"
#include "render/RenderEnvironmentPanel.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad {

RenderEnvironmentDialog::RenderEnvironmentDialog(DocumentHost& host, OcctViewer& viewer,
                                                 std::function<void(bool)> showRendered, QWidget* parent)
    : QDialog(parent), m_viewer(viewer), m_showRendered(std::move(showRendered)) {
  setWindowTitle(tr("Render Environment"));
  setWindowIcon(themeIcon(QStringLiteral("environment")));
  auto* outer = new QVBoxLayout(this);
  m_rendered = new QCheckBox(tr("Rendered view"));
  m_rendered->setToolTip(tr("View › Visual Style › Rendered: the view shows the bodies path traced in these settings"));
  outer->addWidget(m_rendered);
  connect(m_rendered, &QCheckBox::toggled, this, [this](bool on) {
    if (!m_building && m_showRendered) {
      m_showRendered(on);
    }
  });
  m_panel = new RenderEnvironmentPanel(host, viewer);
  outer->addWidget(m_panel);

  auto* buttons = new QDialogButtonBox;
  m_reset = new QPushButton(tr("Reset"));
  m_reset->setToolTip(tr("The default settings: the studio over the view's background, shadows on the ground"));
  m_close = new QPushButton(tr("Close"));
  buttons->addButton(m_reset, QDialogButtonBox::ResetRole);
  buttons->addButton(m_close, QDialogButtonBox::RejectRole);
  mac::styleDialogButtons(buttons, m_close);
  outer->addWidget(buttons);
  connect(m_reset, &QPushButton::clicked, m_panel, &RenderEnvironmentPanel::reset);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  mac::styleDialog(this);
}

void RenderEnvironmentDialog::refresh() {
  m_panel->refresh();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void RenderEnvironmentDialog::setRenderedShown(bool shown) {
  m_building = true;
  m_rendered->setChecked(shown);
  m_building = false;
}

void RenderEnvironmentDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  m_logged.clear();
  TestSync::singleShot(0, this, [this] { logLayout(); });
  if (m_placed || parentWidget() == nullptr) {
    return;
  }
  m_placed = true;
  // Beside the main window, where it leaves the view free to watch the
  // render, as far as the screen allows.
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

void RenderEnvironmentDialog::logLayout() {
  if (!isVisible()) {
    return;
  }
  // In the main window's coordinates, as the panel's fields.
  const QPoint origin = m_viewer.window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget) {
    const QPoint inside = qobject_cast<QCheckBox*>(widget) != nullptr
                              ? QPoint(widget->rect().left() + 8, widget->rect().center().y())
                              : widget->rect().center();
    const QPoint global = widget->mapToGlobal(inside) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  const QStringList lines{QStringLiteral("Render settings rendered at %1").arg(at(m_rendered)),
                          QStringLiteral("Render settings reset at %1").arg(at(m_reset)),
                          QStringLiteral("Render settings close at %1").arg(at(m_close))};
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : lines) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
