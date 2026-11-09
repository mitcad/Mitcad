// SPDX-License-Identifier: MIT
#include "TestDriver.hpp"

#include <algorithm>
#include <cmath>

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAbstractSpinBox>
#include <QAction>
#include <QAccessible>
#include <QComboBox>
#include <QApplication>
#include <QDebug>
#include <QFontInfo>
#include <QLayout>
#include <QLineEdit>
#include <QMenu>
#include <QRegion>
#include <QScreen>
#include <QScrollArea>
#include <QScrollBar>
#include <QStringList>
#include <QToolBar>
#include <QToolButton>
#include <QTabBar>

#include "ChromeStyle.hpp"
#include "Ribbon.hpp"
#include "TitleBar.hpp"
#include "UiSizes.hpp"

namespace mitcad {

void TestDriver::testRibbonLabels(const QStringList& expected) {
  whenReady([this, expected] {
    auto* ribbon = m_main ? m_main->findChild<Ribbon*>() : nullptr;
    QTabBar* tabs = ribbon ? ribbon->tabBar() : nullptr;
    if (tabs == nullptr || tabs->count() != expected.size()) return false;
    QAccessibleInterface* accessible = QAccessible::queryAccessibleInterface(tabs);
    for (int i = 0; i < tabs->count(); ++i) {
      if (tabs->tabText(i) != expected.at(i)) return false;
      QAccessibleInterface* tab = accessible ? accessible->child(i) : nullptr;
      if (tab == nullptr || tab->text(QAccessible::Name) != expected.at(i)) return false;
    }
    for (QToolButton* button : ribbon->findChildren<QToolButton*>()) {
      if (!button->objectName().startsWith(QLatin1String("ribbonMenu_"))) continue;
      // These captions are ordinary words (Create, Sheet Metal, ...), not
      // file formats or user names. Check the actual text used by painting.
      const QString text = button->text();
      if (text.isEmpty() || text == text.toUpper() || button->font().capitalization() == QFont::AllUppercase) {
        m_waitDetail = QStringLiteral("all-caps group caption: %1").arg(text);
        return false;
      }
      if (!button->isVisible()) continue;
      if (button->width() < button->sizeHint().width() || button->menu() == nullptr || button->menu()->isEmpty()) {
        m_waitDetail = QStringLiteral("clipped caption or missing group menu: %1 (%2 < %3)")
                           .arg(text).arg(button->width()).arg(button->sizeHint().width());
        return false;
      }
      // Overflow may put a group outside the current viewport, but ordinary
      // horizontal scrolling must be able to expose its entire menu button.
      for (QWidget* parent = button->parentWidget(); parent != nullptr && parent != ribbon; parent = parent->parentWidget()) {
        auto* scroll = qobject_cast<QScrollArea*>(parent);
        if (scroll == nullptr || scroll->objectName() != QLatin1String("ribbonPageScroll")) continue;
        const QRect caption(button->mapTo(scroll->widget(), QPoint()), button->size());
        const int viewport = scroll->viewport()->width();
        QScrollBar* bar = scroll->horizontalScrollBar();
        const QRect reachable(0, 0, viewport + bar->maximum(), scroll->viewport()->height());
        QWidget* wrapper = scroll->parentWidget();
        auto* back = wrapper->findChild<QToolButton*>(QStringLiteral("ribbonScrollLeft"));
        auto* next = wrapper->findChild<QToolButton*>(QStringLiteral("ribbonScrollRight"));
        const auto exposed = [ribbon, wrapper](QToolButton* arrow) {
          if (arrow == nullptr || !arrow->isVisible() || arrow->width() <= 0 || arrow->height() <= 0 ||
              arrow->width() < arrow->sizeHint().width() || arrow->height() < arrow->sizeHint().height()) return false;
          return wrapper->rect().contains(QRect(arrow->mapTo(wrapper, QPoint()), arrow->size())) &&
                 ribbon->rect().contains(QRect(arrow->mapTo(ribbon, QPoint()), arrow->size())) &&
                 QRegion(arrow->rect()).subtracted(arrow->visibleRegion()).isEmpty();
        };
        if (caption.width() > viewport || !reachable.contains(caption) || back == nullptr || next == nullptr ||
            (bar->maximum() > 0 && (!exposed(back) || !exposed(next))) ||
            back->isEnabled() != (bar->value() > bar->minimum()) || next->isEnabled() != (bar->value() < bar->maximum())) {
          const auto metrics = [](const QWidget* widget) {
            if (widget == nullptr) return QStringLiteral("missing");
            QString detail;
            QDebug(&detail).nospace() << widget->objectName() << " rect=" << widget->geometry()
                << " hint=" << widget->sizeHint() << " minimumHint=" << widget->minimumSizeHint()
                << " minimum=" << widget->minimumSize() << " visible=" << widget->isVisible()
                << " enabled=" << widget->isEnabled() << " exposed=" << widget->visibleRegion().boundingRect();
            return detail;
          };
          m_waitDetail = QStringLiteral("ribbon overflow cannot expose %1: caption=%2x%3; wrapper={%4}; "
                                       "scroll={%5}; viewport={%6}; content={%7}; left={%8}; right={%9}; range=%10..%11 value=%12")
              .arg(text).arg(caption.width()).arg(caption.height()).arg(metrics(wrapper), metrics(scroll),
                  metrics(scroll->viewport()), metrics(scroll->widget()), metrics(back), metrics(next))
              .arg(bar->minimum()).arg(bar->maximum()).arg(bar->value());
          return false;
        }
      }
    }
    qInfo() << "Ribbon labels: passed" << expected;
    return true;
  }, 6, QStringLiteral("workspace labels/accessibility, complete captions or reachable group menus differ"));
}

void TestDriver::testDesignChrome() {
  whenReady([this] {
    QWidget* main = m_main.data();
    QWidget* top = topWindow();
    QWidget* view = viewer();
    auto* ribbon = main != nullptr ? main->findChild<Ribbon*>() : nullptr;
    if (main == nullptr || top == nullptr || view == nullptr || ribbon == nullptr) {
      m_waitDetail = QStringLiteral("no design window, view or ribbon");
      return false;
    }
    const bool docked = chromeStyle() == ChromeStyle::Docked;
    // In the top-level window's coordinates, as the log's places are.
    const auto place = [top](const QWidget* widget) { return QRect(widget->mapTo(top, QPoint(0, 0)), widget->size()); };
    const auto text = [](const QRect& r) {
      return QStringLiteral("%1,%2 %3x%4").arg(r.x()).arg(r.y()).arg(r.width()).arg(r.height());
    };
    // Shown, not empty, inside the window and not all covered.
    const auto shown = [top, &place](const QWidget* widget) {
      return widget != nullptr && widget->isVisible() && !widget->size().isEmpty() &&
             top->rect().contains(place(widget)) && !widget->visibleRegion().isEmpty();
    };
    QWidget* holder = docked ? main->findChild<QToolBar*>(QStringLiteral("ribbonToolbar"), Qt::FindDirectChildrenOnly)
                             : static_cast<QWidget*>(ribbon);
    QWidget* browser = main->findChild<QWidget*>(QStringLiteral("browserDock"));
    QWidget* navigation = main->findChild<QWidget*>(QStringLiteral("navigationBarButtons"));
    QStringList missing;
    if (!shown(holder) || !ribbon->isVisible() || (!docked && ribbon->groupCards().empty())) {
      missing << QStringLiteral("the ribbon");
    } else if (docked && place(view).top() <= place(holder).bottom()) {
      missing << QStringLiteral("the view below the ribbon (view top %1, ribbon bottom %2)")
                     .arg(place(view).top())
                     .arg(place(holder).bottom());
    }
    if (!shown(browser)) {
      missing << QStringLiteral("the Browser");
    }
    if (!shown(navigation)) {
      missing << QStringLiteral("the navigation bar");
    }
    if (!view->isVisible() || view->size().isEmpty()) {
      missing << QStringLiteral("the view");
    }
    if (!missing.isEmpty()) {
      m_waitDetail = QStringLiteral("not shown: %1").arg(missing.join(QStringLiteral(", ")));
      return false;
    }
    qInfo().noquote() << QStringLiteral("Design chrome: %1, ribbon %2, view %3, browser %4, navigation bar %5")
                             .arg(docked ? QStringLiteral("docked") : QStringLiteral("floating"), text(place(holder)),
                                  text(place(view)), text(place(browser)), text(place(navigation)));
    return true;
  }, 6, QStringLiteral("the design in front does not show its ribbon, Browser and navigation bar"));
}

void TestDriver::testInterfaceSize(double ratio) {
  whenReady([this, ratio] {
    QWidget* main = m_main.data();
    QWidget* top = topWindow();
    QWidget* view = viewer();
    auto* ribbon = main != nullptr ? main->findChild<Ribbon*>() : nullptr;
    if (main == nullptr || top == nullptr || view == nullptr || ribbon == nullptr) {
      m_waitDetail = QStringLiteral("no design window, view or ribbon");
      return false;
    }
    const auto sameRatio = [](double a, double b) { return std::abs(a - b) < 0.01; };
    // The window and the 3D view draw at the ratio the Interface size gives
    // (0: any ratio, mitcad#1556).
    if (ratio > 0.0 &&
        (!sameRatio(top->devicePixelRatioF(), ratio) || !sameRatio(view->devicePixelRatioF(), ratio))) {
      m_waitDetail = QStringLiteral("device pixel ratio %1 (view %2), expected %3")
                         .arg(top->devicePixelRatioF())
                         .arg(view->devicePixelRatioF())
                         .arg(ratio);
      return false;
    }
    // The window within the screen's logical area.
    const QScreen* screen = top->screen();
    if (screen == nullptr || !screen->availableGeometry().contains(top->frameGeometry())) {
      QString detail;
      QDebug(&detail).nospace() << "window " << top->frameGeometry() << " outside the screen's "
                                << (screen != nullptr ? screen->availableGeometry() : QRect());
      m_waitDetail = detail;
      return false;
    }
    // In the top-level window's coordinates.
    const auto place = [top](const QWidget* widget) { return QRect(widget->mapTo(top, QPoint(0, 0)), widget->size()); };
    const auto text = [](const QRect& r) {
      return QStringLiteral("%1,%2 %3x%4").arg(r.x()).arg(r.y()).arg(r.width()).arg(r.height());
    };
    // Shown, not empty and inside the window.
    const auto shown = [top, &place](const QWidget* widget) {
      return widget != nullptr && widget->isVisible() && !widget->size().isEmpty() &&
             top->rect().contains(place(widget)) && !widget->visibleRegion().isEmpty();
    };
    // Shown and nothing of it cut off by its parents or covered.
    const auto whole = [&shown](const QWidget* widget) {
      return shown(widget) && QRegion(widget->rect()).subtracted(widget->visibleRegion()).isEmpty();
    };
    const bool docked = chromeStyle() == ChromeStyle::Docked;

    // The ribbon: its tabs shown, every group's caption complete, every
    // button at least as large as its icon.
    QWidget* holder = docked ? main->findChild<QToolBar*>(QStringLiteral("ribbonToolbar"), Qt::FindDirectChildrenOnly)
                             : static_cast<QWidget*>(ribbon);
    if (!shown(holder) || !ribbon->isVisible() || (!docked && ribbon->groupCards().empty()) ||
        !shown(ribbon->tabBar())) {
      m_waitDetail = QStringLiteral("the ribbon or its tabs not shown whole");
      return false;
    }
    int captions = 0;
    int buttons = 0;
    for (QToolButton* button : ribbon->findChildren<QToolButton*>()) {
      // The overflow's scroll arrows are drawn smaller than an icon.
      if (!button->isVisible() || button->objectName().startsWith(QLatin1String("ribbonScroll"))) continue;
      if (button->objectName().startsWith(QLatin1String("ribbonMenu_"))) {
        if (button->width() < button->sizeHint().width()) {
          m_waitDetail = QStringLiteral("clipped ribbon caption %1 (%2 < %3)")
                             .arg(button->text())
                             .arg(button->width())
                             .arg(button->sizeHint().width());
          return false;
        }
        ++captions;
      } else if (!button->icon().isNull() && button->toolButtonStyle() != Qt::ToolButtonTextOnly) {
        if (button->height() < button->iconSize().height() || button->width() < button->iconSize().width()) {
          m_waitDetail = QStringLiteral("ribbon button %1 smaller than its icon (%2x%3 < %4x%5)")
                             .arg(button->objectName())
                             .arg(button->width())
                             .arg(button->height())
                             .arg(button->iconSize().width())
                             .arg(button->iconSize().height());
          return false;
        }
        ++buttons;
      }
    }
    if (captions == 0) {
      m_waitDetail = QStringLiteral("no ribbon group caption shown");
      return false;
    }

    // The Browser: shown in the window, its first row as high as its icon.
    QWidget* browser = main->findChild<QWidget*>(QStringLiteral("browserDock"));
    auto* tree = main->findChild<QAbstractItemView*>(QStringLiteral("modelBrowserTree"));
    if (!shown(browser) || tree == nullptr || !shown(tree) || tree->model() == nullptr ||
        tree->model()->rowCount(tree->rootIndex()) == 0) {
      m_waitDetail = QStringLiteral("the Browser or its rows not shown");
      return false;
    }
    const QRect row = tree->visualRect(tree->model()->index(0, 0, tree->rootIndex()));
    const int iconHeight = tree->iconSize().isValid() ? tree->iconSize().height() : 0;
    if (row.height() < iconHeight || !tree->viewport()->rect().contains(row.topLeft())) {
      m_waitDetail = QStringLiteral("the Browser's first row %1 high, its icons %2").arg(row.height()).arg(iconHeight);
      return false;
    }

    // The command panel: in the window, OK (or Close) whole with its text,
    // every field at least as high as its text needs.
    QWidget* panel = nullptr;
    for (QWidget* candidate : main->findChildren<QWidget*>(QStringLiteral("commandPanel"))) {
      if (candidate->isVisible()) panel = candidate;
    }
    // A panel longer than its card scrolls in its page's scroll area (Text
    // size 200 % in a 1024 x 768 window): the view it shows through is
    // inside the window, not the whole panel.
    const QWidget* panelView = panel;
    if (panel != nullptr && panel->parentWidget() != nullptr) {
      const auto* area = qobject_cast<const QAbstractScrollArea*>(panel->parentWidget()->parentWidget());
      if (area != nullptr && area->viewport() == panel->parentWidget()) {
        panelView = area;
      }
    }
    if (!shown(panelView) || panel->visibleRegion().isEmpty()) {
      m_waitDetail = QStringLiteral("no command panel shown");
      return false;
    }
    // In the panel, or under it where a page lays its buttons out itself
    // (CommandPanel::takeButtons).
    QAbstractButton* ok = nullptr;
    for (QAbstractButton* button : main->findChildren<QAbstractButton*>()) {
      if (button->isVisible() && (button->objectName() == QLatin1String("commandOk") ||
                                  button->objectName() == QLatin1String("commandClose"))) {
        ok = button;
      }
    }
    if (ok == nullptr || !whole(ok) || ok->width() < ok->fontMetrics().horizontalAdvance(ok->text())) {
      m_waitDetail = QStringLiteral("the command panel's OK not shown whole");
      return false;
    }
    int fields = 0;
    for (QWidget* field : panel->findChildren<QWidget*>()) {
      if (!field->isVisible() || (qobject_cast<QLineEdit*>(field) == nullptr &&
                                  qobject_cast<QAbstractSpinBox*>(field) == nullptr &&
                                  qobject_cast<QComboBox*>(field) == nullptr)) {
        continue;
      }
      // A spin box's or a combo box's own line edit counts with its box.
      if (qobject_cast<QAbstractSpinBox*>(field->parentWidget()) != nullptr ||
          qobject_cast<QComboBox*>(field->parentWidget()) != nullptr) {
        continue;
      }
      if (field->height() < field->minimumSizeHint().height()) {
        m_waitDetail = QStringLiteral("command panel field %1 %2 high, its text needs %3")
                           .arg(field->objectName())
                           .arg(field->height())
                           .arg(field->minimumSizeHint().height());
        return false;
      }
      ++fields;
    }
    if (fields == 0) {
      m_waitDetail = QStringLiteral("no field in the command panel");
      return false;
    }
    qInfo().noquote() << QStringLiteral("Interface size layout: %1, device pixel ratio %2, window %3x%4 within the "
                                        "screen; ribbon %5 (%6 captions, %7 buttons); browser %8 (rows %9 high); "
                                        "command panel %10 (%11 fields, OK whole)")
                             .arg(docked ? QStringLiteral("docked") : QStringLiteral("floating"))
                             .arg(top->devicePixelRatioF())
                             .arg(top->width())
                             .arg(top->height())
                             .arg(text(place(holder)))
                             .arg(captions)
                             .arg(buttons)
                             .arg(text(place(browser)))
                             .arg(row.height())
                             .arg(text(place(panelView)))
                             .arg(fields);
    return true;
  }, 10, QStringLiteral("the window, ribbon, Browser or command panel does not fit at the Interface size"));
}

void TestDriver::testTextIconSize(int textPercent, int iconPercent, const QString& family) {
  whenReady([this, textPercent, iconPercent, family] {
    QWidget* main = m_main.data();
    auto* ribbon = main != nullptr ? main->findChild<Ribbon*>() : nullptr;
    auto* tree = main != nullptr ? main->findChild<QAbstractItemView*>(QStringLiteral("modelBrowserTree")) : nullptr;
    if (main == nullptr || ribbon == nullptr || tree == nullptr) {
      m_waitDetail = QStringLiteral("no design window, ribbon or Browser");
      return false;
    }
    // As the start applied them.
    if (ui_sizes_detail::textPercent != textPercent || ui_sizes_detail::iconPercent != iconPercent) {
      m_waitDetail = QStringLiteral("the run's Text size %1% and Icon size %2%, expected %3% and %4%")
                         .arg(ui_sizes_detail::textPercent)
                         .arg(ui_sizes_detail::iconPercent)
                         .arg(textPercent)
                         .arg(iconPercent);
      return false;
    }
    // The application font: its family as drawn, its size the platform's
    // times Text size.
    const QFont font = QApplication::font();
    const QString drawn = QFontInfo(font).family();
    const bool systemFont = family == QLatin1String("system");
    if (systemFont ? ui_sizes_detail::fontChosen
               : (!ui_sizes_detail::fontChosen || drawn.compare(family, Qt::CaseInsensitive) != 0)) {
      m_waitDetail = QStringLiteral("the application font is %1 (%2), expected %3")
                         .arg(drawn, font.family(), family);
      return false;
    }
    const double platform = ui_sizes_detail::platformPointSize;
    if (platform > 0.0 && std::abs(font.pointSizeF() - platform * textPercent / 100.0) > 0.01) {
      m_waitDetail = QStringLiteral("the application font is %1 pt, expected %2% of %3 pt")
                         .arg(font.pointSizeF())
                         .arg(textPercent)
                         .arg(platform);
      return false;
    }
    // The fixed sizes times Icon size, rounded to whole pixels; counted
    // here, not by uiIconSize.
    const auto scaled = [iconPercent](int pixels) { return std::max(1, (pixels * iconPercent + 50) / 100); };
    const QSize browserIcon(scaled(18), scaled(18));
    if (tree->iconSize() != browserIcon) {
      m_waitDetail = QStringLiteral("the Browser's icons %1x%2, expected %3x%4")
                         .arg(tree->iconSize().width())
                         .arg(tree->iconSize().height())
                         .arg(browserIcon.width())
                         .arg(browserIcon.height());
      return false;
    }
    const int ribbonSide = scaled(chromeStyle() == ChromeStyle::Floating ? 26 : 28);
    const QSize ribbonIcon(ribbonSide, ribbonSide);
    int buttons = 0;
    int quick = 0;
    for (QToolButton* button : ribbon->findChildren<QToolButton*>()) {
      // The command buttons: shown, with an icon, not a group's caption nor
      // the overflow's arrows.
      if (!button->isVisible() || button->icon().isNull() || button->toolButtonStyle() == Qt::ToolButtonTextOnly ||
          button->objectName().startsWith(QLatin1String("ribbonMenu_")) ||
          button->objectName().startsWith(QLatin1String("ribbonScroll"))) {
        continue;
      }
      // The quick-access group's buttons in the tabs' row: 18 pixels times
      // Icon size, smaller (down to 12 times it) where the row is compact,
      // not the 28-pixel command buttons.
      if (button->objectName().startsWith(QLatin1String("quickAccess_"))) {
        const QSize icon = button->iconSize();
        if (icon.width() != icon.height() || icon.width() < scaled(12) || icon.width() > scaled(18) ||
            button->width() < icon.width() || button->height() < icon.height()) {
          m_waitDetail = QStringLiteral("quick-access button %1 %2x%3 with icons %4x%5, expected icons %6 to %7")
                             .arg(button->objectName())
                             .arg(button->width())
                             .arg(button->height())
                             .arg(icon.width())
                             .arg(icon.height())
                             .arg(scaled(12))
                             .arg(scaled(18));
          return false;
        }
        ++quick;
        continue;
      }
      if (button->iconSize() != ribbonIcon || button->width() < ribbonSide || button->height() < ribbonSide) {
        m_waitDetail = QStringLiteral("ribbon button %1 %2x%3 with icons %4x%5, expected icons %6x%6")
                           .arg(button->objectName())
                           .arg(button->width())
                           .arg(button->height())
                           .arg(button->iconSize().width())
                           .arg(button->iconSize().height())
                           .arg(ribbonSide);
        return false;
      }
      ++buttons;
    }
    if (buttons == 0) {
      m_waitDetail = QStringLiteral("no ribbon command button shown");
      return false;
    }
    qInfo().noquote() << QStringLiteral("Text and icon size layout: font %1, %2 pt; browser icons %3x%4; ribbon %5 "
                                        "buttons, icons %6x%6; quick access %7 buttons")
                             .arg(drawn)
                             .arg(font.pointSizeF())
                             .arg(browserIcon.width())
                             .arg(browserIcon.height())
                             .arg(buttons)
                             .arg(ribbonSide)
                             .arg(quick);
    return true;
  }, 10, QStringLiteral("the application font, the Browser's or the ribbon's icons are not at Text and Icon size"));
}

void TestDriver::testSketchFinishLayout() {
  whenReady([this] {
    QWidget* window = m_main.data();
    if (window == nullptr || window->findChild<QWidget*>(QStringLiteral("titleFinishSketch")) != nullptr) {
      return false;
    }
    auto* action = window->findChild<QAction*>(QStringLiteral("command_sketch.finish"));
    auto* ribbon = window->findChild<Ribbon*>();
    auto* palette = window->findChild<QToolButton*>(QStringLiteral("paletteFinish"));
    auto* group = ribbon ? ribbon->findChild<QToolButton*>(QStringLiteral("ribbonMenu_SKETCH/FINISH SKETCH")) : nullptr;
    if (action == nullptr || !action->isEnabled() || ribbon == nullptr || group == nullptr ||
        !group->isVisible() || group->menu() == nullptr || !group->menu()->actions().contains(action) ||
        palette == nullptr || !palette->isVisible() || !palette->isEnabled() || palette->defaultAction() != action) {
      return false;
    }
    if (!ribbon->rect().contains(QRect(group->mapTo(ribbon, QPoint()), group->size()))) {
      return false;
    }
    auto* title = window->findChild<TitleBar*>();
    if (chromeStyle() == ChromeStyle::Floating && title == nullptr) {
      return false;
    }
    if (title != nullptr) {
      for (QToolButton* button : title->findChildren<QToolButton*>()) {
        if (button->defaultAction() == action) return false;
      }
      QWidget* search = title->findChild<QWidget*>(QStringLiteral("titleSearch"));
      QLayout* row = title->layout();
      // Search is the final layout item and reaches its normal right margin:
      // removing Finish Sketch must release its whole slot and spacing.
      if (search == nullptr || row == nullptr || row->indexOf(search) != row->count() - 1 ||
          title->width() - search->geometry().right() - 1 != row->contentsMargins().right()) {
        return false;
      }
      if (title->isFloatingRow()) {
        QRegion controls;
        for (const auto& part : title->rowParts()) {
          if (part.first == QLatin1String("titleFinishSketch")) return false;
          controls += part.second;
        }
        // All remaining space passes through to the viewport, including the
        // former Finish Sketch slot. No stale mouse mask may survive.
        if (controls != title->mask()) return false;
      }
    }
    qInfo() << "Sketch finish layout: title none, ribbon and palette enabled, no reserved title slot";
    return true;
  }, 6, QStringLiteral("duplicate title Finish Sketch, stale title slot/mask, or missing ribbon/palette action"));
}

} // namespace mitcad
