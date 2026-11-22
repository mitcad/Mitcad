// SPDX-License-Identifier: MIT
// Test steps for widgets and dialogs checked: texts and choices, the status message,
// properties, backgrounds and themes, the projects' layout, the tree's rename,
// popups, cards, widgets revealed, focused and invoked, a dialog's size and
// appearance.
// Split from TestDriver.cpp (mitcad#2173), so that a UI test depends on the
// step areas it uses; the steps are described in TestDriver.hpp.
#include "TestDriver.hpp"
#include "TestDriverSupport.hpp"
#include "Diagnostics.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdlib>
#include <limits>
#include <memory>
#include <unordered_set>
#include <utility>
#include <vector>

#include <BRep_Tool.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Poly_Polygon3D.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_TShape.hxx>

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAction>
#include <QAbstractScrollArea>
#include <QApplication>
#include <QClipboard>
#include <QColor>
#include <QComboBox>
#include <QContextMenuEvent>
#include <QDir>
#include <QDockWidget>
#include <QEnterEvent>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QHash>
#include <QHeaderView>
#include <QIcon>
#include <QImage>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLayout>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QOpenGLWidget>
#include <QPainter>
#include <QPixmap>
#include <QPointingDevice>
#include <QRegion>
#include <QRegularExpression>
#include <QScreen>
#include <QScrollArea>
#include <QSettings>
#include <QShortcut>
#include <QStyleHints>
#include <QThread>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QTextDocument>
#include <QWheelEvent>
#include <QWidget>
#include <QWindow>
#include <QtGlobal>
#include <QtLogging>

#include "../platform/MacChrome.hpp"
#include "../platform/MacTestApplication.hpp"
#include "../platform/NativeTestInput.hpp"
#include "../OcctViewer.hpp"
#include "../DocumentFrame.hpp"
#include "../MainWindow.hpp"
#include "../WindowManager.hpp"
#include "../update/UpdateController.hpp"
#include "../view/SpaceMouseController.hpp"
#include "../view/ViewController.hpp"
#include "../view/ViewEdges.hpp"
#include "../sketch/SketchController.hpp"
#include "../sketch/SketchOverlay.hpp"
#include "../sketch/SketchTool.hpp"
#include "../browser/TimelineWidget.hpp"
#include "GlassCard.hpp"
#include "Ribbon.hpp"
#include "TestSync.hpp"
#include "HangWatchdog.hpp"
#include "SessionLog.hpp"
#include "Dialogs.hpp"
#include "DisplayMeshes.hpp"
#include "Icons.hpp"
#include "ComputeProgress.hpp"
#include "Theme.hpp"
#include "TreeItemDelegate.hpp"
#include "mitcad/geometry/geometry.hpp"
#include "mitcad/io/mesh.hpp"

namespace mitcad {

using namespace testdriver;

namespace {

// The colour that covers most of a visible widget's area as its window
// draws it, the parents' backgrounds under it included (mitcad#1245: the
// Docked toolbar's surface under the ribbon's groups and beside Undo, not a
// style's own fill), with how many of the area's pixels have it; `missing`
// says why there is none (expect-widget-background, expect-widget-theme).
struct WidgetColor {
  QColor color;
  int count = 0;
  int total = 0;
  QString missing;
};

WidgetColor mostCommonColor(QWidget* top, QWidget* widget, const QString& widgetName) {
  WidgetColor result;
  if (top == nullptr || widget == nullptr || !widget->isVisible()) {
    result.missing = QStringLiteral("no visible widget %1").arg(widgetName);
    return result;
  }
  const QRect area =
      QRect(top->mapFromGlobal(widget->mapToGlobal(QPoint(0, 0))), widget->size()).intersected(top->rect());
  const QImage shot = area.isEmpty() ? QImage() : top->grab(area).toImage().convertToFormat(QImage::Format_RGB32);
  if (shot.isNull()) {
    result.missing = QStringLiteral("%1 has no area in its window").arg(widgetName);
    return result;
  }
  QHash<QRgb, int> counts;
  QRgb most = 0;
  for (int y = 0; y < shot.height(); ++y) {
    for (int x = 0; x < shot.width(); ++x) {
      const QRgb rgb = shot.pixel(x, y);
      const int count = ++counts[rgb];
      if (count > result.count) {
        most = rgb;
        result.count = count;
      }
    }
  }
  result.color = QColor(most);
  result.total = shot.width() * shot.height();
  return result;
}

const TestDriver::StepSpec kStepSpecs[] = {
    {"test-status-message", 2, 2, TestDriver::kInputOfMain},
    {"expect-input-text", 2, 2, TestDriver::kIdle},
    // A Qt choice's shown option (mitcad#1868: a remembered choice).
    {"expect-choice-text", 2, 2, TestDriver::kIdle},
    {"reveal-widget", 1, 1, TestDriver::kInputOfMain},
    {"focus-widget", 1, 1, TestDriver::kInputOfTarget},
    {"expect-clear-of-cube", 1, 1, TestDriver::kIdle},
    {"expect-widget-property", 3, 3, TestDriver::kIdle},
    {"expect-no-qt-popup", 0, 0, TestDriver::kIdle},
    {"expect-projects-layout", 1, 1, TestDriver::kIdle},
    {"expect-tree-rename", 1, 1, TestDriver::kIdle},
    {"collapse-card", 2, 2, TestDriver::kInputOfMain},
    {"expect-dialog-gone", 1, 1, TestDriver::kIdle},
    {"dialog-resize", 2, 2, TestDriver::kInputOfTarget},
    {"expect-dialog-appearance", 1, 1, TestDriver::kIdle},
    // The colour a widget shows as drawn in its window (mitcad#1245).
    {"expect-widget-background", 3, 3, TestDriver::kIdle},
    // Whether that colour is dark or light (mitcad#1282).
    {"expect-widget-theme", 2, 2, TestDriver::kIdle},
    // A named widget's invokable method, with one text or none: what a
    // window offers its tests where input cannot reach, such as a link on
    // the component browser's page (mitcad#1579).
    {"invoke-widget", 2, 3, TestDriver::kInputOfTarget},
};

} // namespace

const TestDriver::StepArea TestDriver::kWidgetSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runWidgetStep};

void TestDriver::runWidgetStep(const QString& name, const QStringList& a, int line) {
  QWidget* mainWindow = m_main.data();
  // A regular expression argument.
  const auto regex = [&](const QString& pattern, QRegularExpression& re) {
    re = QRegularExpression(pattern);
    if (!re.isValid()) {
      fail(line, QStringLiteral("bad regular expression '%1': %2").arg(pattern, re.errorString()));
      return false;
    }
    return true;
  };
  if (name == QLatin1String("expect-input-text")) {
    QRegularExpression re;
    if (!regex(a.at(1), re)) { return; }
    const QString id = QStringLiteral("input_") + a.at(0);
    whenReady([this, id, re] {
      if (mac::test::active()) { return mac::test::textMatches(id, re); }
      QWidget* window = m_main.data();
      const auto* input = window ? window->findChild<QLineEdit*>(id) : nullptr;
      if (input == nullptr && window != nullptr) {
        // A text box of several lines (mitcad#2029): its lines joined by \n.
        if (const auto* lines = window->findChild<QPlainTextEdit*>(id)) {
          m_waitDetail = QStringLiteral("shown '%1'").arg(lines->toPlainText());
          return re.match(lines->toPlainText()).hasMatch();
        }
      }
      return input && re.match(input->text()).hasMatch();
    }, 6, QStringLiteral("input %1 did not match %2").arg(a.at(0), a.at(1)));
  } else if (name == QLatin1String("expect-choice-text")) {
    // The option a panel's choice shows (its combo's text, or a wrapper's
    // child combo's), as the user reads it (mitcad#1868).
    QRegularExpression re;
    if (!regex(a.at(1), re)) { return; }
    const QString id = QStringLiteral("input_") + a.at(0);
    whenReady([this, id, re] {
      QWidget* window = m_main.data();
      QWidget* field = window ? window->findChild<QWidget*>(id) : nullptr;
      const QComboBox* combo = qobject_cast<QComboBox*>(field);
      if (!combo && field) { combo = field->findChild<QComboBox*>(); }
      if (combo) { m_waitDetail = QStringLiteral("shown '%1'").arg(combo->currentText()); }
      return combo && re.match(combo->currentText()).hasMatch();
    }, 6, QStringLiteral("choice %1 did not show %2").arg(a.at(0), a.at(1)));
  } else if (name == QLatin1String("test-status-message")) {
    // Exercise the ordinary status path without manufacturing a model
    // failure just to obtain an error's longer lifetime (mitcad#996).
    // Any level (mitcad#1040): hint, progress, info, success, warning, error.
    auto* window = qobject_cast<MainWindow*>(mainWindow);
    const QStringList levels = {QStringLiteral("hint"), QStringLiteral("progress"), QStringLiteral("info"),
                                QStringLiteral("success"), QStringLiteral("warning"), QStringLiteral("error")};
    if (window == nullptr || !levels.contains(a.at(0))) {
      fail(line, QStringLiteral("test-status-message requires %1 and the main window")
                     .arg(levels.join(QLatin1Char('|'))));
      return;
    }
    const NoticeLevel level = static_cast<NoticeLevel>(levels.indexOf(a.at(0)));
    window->showStatus(a.at(1), level);
  } else if (name == QLatin1String("collapse-card")) {
    auto* card = m_main->findChild<GlassCard*>(a.at(0));
    if (!card || (a.at(1) != QLatin1String("true") && a.at(1) != QLatin1String("false"))) {
      fail(line, QStringLiteral("collapse-card needs a card name and true|false")); return;
    }
    if (card->isCollapsed() != (a.at(1) == QLatin1String("true"))) {
      card->findChild<QToolButton*>(QStringLiteral("cardChevron"))->click();
    }
  } else if (name == QLatin1String("expect-projects-layout")) {
    whenReady([this, a] {
      auto* projects = m_main->findChild<QWidget*>(QStringLiteral("projectsDock"));
      auto* browser = m_main->findChild<QWidget*>(QStringLiteral("browserDock"));
      auto* tree = m_main->findChild<QTreeWidget*>(QStringLiteral("projectsTree"));
      if (!projects || !browser || !tree || !projects->isVisible() || !browser->isVisible() ||
          tree->topLevelItemCount() != a.at(0).toInt()) {
        m_waitDetail = QStringLiteral("missing or hidden panels, or unexpected project count");
        return false;
      }
      const QRect actual(projects->mapToGlobal(QPoint()), projects->size());
      const QRect next(browser->mapToGlobal(QPoint()), browser->size());
      const QRect host(m_main->mapToGlobal(QPoint()), m_main->size());
      if (!host.contains(actual) || !host.contains(next)) {
        m_waitDetail = QStringLiteral("Projects or Browser extends outside the main window");
        return false;
      }
      auto* card = qobject_cast<GlassCard*>(projects);
      auto* browserCard = qobject_cast<GlassCard*>(browser);
      if (!card) {
        auto* browserRows = browser->findChild<QTreeWidget*>();
        for (QTreeWidget* rows : {tree, browserRows}) {
          if (!rows || rows->viewport()->height() < std::max(rows->fontMetrics().height() + 6,
                                                            rows->sizeHintForRow(0))) {
            // The window against its own minimum and the screen: a window
            // server that keeps the frame on a short screen leaves the
            // stacked docks less than their minimum.
            const QWidget* top = m_main->window();
            const QSize lowest = top->minimumSizeHint().expandedTo(top->minimumSize());
            const QSize room = top->screen() != nullptr ? top->screen()->availableGeometry().size() : QSize();
            m_waitDetail = QStringLiteral("a docked tree cannot show one complete row: '%1' viewport %2 high, "
                                          "row %3; window %4x%5 (frame %6 high), minimum %7x%8, screen available %9x%10")
                               .arg(rows ? rows->objectName() : QStringLiteral("browser rows missing"))
                               .arg(rows ? rows->viewport()->height() : -1)
                               .arg(rows ? std::max(rows->fontMetrics().height() + 6, rows->sizeHintForRow(0)) : -1)
                               .arg(top->width()).arg(top->height()).arg(top->frameGeometry().height())
                               .arg(lowest.width()).arg(lowest.height()).arg(room.width()).arg(room.height());
            return false;
          }
        }
      }
      qInfo() << "Projects layout: actual" << actual << "browser" << next
              << "slot" << (card ? card->slot() : QRect()) << "minimum" << projects->minimumSize();
      if (actual.intersects(next) || actual.bottom() >= next.top()) {
        m_waitDetail = QStringLiteral("Projects bottom %1 reaches Browser top %2")
            .arg(actual.bottom()).arg(next.top());
        return false;
      }
      if (card && (!browserCard || card->slot().size() != card->size() ||
                   card->slot().intersects(browserCard->slot()) ||
                   card->parentWidget()->mapToGlobal(card->slot().topLeft()) != actual.topLeft())) {
        m_waitDetail = QStringLiteral("actual card bounds differ from their slots");
        return false;
      }
      auto* toolbar = m_main->findChild<QToolBar*>(QStringLiteral("projectsToolbar"));
      if (!toolbar || ((!card || !card->isCollapsed()) &&
          (toolbar->height() < toolbar->sizeHint().height() || !actual.contains(
              QRect(toolbar->mapToGlobal(QPoint()), toolbar->size()))))) {
        m_waitDetail = QStringLiteral("toolbar height %1, preferred %2, Projects height %3")
            .arg(toolbar ? toolbar->height() : -1).arg(toolbar ? toolbar->sizeHint().height() : -1)
            .arg(projects->height());
        return false;
      }
      if (!card || !card->isCollapsed()) {
        for (auto* button : toolbar->findChildren<QToolButton*>()) {
          if (button->isVisible() && !toolbar->rect().contains(QRect(
                  button->mapTo(toolbar, QPoint()), button->size()))) {
            m_waitDetail = QStringLiteral("toolbar clips visible button '%1'").arg(button->objectName());
            return false;
          }
        }
      }
      qInfo() << "Projects layout: passed, projects" << tree->topLevelItemCount()
              << "DPR" << projects->devicePixelRatioF();
      return true;
    }, 6, QStringLiteral("Projects/Browser overlap, stale slot, clipped toolbar or project rows not loaded"));
  } else if (name == QLatin1String("expect-tree-rename")) {
    whenReady([this, a] {
      QStringList observed;
      const auto rectText = [](const QRect& rect) {
        return QStringLiteral("%1,%2 %3x%4").arg(rect.x()).arg(rect.y()).arg(rect.width()).arg(rect.height());
      };
      for (QLineEdit* editor : m_main->findChildren<QLineEdit*>(QStringLiteral("treeRenameEditor"))) {
        observed << QStringLiteral("'%1' selected '%2' visible %3")
                        .arg(editor->text(), editor->selectedText()).arg(editor->isVisible());
        if (!editor->isVisible() || editor->text() != a.at(0)) continue;
        m_waitDetail = QStringLiteral("editor '%1', selected '%2', rect %3")
                           .arg(editor->text(), editor->selectedText(), rectText(editor->geometry()));
        const auto reject = [this](const QString& reason) {
          m_waitDetail += QStringLiteral("; %1").arg(reason);
          return false;
        };
        auto* tree = qobject_cast<QTreeWidget*>(editor->parentWidget()->parentWidget());
        if (!tree) return reject(QStringLiteral("editor is not a tree viewport child"));
        const QModelIndex index = tree->currentIndex();
        m_waitDetail += QStringLiteral("; current row %1 column %2 value '%3'")
                            .arg(index.row()).arg(index.column()).arg(index.data().toString());
        auto* delegate = dynamic_cast<TreeItemDelegate*>(tree->itemDelegateForIndex(index));
        if (!delegate) return reject(QStringLiteral("no shared tree delegate"));
        if (!delegate->editing(index)) return reject(QStringLiteral("current index has no tracked editor"));
        if (editor->hasFrame()) return reject(QStringLiteral("editor has a native line-edit frame"));
        if (!editor->testAttribute(Qt::WA_OpaquePaintEvent)) return reject(QStringLiteral("editor lacks opaque paint attribute"));
        if (editor->palette().color(QPalette::Base).alpha() != 255) {
          return reject(QStringLiteral("editor Base alpha %1").arg(editor->palette().color(QPalette::Base).alpha()));
        }
        if (editor->selectedText() != editor->text()) return reject(QStringLiteral("editor text is not fully selected"));
        for (QPalette::ColorGroup group : {QPalette::Active, QPalette::Inactive}) {
          const QPalette colors = editor->palette();
          const QPalette application = QApplication::palette();
          for (QPalette::ColorRole role : {QPalette::Text, QPalette::Highlight, QPalette::HighlightedText}) {
            if (colors.color(group, role) != application.color(group, role)) {
              return reject(QStringLiteral("palette group %1 role %2: %3 instead of platform editor colour %4")
                                .arg(int(group)).arg(int(role))
                                .arg(colors.color(group, role).name(QColor::HexArgb),
                                     application.color(group, role).name(QColor::HexArgb)));
            }
          }
          if (colors.color(group, QPalette::Highlight).alpha() == 0) {
            return reject(QStringLiteral("palette group %1 has a transparent text-selection background").arg(int(group)));
          }
        }
        QStyleOptionViewItem cell;
        cell.initFrom(tree);
        cell.widget = tree;
        cell.rect = tree->visualRect(index);
        cell.font = tree->font();
        cell.fontMetrics = QFontMetrics(cell.font);
        cell.decorationSize = tree->iconSize();
        cell.state |= QStyle::State_Selected;
        // The editor must stay inside its row and cover the original name,
        // including the Browser's custom icon/bulb text margin.
        const QRect nameInk = delegate->nameTextRect(cell, index);
        const int editorTextLeft = editor->x() + editor->textMargins().left();
        m_waitDetail += QStringLiteral("; row %1, name ink %2, editor text left %3, font height %4")
                            .arg(rectText(cell.rect), rectText(nameInk)).arg(editorTextLeft).arg(cell.fontMetrics.height());
        if (!cell.rect.contains(editor->geometry())) return reject(QStringLiteral("editor extends outside its row"));
        if (editor->height() < cell.fontMetrics.height()) return reject(QStringLiteral("editor is shorter than the font"));
        if (nameInk.isEmpty()) return reject(QStringLiteral("old name ink rectangle is empty"));
        if (!editor->geometry().contains(nameInk)) return reject(QStringLiteral("editor does not cover the old name ink rectangle"));
        if (std::abs(editorTextLeft - nameInk.left()) > 2) return reject(QStringLiteral("editor text is horizontally misaligned"));
        const auto painted = [&](const QColor& ink) {
          QImage image(tree->viewport()->size(), QImage::Format_ARGB32_Premultiplied);
          image.fill(Qt::transparent);
          QPainter painter(&image);
          QStyleOptionViewItem option = cell;
          option.palette.setColor(QPalette::Text, ink);
          option.palette.setColor(QPalette::HighlightedText, ink);
          delegate->paint(&painter, option, index);
          return image;
        };
        // Changing only the text ink must not change a single row pixel:
        // the delegate must omit the old name while the editor is active.
        if (painted(Qt::magenta) != painted(Qt::green)) {
          return reject(QStringLiteral("delegate painting changes with text ink while the editor is open"));
        }
        QImage cover(editor->size(), QImage::Format_ARGB32_Premultiplied);
        cover.fill(Qt::transparent);
        editor->render(&cover);
        for (int y = 0; y < cover.height(); ++y) {
          for (int x = 0; x < cover.width(); ++x) {
            if (cover.pixelColor(x, y).alpha() != 255) {
              return reject(QStringLiteral("editor pixel %1,%2 has alpha %3")
                                .arg(x).arg(y).arg(cover.pixelColor(x, y).alpha()));
            }
          }
        }
        qInfo() << "Tree rename: passed" << editor->text() << editor->geometry() << "covers" << nameInk
                << "DPR" << editor->devicePixelRatioF();
        return true;
      }
      m_waitDetail = QStringLiteral("no visible editor with text '%1'; found %2").arg(a.at(0), observed.join(QStringLiteral(" | ")));
      return false;
    }, 6, QStringLiteral("rename editor is missing, translucent, misplaced, unselected or paints the old name"));
  } else if (name == QLatin1String("expect-no-qt-popup")) {
    QWidget* popup = QApplication::activePopupWidget();
    if (popup == nullptr) {
      for (QWidget* candidate : QApplication::topLevelWidgets()) {
        if (candidate->isVisible() && candidate->windowType() == Qt::Popup) {
          popup = candidate;
          break;
        }
      }
    }
    if (popup != nullptr) {
      fail(line, QStringLiteral("Qt popup '%1' (%2) owns input or remains logically visible")
                     .arg(popup->objectName(), QString::fromLatin1(popup->metaObject()->className())));
    }
  } else if (name == QLatin1String("expect-clear-of-cube")) {
    // expect-clear-of-cube <object-name>: wait until that visible widget
    // (a floating card) lies clear of the orientation cube's corner
    // (mitcad#2071: below a larger cube without a window resize).
    const QString widgetName = a.at(0);
    whenReady(
        [this, widgetName] {
          auto* window = qobject_cast<MainWindow*>(m_main.data());
          QWidget* widget = namedWidget(m_main.data(), widgetName);
          if (window == nullptr || widget == nullptr || !widget->isVisible()) {
            m_waitDetail = QStringLiteral("no visible %1").arg(widgetName);
            return false;
          }
          OcctViewer& view = window->viewer();
          const QRect cube = view.overlayCubeArea();
          const QRect card(view.mapFromGlobal(widget->mapToGlobal(QPoint(0, 0))), widget->size());
          m_waitDetail = QStringLiteral("%1 %2,%3 %4x%5, the cube's corner %6,%7 %8x%9")
                             .arg(widgetName)
                             .arg(card.x())
                             .arg(card.y())
                             .arg(card.width())
                             .arg(card.height())
                             .arg(cube.x())
                             .arg(cube.y())
                             .arg(cube.width())
                             .arg(cube.height());
          return !card.intersects(cube);
        },
        6, QStringLiteral("%1 is not clear of the orientation cube").arg(widgetName),
        [this] { qDebug().noquote() << QStringLiteral("TestDriver: clear of the cube: %1").arg(m_waitDetail); });
  } else if (name == QLatin1String("expect-widget-property")) {
    QWidget* window = targetWindow();
    QObject* widget = namedWidget(window, a.at(0));
    const QVariant value = widget ? widget->property(a.at(1).toUtf8().constData()) : QVariant();
    // A size as WxH (an item view's iconSize, mitcad#1610); others as
    // QVariant gives them as text.
    QString text = value.toString();
    if (value.typeId() == QMetaType::QSize) {
      const QSize size = value.toSize();
      text = QStringLiteral("%1x%2").arg(size.width()).arg(size.height());
    }
    if (!value.isValid() || text != a.at(2)) {
      fail(line, QStringLiteral("widget %1.%2 is '%3', expected '%4'").arg(a.at(0), a.at(1), text, a.at(2)));
    }
  } else if (name == QLatin1String("expect-widget-background")) {
    // The colour that covers most of a visible widget's area as its window
    // draws it (mostCommonColor).
    const QColor wanted(a.at(1));
    bool number = false;
    const int tolerance = a.at(2).toInt(&number);
    if (!wanted.isValid() || !number || tolerance < 0) {
      fail(line, QStringLiteral("expect-widget-background <object-name> <#rrggbb> <tolerance>"));
      return;
    }
    const QString widgetName = a.at(0);
    whenReady(
        [this, widgetName, wanted, tolerance] {
          const WidgetColor most = mostCommonColor(topWindow(), namedWidget(m_main.data(), widgetName), widgetName);
          if (!most.missing.isEmpty()) {
            m_waitDetail = most.missing;
            return false;
          }
          const QColor& seen = most.color;
          const int off = std::abs(seen.red() - wanted.red()) + std::abs(seen.green() - wanted.green()) +
                          std::abs(seen.blue() - wanted.blue());
          m_waitDetail = QStringLiteral("%1 %2 (%3 of %4 pixels), wanted %5, off by %6")
                             .arg(widgetName, seen.name())
                             .arg(most.count)
                             .arg(most.total)
                             .arg(wanted.name())
                             .arg(off);
          return off <= tolerance;
        },
        6, QStringLiteral("the background of %1 is not %2").arg(a.at(0), a.at(1)),
        [this] { qDebug().noquote() << QStringLiteral("TestDriver: widget background %1").arg(m_waitDetail); });
  } else if (name == QLatin1String("expect-widget-theme")) {
    // Whether that colour is a dark or a light one, as isDarkPalette decides
    // on a window (lightness below 128): the Start page and the Browser in
    // Windows 10's dark app mode (mitcad#1282), whatever colour exactly.
    const bool dark = a.at(1) == QLatin1String("dark");
    if (!dark && a.at(1) != QLatin1String("light")) {
      fail(line, QStringLiteral("expect-widget-theme <object-name> dark|light"));
      return;
    }
    const QString widgetName = a.at(0);
    whenReady(
        [this, widgetName, dark] {
          const WidgetColor most = mostCommonColor(topWindow(), namedWidget(m_main.data(), widgetName), widgetName);
          if (!most.missing.isEmpty()) {
            m_waitDetail = most.missing;
            return false;
          }
          const bool seenDark = most.color.lightness() < 128;
          m_waitDetail = QStringLiteral("%1 %2 (%3 of %4 pixels), lightness %5: %6")
                             .arg(widgetName, most.color.name())
                             .arg(most.count)
                             .arg(most.total)
                             .arg(most.color.lightness())
                             .arg(seenDark ? QStringLiteral("dark") : QStringLiteral("light"));
          return seenDark == dark;
        },
        6, QStringLiteral("the background of %1 is not %2").arg(a.at(0), a.at(1)),
        [this] { qDebug().noquote() << QStringLiteral("TestDriver: widget theme %1").arg(m_waitDetail); });
  } else if (name == QLatin1String("expect-dialog-gone")) {
    QRegularExpression re;
    if (!regex(a.at(0), re)) return;
    whenReady([this, re] {
      for (QWidget* widget : QApplication::topLevelWidgets()) {
        if (widget != m_main.data() && widget != topWindow() && widget->isVisible() && dialogMatches(widget, re)) {
          return false;
        }
      }
      return true;
    }, 6, QStringLiteral("a window matching '%1' remains visible").arg(a.at(0)));
  } else if (name == QLatin1String("reveal-widget")) {
    // A control can be visible in Qt yet clipped by a floating card's
    // scroll viewport. Reveal the identified control before a script
    // waits for its current place, without clicking a stale coordinate.
    whenReady([this, a] {
      // Revealing is not input: a disabled relation still has a place in
      // the palette. Only subsequent input requires an enabled control.
      QWidget* widget = namedWidget(m_main.data(), a.at(0));
      if (widget == nullptr || !widget->isVisible()) {
        QStringList candidates;
        if (!m_main.isNull()) {
          for (QWidget* candidate : m_main->findChildren<QWidget*>(a.at(0))) {
            candidates << QStringLiteral("%1 visible=%2 enabled=%3")
                              .arg(QString::fromLatin1(candidate->metaObject()->className()))
                              .arg(candidate->isVisible()).arg(candidate->isEnabled());
          }
        }
        m_waitDetail = QStringLiteral("no unique visible widget; candidates: %1")
                           .arg(candidates.join(QStringLiteral("; ")));
        return false;
      }
      for (QWidget* parent = widget->parentWidget(); parent; parent = parent->parentWidget()) {
        if (auto* scroll = qobject_cast<QScrollArea*>(parent)) {
          scroll->ensureWidgetVisible(widget);
        }
      }
      const bool inView = widget->visibleRegion().contains(widget->rect().center());
      m_waitDetail = QStringLiteral("visible=%1 enabled=%2 center in visible region=%3; size %4x%5")
                         .arg(widget->isVisible()).arg(widget->isEnabled()).arg(inView)
                         .arg(widget->width()).arg(widget->height());
      return inView;
    }, 6, QStringLiteral("named widget '%1' is not in view").arg(a.at(0)), [a] {
      qDebug().noquote() << QStringLiteral("TestDriver: revealed widget '%1'").arg(a.at(0));
    });
  } else if (name == QLatin1String("focus-widget")) {
    QWidget* window = QApplication::activePopupWidget();
    if (!window) window = targetWindow();
    QWidget* widget = namedWidget(window, a.at(0), true);
    const auto describe = [](const QWidget* value) {
      if (value == nullptr) return QStringLiteral("none");
      const QRect rect = value->geometry();
      return QStringLiteral("%1 name='%2' title='%3' visible=%4 hidden=%5 enabled=%6 rect=%7,%8 %9x%10")
          .arg(QString::fromLatin1(value->metaObject()->className()), value->objectName(), value->windowTitle())
          .arg(value->isVisible()).arg(value->isHidden()).arg(value->isEnabled())
          .arg(rect.x()).arg(rect.y()).arg(rect.width()).arg(rect.height());
    };
    // Record the real lookup context before changing focus. A floating
    // command card, a still-active combo popup and a hidden duplicate field
    // otherwise all reduce to the same unhelpful "no visible widget" error.
    qInfo().noquote() << QStringLiteral("Qt field focus context: id='%1'; popup={%2}; active={%3}; target={%4}; search={%5}")
        .arg(a.at(0), describe(QApplication::activePopupWidget()), describe(QApplication::activeWindow()),
             describe(targetWindow()), describe(window));
    int candidates = 0;
    for (QWidget* candidate : QApplication::allWidgets()) {
      if (candidate->objectName() != a.at(0)) continue;
      ++candidates;
      QStringList ancestry;
      for (QWidget* parent = candidate->parentWidget(); parent != nullptr; parent = parent->parentWidget()) {
        ancestry << describe(parent);
      }
      qInfo().noquote() << QStringLiteral("Qt field focus candidate: selected=%1; field={%2}; window={%3}; host={%4}; parents=[%5]")
          .arg(candidate == widget).arg(describe(candidate), describe(candidate->window()),
               describe(GlassCard::hostWindow(candidate)), ancestry.join(QStringLiteral("; ")));
    }
    if (candidates == 0) qInfo().noquote() << QStringLiteral("Qt field focus candidate: none for '%1'").arg(a.at(0));
    if (!widget || !widget->isVisible() || !widget->isEnabled()) {
      fail(line, QStringLiteral("no unique visible enabled widget '%1'").arg(a.at(0))); return;
    }
    m_dialog = widget->window();
    widget->setFocus(Qt::TabFocusReason);
  } else if (name == QLatin1String("dialog-resize")) {
    double width = 0;
    double height = 0;
    QWidget* window = targetWindow();
    if (!window || window == m_main.data() || !toNumber(a.at(0), width) || !toNumber(a.at(1), height) ||
        width < 300 || height < 200 || width > 3000 || height > 2000) {
      fail(line, QStringLiteral("dialog-resize requires a focused dialog and width 300..3000, height 200..2000"));
      return;
    }
    window->resize(qRound(width), qRound(height));
    m_notBefore = std::max(m_notBefore, m_clock.elapsed() + 300);
  } else if (name == QLatin1String("invoke-widget")) {
    const QPointer<QWidget> window = targetWindow();
    const QString widgetName = a.at(0);
    const QByteArray method = a.at(1).toUtf8();
    const QString text = a.size() > 2 ? a.at(2) : QString();
    const bool withText = a.size() > 2;
    whenReady([this, window, widgetName] {
      if (window.isNull()) {
        return false;
      }
      // The window itself, or a widget in it.
      m_found = window->objectName() == widgetName ? window.data() : namedWidget(window.data(), widgetName);
      return !m_found.isNull();
    }, 6, QStringLiteral("no widget named %1").arg(widgetName), [this, line, method, text, withText] {
      const bool invoked = withText ? QMetaObject::invokeMethod(m_found.data(), method.constData(), Q_ARG(QString, text))
                                    : QMetaObject::invokeMethod(m_found.data(), method.constData());
      if (!invoked) {
        fail(line, QStringLiteral("%1 has no invokable %2").arg(m_found->objectName(), QString::fromUtf8(method)));
      }
    });
  } else if (name == QLatin1String("expect-dialog-appearance")) {
    const bool dark = a.at(0) == QStringLiteral("dark");
    if (!dark && a.at(0) != QStringLiteral("light")) {
      fail(line, QStringLiteral("expect-dialog-appearance light|dark")); return;
    }
    whenReady([this, dark] {
      QWidget* window = targetWindow();
      if (!window || !window->isVisible()) return false;
      const QPalette palette = window->palette();
      const QColor background = palette.color(QPalette::Window);
      const QColor text = palette.color(QPalette::WindowText);
      const double ratio = window->devicePixelRatioF();
      m_waitDetail = QStringLiteral("actual dialog palette window %1, text %2, device scale %3")
                         .arg(background.name(), text.name()).arg(ratio);
      const double minimumScale = qEnvironmentVariableIntValue("MITCAD_UI_REQUIRE_RETINA") == 1 ? 2 : 1;
      const bool matches = isDarkPalette(palette) == dark &&
          (dark ? text.lightness() > background.lightness() : text.lightness() < background.lightness()) &&
          ratio >= minimumScale;
      if (matches) qDebug().noquote() << "TestDriver:" << m_waitDetail;
      return matches;
    }, 6, QStringLiteral("dialog did not adopt the %1 appearance at the required scale").arg(a.at(0)));
  }
}

} // namespace mitcad
