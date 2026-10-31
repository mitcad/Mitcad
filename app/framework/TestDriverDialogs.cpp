// SPDX-License-Identifier: MIT
// Focused production dialog fixtures, using synthetic/local library data.
#include "TestDriver.hpp"

#include <algorithm>

#include <QApplication>
#include <QDialogButtonBox>
#include <QHeaderView>
#include <QJsonArray>
#include <QKeyEvent>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPushButton>
#include <QStyleOptionViewItem>
#include <QTableWidget>
#include <QTabWidget>
#include <QTreeWidget>
#include <QStyle>

#include "Dialogs.hpp"
#include "../files/Libraries.hpp"
#include "../files/ProjectSettings.hpp"
#include "../files/LibraryDialogs.hpp"
#include "../library/PartPicker.hpp"
#include "../projects/ProjectBrowser.hpp"

namespace mitcad {

void TestDriver::showDialogStyleFixture(const QString& kind) {
  const QString url = qEnvironmentVariable("MITCAD_STYLE_LIBRARY");
  if (kind == QStringLiteral("setup")) {
    if (url.isEmpty() || !qEnvironmentVariableIsSet("MITCAD_SETTINGS_DIR")) {
      fail(m_line, QStringLiteral("dialog fixture requires isolated settings and a local library"));
      return;
    }
    LibrarySettings settings;
    settings.sources = {{url, true, false}, {url + QStringLiteral("-disabled"), false, false}};
    settings.save();
    return;
  }
  if (kind == QStringLiteral("bom")) {
    auto* tabs = targetWindow()->findChild<QTabWidget*>();
    if (tabs == nullptr) { fail(m_line, QStringLiteral("library parts tabs missing")); return; }
    tabs->setCurrentIndex(1);
    return;
  }
  if (kind == QStringLiteral("project-settings")) {
    showProjectSettings(m_main, url, ProjectSettingsHost{});
    return;
  }
  if (kind == QStringLiteral("projects")) {
    ProjectBrowserOptions options;
    options.title = QStringLiteral("Open a Project");
    options.acceptText = QStringLiteral("Open");
    browseProjects(m_main, options);
    return;
  }
  QDialog* dialog = nullptr;
  if (kind == QStringLiteral("picker")) {
    auto* picker = new PartPicker(m_main);
    PickerRequest request;
    request.command = QStringLiteral("test.dialog-style");
    request.title = QStringLiteral("Parts");
    request.parts = pickerParts(libraryCommand({{QStringLiteral("cmd"), QStringLiteral("library_search")}}));
    if (request.parts.isEmpty()) { delete picker; fail(m_line, QStringLiteral("no synthetic library parts")); return; }
    request.current = request.parts.first().key;
    picker->ask(request);
    dialog = picker;
  } else if (kind == QStringLiteral("parts")) {
    const QJsonObject shown = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("library_show")},
                                              {QStringLiteral("url"), url}});
    const QJsonArray versions = shown.value(QStringLiteral("versions")).toArray();
    if (versions.isEmpty()) { fail(m_line, QStringLiteral("synthetic library has no version")); return; }
    const QString rev = versions.first().toObject().value(QStringLiteral("rev")).toString();
    const QJsonObject library{{QStringLiteral("library"), QStringLiteral("mitcad-parts")},
                             {QStringLiteral("url"), url}, {QStringLiteral("rev"), rev},
                             {QStringLiteral("component"), QStringLiteral("iso4762")},
                             {QStringLiteral("config"), QStringLiteral("M5x16")},
                             {QStringLiteral("license"), QStringLiteral("CC0-1.0")}};
    const QJsonArray parts{QJsonObject{{QStringLiteral("component"), QStringLiteral("C1")},
                                     {QStringLiteral("name"), QStringLiteral("Socket screw")},
                                     {QStringLiteral("linked"), true}, {QStringLiteral("library"), library}}};
    const QJsonArray list{QJsonObject{{QStringLiteral("quantity"), 2},
                                    {QStringLiteral("designation"), QStringLiteral("M5x16 socket screw")},
                                    {QStringLiteral("library"), QStringLiteral("mitcad-parts")},
                                    {QStringLiteral("version"), QStringLiteral("1.0.0")},
                                    {QStringLiteral("license"), QStringLiteral("CC0-1.0")}}};
    dialog = new LibraryPartsDialog(parts, list, m_main);
  } else {
    fail(m_line, QStringLiteral("unknown dialog fixture %1").arg(kind));
    return;
  }
  connect(dialog, &QDialog::finished, dialog, &QObject::deleteLater);
  // Through the shared dispatch, as the windows' production entries: the
  // native frontend hosts them in AppKit instead of showing Qt's window
  // (a Qt window there was a harness artefact, mitcad#819).
  showDialog(dialog);
}

void TestDriver::testDialogControls(const QString& kind) {
  QWidget* window = targetWindow();
  if (window == nullptr) { fail(m_line, QStringLiteral("no dialog for control checks")); return; }
  const auto check = [this](bool passed, const QString& message) {
    if (!passed) { fail(m_line, message); }
    return passed;
  };
  int defaults = 0;
  for (QPushButton* button : window->findChildren<QPushButton*>()) {
    if (!button->isVisibleTo(window)) { continue; }
    if (button->isDefault()) {
      ++defaults;
      qInfo().noquote() << QStringLiteral("Dialog default %1: %2").arg(window->windowTitle(), button->text());
    }
    const QString label = button->text().isEmpty() ? button->accessibleName() : button->text();
    if (label.contains(QRegularExpression(QStringLiteral("Add URL|Browse|Fetch|Copy as CSV|Save as PDF")))) {
      check(!button->isDefault() && !button->autoDefault(), QStringLiteral("auxiliary button can take Return: %1").arg(label));
    }
  }
  // The print windows save each field at Enter: no button takes it
  // (mitcad#655). Appearances commits a field at Return and stays open, so
  // none of its buttons takes Return either (mitcad#977).
  if (kind == QStringLiteral("appearances")) {
    check(defaults == 0, QStringLiteral("expected no visible dialog default, found %1").arg(defaults));
    for (QPushButton* button : window->findChildren<QPushButton*>()) {
      if (button->isVisibleTo(window)) {
        check(!button->autoDefault(), QStringLiteral("button can take Return: %1").arg(button->text()));
      }
    }
  } else if (kind != QStringLiteral("settings") && kind != QStringLiteral("materials")) {
    check(defaults == 1, QStringLiteral("expected one visible dialog default, found %1").arg(defaults));
  }
  for (QTableView* table : window->findChildren<QTableView*>()) {
    check(!table->showGrid() && table->verticalHeader()->isHidden(), QStringLiteral("table still has grid or row numbers"));
  }
#ifdef Q_OS_MACOS
  for (QLineEdit* field : window->findChildren<QLineEdit*>()) {
    if (field->property("macSearchField").toBool()) {
      check(field->placeholderText() == QStringLiteral("Search") && !field->actions().isEmpty(),
            QStringLiteral("search lacks its short placeholder or magnifier"));
    }
  }
  for (QDialogButtonBox* box : window->findChildren<QDialogButtonBox*>()) {
    for (QAbstractButton* accept : box->buttons()) {
      if (box->buttonRole(accept) != QDialogButtonBox::AcceptRole || !accept->isVisibleTo(window)) { continue; }
      for (QAbstractButton* reject : box->buttons()) {
        if (box->buttonRole(reject) == QDialogButtonBox::RejectRole && reject->isVisibleTo(window)) {
          check(accept->mapTo(window, accept->rect().center()).x() > reject->mapTo(window, reject->rect().center()).x(),
                QStringLiteral("Mac accept button is left of cancel/close"));
        }
      }
    }
  }
#endif
  if (kind == QStringLiteral("shortcuts-empty") || kind == QStringLiteral("shortcuts-results")) {
    auto* tree = window->findChild<QTreeWidget*>(QStringLiteral("shortcutTree"));
    int visible = 0;
    if (tree != nullptr) {
      for (int row = 0; row < tree->topLevelItemCount(); ++row) { visible += !tree->topLevelItem(row)->isHidden(); }
    }
    check(tree != nullptr && (kind == QStringLiteral("shortcuts-empty") ? visible == 0 : visible > 0),
          QStringLiteral("shortcut search did not filter the actual rows"));
  }
  if (kind == QStringLiteral("sources")) {
    auto* table = window->findChild<QTableWidget*>(QStringLiteral("librarySources"));
    if (!check(table != nullptr && table->rowCount() == 2, QStringLiteral("source fixture rows missing"))) { return; }
    const QModelIndex index = table->model()->index(0, 1);
    table->setCurrentIndex(index);
    const QVariant before = index.data(Qt::CheckStateRole);
    QKeyEvent key(QEvent::KeyPress, Qt::Key_Space, Qt::NoModifier);
    QApplication::sendEvent(table, &key);
    check(index.data(Qt::CheckStateRole) != before, QStringLiteral("Space did not toggle source Auto-update"));
    QStyleOptionViewItem option;
    option.initFrom(table);
    option.rect = table->visualRect(index);
    option.features = QStyleOptionViewItem::HasCheckIndicator;
    option.checkState = Qt::CheckState(index.data(Qt::CheckStateRole).toInt());
    const QPoint point = table->style()->subElementRect(QStyle::SE_ItemViewItemCheckIndicator, &option, table).center();
    const QPointF local(point);
    const QPointF global(table->viewport()->mapToGlobal(point));
    QMouseEvent down(QEvent::MouseButtonPress, local, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent up(QEvent::MouseButtonRelease, local, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QApplication::sendEvent(table->viewport(), &down);
    QApplication::sendEvent(table->viewport(), &up);
    check(index.data(Qt::CheckStateRole) == before, QStringLiteral("mouse did not restore source Auto-update"));
    for (const QModelIndex& off : {table->model()->index(0, 1), table->model()->index(1, 0)}) {
      option.rect = table->visualRect(off);
      option.checkState = Qt::Unchecked;
      const QRect indicator = table->style()->subElementRect(QStyle::SE_ItemViewItemCheckIndicator, &option, table);
      const QImage pixels = table->viewport()->grab(indicator).toImage();
      int darkest = 255;
      int lightest = 0;
      for (int y = 0; y < pixels.height(); ++y) {
        for (int x = 0; x < pixels.width(); ++x) {
          const int shade = qGray(pixels.pixel(x, y));
          darkest = std::min(darkest, shade);
          lightest = std::max(lightest, shade);
        }
      }
      check(!pixels.isNull() && lightest - darkest > 12,
            QStringLiteral("unchecked source indicator has no visible outline"));
      qInfo() << "Dialog unchecked checkbox contrast" << off.row() << off.column() << lightest - darkest;
    }
    qInfo() << "Dialog source checkboxes: keyboard and mouse toggled";
  }
  qInfo().noquote() << QStringLiteral("Dialog controls checked: %1").arg(kind);
}

} // namespace mitcad
