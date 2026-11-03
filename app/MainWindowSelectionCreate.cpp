// SPDX-License-Identifier: MIT
// The selection card's Create row (mitcad#619; app/COMMANDS.md, "What the
// framework does"): up to four CREATE commands that fit the selection,
// Extrude first whenever it applies, as buttons under the selection's
// text and above Autoselect, in the Floating card, the Docked panel and
// the native macOS inspector (which mirrors the idle page's widgets). The
// rule is framework/SelectionCreate.cpp's; here the window gives it the
// commands' definitions and whether they are enabled, and logs the row.
// The Make tab has no row: the Make section takes its place (mitcad#1178).
#include "MainWindow.hpp"

#include <utility>

#include <QAction>
#include <QDebug>
#include <QPoint>
#include <QScrollArea>
#include <QScrollBar>
#include <QStackedWidget>
#include <QString>
#include <QStringList>
#include <QToolButton>
#include <QVector>
#include <QtLogging>

#include "framework/AutoselectPanel.hpp"
#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/GlassCard.hpp"
#include "framework/Ribbon.hpp"
#include "framework/SelectionCreate.hpp"
#include "framework/TestSync.hpp"
#include "print/MakeBodyCard.hpp"

namespace mitcad {

void MainWindow::updateSelectionCreate() {
  if (m_selectionCreate == nullptr) {
    return; // before the idle page is made
  }
  QVector<SelectionCreatePanel::Entry> entries;
  QStringList names;
  // None on the Make tab: the Make section is the card's there (mitcad#1178).
  const bool makeTab = m_ribbon != nullptr && m_ribbon->currentTab() == QLatin1String("MAKE");
  if (m_mode == Mode::Idle && !m_selection.isEmpty() && !makeTab) {
    const Selection selection = m_selection;
    // Enabled as the ribbon shows it (updateActions, isAvailable): no query
    // here, which the model would refuse while a job computes.
    const auto fits = [this, &selection](const QString& id) {
      const CommandDef* def = m_registry->find(id);
      const QAction* action = def != nullptr ? m_registry->action(id) : nullptr;
      return action != nullptr && action->isEnabled() && takesWholeSelection(*def, selection);
    };
    for (const QString& id : selectionCreateCommands(selection, fits)) {
      const CommandDef* def = m_registry->find(id);
      entries.append(SelectionCreatePanel::Entry{def->id, def->name, def->icon, def->tooltip});
      names << def->name;
    }
  }
  const bool changed = m_selectionCreate->setEntries(entries);
  const QString logged = selectionCreateLogText(names);
  if (logged != m_loggedSelectionCreate) {
    m_loggedSelectionCreate = logged;
    qDebug().noquote() << QStringLiteral("Selection card create: %1").arg(logged);
  }
  if (!changed) {
    return;
  }
  // Where each button is, for UI tests, once the card has laid it out:
  // "Selection card create Extrude at x,y".
  TestSync::singleShot(300, m_selectionCreate, [this] {
    for (int i = 0; i < m_selectionCreate->entries().size(); ++i) {
      const QToolButton* button = m_selectionCreate->button(i);
      if (button == nullptr || !button->isVisible()) {
        continue;
      }
      const QPoint at = GlassCard::mapToHost(button, button->rect().center());
      qDebug().noquote() << QStringLiteral("Selection card create %1 at %2,%3")
                                .arg(m_selectionCreate->entries().at(i).name)
                                .arg(at.x())
                                .arg(at.y());
    }
    logSelectionCardSections();
  });
  updateCommandCard();
#ifdef MITCAD_MACOS_NATIVE_UI
  refreshNativeInspector();
#endif
}

void MainWindow::logSelectionCardSections() {
  if (m_commandPages == nullptr || m_idlePage == nullptr || m_commandPages->currentWidget() != m_idlePage) {
    return;
  }
  // The card: the Floating layout's glass card, else (Docked) the panel's
  // pages; what of it shows the page: the scroll area's viewport.
  const QWidget* card = m_commandCard != nullptr && m_commandCard->isAncestorOf(m_commandPages)
                            ? static_cast<const QWidget*>(m_commandCard)
                            : static_cast<const QWidget*>(m_commandPages);
  const auto* scroll = qobject_cast<const QScrollArea*>(m_idlePage);
  const QWidget* visible = scroll != nullptr ? scroll->viewport() : m_idlePage;
  const bool scrolled = scroll != nullptr && scroll->verticalScrollBar()->isVisible();
  const auto left = [](const QWidget* widget) { return GlassCard::mapToHost(widget, QPoint(0, 0)).x(); };
  const auto right = [](const QWidget* widget) {
    return GlassCard::mapToHost(widget, QPoint(widget->width() - 1, 0)).x();
  };
  const std::pair<const char*, const QWidget*> sections[] = {
      {"Make", m_makeCard}, {"Create", m_selectionCreate}, {"Autoselect", m_autoselect}};
  for (const auto& [name, section] : sections) {
    if (section == nullptr || !section->isVisible()) {
      continue;
    }
    qDebug().noquote() << QStringLiteral("Selection card section %1: %2..%3 of card %4..%5, visible to %6%7")
                              .arg(QLatin1String(name))
                              .arg(left(section))
                              .arg(right(section))
                              .arg(left(card))
                              .arg(right(card))
                              .arg(right(visible))
                              .arg(scrolled ? QStringLiteral(", scrolled") : QString());
  }
}

} // namespace mitcad
