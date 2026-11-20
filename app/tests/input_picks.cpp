// SPDX-License-Identifier: MIT
// A selection input's picks listed by name on demand (mitcad#2064, GitHub
// mitcad/Mitcad#473): the picks' words with each owner asked once, the
// chip's tooltip (the first ten, then "and N more", above the hint), the
// lists' one open state in the settings and its notice to the panels, and
// the row of a list under the pointer. On Qt's offscreen platform.
#include "UnitTests.hpp"
#include "framework/InputPicks.hpp"

#include <QCoreApplication>
#include <QEvent>
#include <QListWidget>
#include <QMouseEvent>
#include <QPointingDevice>
#include <QSettings>
#include <QTemporaryDir>

#include <cstdio>
#include <vector>

namespace {

// A move of the pointer to `local` in the list's viewport, as the pointer's
// moves reach it.
void moveOver(QListWidget& list, const QPoint& local) {
  QWidget* viewport = list.viewport();
  const QPoint global = viewport->mapToGlobal(local);
  QMouseEvent event(QEvent::MouseMove, QPointF(local), QPointF(viewport->window()->mapFromGlobal(global)),
                    QPointF(global), Qt::NoButton, Qt::NoButton, Qt::NoModifier,
                    QPointingDevice::primaryPointingDevice());
  QCoreApplication::sendEvent(viewport, &event);
}

} // namespace

int inputPicksTests() {
  using namespace mitcad;
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "input picks: %s\n", message);
      ++failures;
    }
  };

  // The words: picks of one kind on one owner asked once, construction
  // geometry one by one.
  {
    const Selection items = {
        {SelectKind::Edge, QStringLiteral("F1.b0"), QStringLiteral("E{F1:top|F1:side0}"), QString()},
        {SelectKind::Edge, QStringLiteral("F1.b0"), QStringLiteral("E{F1:top|F1:side2}"), QString()},
        {SelectKind::Face, QStringLiteral("F1.b0"), QStringLiteral("F1:top"), QString()},
        {SelectKind::Body, QStringLiteral("F2.b0"), QString(), QString()},
        {SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E{F2:top|F2:side0}"), QString()},
        {SelectKind::Plane, QStringLiteral("mount"), QStringLiteral("end_a"), QString()},
        {SelectKind::Plane, QStringLiteral("mount"), QStringLiteral("end_b"), QString()},
    };
    int asked = 0;
    const QStringList words = pickWords(items, [&asked](const SelectionItem& item) {
      ++asked;
      const QString owner = item.owner == QStringLiteral("F1.b0") ? QStringLiteral("Body1") : QStringLiteral("Body2");
      switch (item.kind) {
      case SelectKind::Edge:
        return QStringLiteral("Edge of %1").arg(owner);
      case SelectKind::Face:
        return QStringLiteral("Face of %1").arg(owner);
      case SelectKind::Plane:
        return QStringLiteral("Frame %1").arg(item.name);
      default:
        return owner;
      }
    });
    check(words == QStringList({QStringLiteral("Edge of Body1"), QStringLiteral("Edge of Body1"),
                                QStringLiteral("Face of Body1"), QStringLiteral("Body2"),
                                QStringLiteral("Edge of Body2"), QStringLiteral("Frame end_a"),
                                QStringLiteral("Frame end_b")}),
          "one word for each pick, in order");
    check(asked == 6, "each owner asked once for its kind, each plane alone");
    check(pickWords({}, [](const SelectionItem&) { return QString(); }).isEmpty(), "no picks, no words");
  }

  // The tooltip.
  {
    const QString hint = QStringLiteral("Click to pick Objects");
    check(inputPicksTooltip({}, hint) == hint, "without picks the hint alone");
    check(inputPicksTooltip({QStringLiteral("Body1"), QStringLiteral("Body2")}, hint) ==
              QStringLiteral("Body1\nBody2\nClick to pick Objects"),
          "the picks one a line above the hint");
    QStringList many;
    for (int i = 1; i <= 12; ++i) {
      many << QStringLiteral("Edge %1").arg(i);
    }
    const QStringList lines = inputPicksTooltip(many, hint).split(QLatin1Char('\n'));
    check(lines.size() == kInputPicksTipItems + 2, "ten picks, the rest counted, the hint");
    check(lines.value(kInputPicksTipItems - 1) == QStringLiteral("Edge 10"), "the tenth pick last");
    check(lines.value(kInputPicksTipItems) == QStringLiteral("and 2 more"), "then how many more");
    check(lines.last() == hint, "the hint at the end");
    many = many.mid(0, kInputPicksTipItems);
    check(!inputPicksTooltip(many, hint).contains(QStringLiteral("more")), "ten picks: none more");
  }

  // One open state, in the settings of the test's own, told to the panels.
  {
    QTemporaryDir directory;
    check(directory.isValid(), "a settings folder");
    const QString organization = QCoreApplication::organizationName();
    const QString application = QCoreApplication::applicationName();
    const QSettings::Format format = QSettings::defaultFormat();
    QCoreApplication::setOrganizationName(QStringLiteral("MitcadInputPicksTest"));
    QCoreApplication::setApplicationName(QStringLiteral("MitcadInputPicksTest"));
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
    {
      QSettings settings;
      settings.clear();
      settings.sync();
    }
    std::vector<bool> told;
    const QMetaObject::Connection connection =
        QObject::connect(&InputPicksState::instance(), &InputPicksState::shownChanged,
                         [&told](bool shown) { told.push_back(shown); });
    check(!inputPicksShown(), "closed by default");
    setInputPicksShown(true);
    check(inputPicksShown(), "opened");
    check(told == std::vector<bool>{true}, "the panels told once");
    check(QSettings().value(QStringLiteral("commandPanel/picksShown")).toBool(), "remembered in the settings");
    setInputPicksShown(true);
    check(told.size() == 1, "opened again: nothing told");
    setInputPicksShown(false);
    check(!inputPicksShown() && told == std::vector<bool>({true, false}), "closed and told");
    check(!QSettings().value(QStringLiteral("commandPanel/picksShown"), true).toBool(), "closed in the settings");
    QObject::disconnect(connection);
    QCoreApplication::setOrganizationName(organization);
    QCoreApplication::setApplicationName(application);
    QSettings::setDefaultFormat(format);
  }

  // The row under the pointer.
  {
    QListWidget list;
    list.addItems({QStringLiteral("1. Body1"), QStringLiteral("2. Body2"), QStringLiteral("3. Body3")});
    list.resize(200, 200);
    list.show();
    QCoreApplication::processEvents();
    std::vector<int> hovered;
    auto* watch = new PickHoverWatch(&list, [&hovered](int row) { hovered.push_back(row); });
    check(watch->row() == -1, "no row at first");
    const QPoint second = list.visualItemRect(list.item(1)).center();
    moveOver(list, second);
    check(hovered == std::vector<int>{1} && watch->row() == 1, "the pointer over the second row");
    moveOver(list, second + QPoint(1, 0));
    check(hovered.size() == 1, "a move within the row tells nothing");
    moveOver(list, list.visualItemRect(list.item(0)).center());
    check(hovered == std::vector<int>({1, 0}), "then over the first");
    moveOver(list, QPoint(list.visualItemRect(list.item(2)).center().x(), list.viewport()->height() - 2));
    check(hovered == std::vector<int>({1, 0, -1}), "below the rows: none");
    moveOver(list, second);
    QEvent leave(QEvent::Leave);
    QCoreApplication::sendEvent(list.viewport(), &leave);
    check(hovered == std::vector<int>({1, 0, -1, 1, -1}), "leaving the list: none");
    moveOver(list, second);
    watch->reset();
    check(hovered == std::vector<int>({1, 0, -1, 1, -1, 1, -1}) && watch->row() == -1, "reset: none");
    watch->reset();
    check(hovered.size() == 7, "reset again: nothing told");
    moveOver(list, second);
    list.hide();
    check(hovered.back() == -1 && watch->row() == -1, "the list hidden: none");
  }
  return failures;
}
MITCAD_UNIT_TEST(inputPicksTests);
