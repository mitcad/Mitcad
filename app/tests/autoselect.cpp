// SPDX-License-Identifier: MIT
#include "UnitTests.hpp"
#include "framework/AutoselectPanel.hpp"

#include <QCoreApplication>
#include <QEnterEvent>
#include <QPushButton>

#include <cstdio>

int autoselectTests() {
  using namespace mitcad;
  int failures = 0;
  const auto check = [&failures](bool condition, const char* message) {
    if (!condition) {
      std::fprintf(stderr, "autoselect: %s\n", message);
      ++failures;
    }
  };
  // The surrounding projected rectangle has drawn holes: the model puts
  // it in Hybrid and the three circle interiors in Drawings.
  const QJsonObject answer{{"eligible", true}, {"sketch", "F4"}, {"component", "C1"},
                           {"projected", QJsonArray()}, {"drawn", QJsonArray{"circle1", "circle2", "circle3"}},
                           {"hybrid", QJsonArray{"surround"}},
                           {"all", QJsonArray{"circle1", "circle2", "circle3", "surround"}}};
  const auto profiles = AutoselectProfiles::read(answer, QStringLiteral("O1/O2"));
  check(profiles.available() && profiles.groups[0].isEmpty() && profiles.groups[1].size() == 3 &&
            profiles.groups[2].size() == 1 && profiles.groups[3].size() == 4,
        "model groups preserve their non-overlapping region identities");
  const SelectionItem first = profiles.groups[1][0];
  const SelectionItem second = profiles.groups[1][1];
  const SelectionItem third = profiles.groups[1][2];
  const SelectionItem surround = profiles.groups[2][0];
  check(first.owner == QStringLiteral("F4") && first.kind == SelectKind::Profile &&
            first.occurrence == QStringLiteral("O1/O2") && first.geometry == QStringLiteral("plane"),
        "candidate identity carries the same occurrence as normal profile picking");
  SelectionItem otherOccurrence = first;
  otherOccurrence.occurrence = QStringLiteral("O3/O2");
  const SelectionItem face{SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("top"), QStringLiteral("plane")};
  const Selection unrelated{face, otherOccurrence};
  Selection selected = toggleSelectionGroup(unrelated, profiles.groups[1]);
  check(selected.size() == 5 && selected.contains(face) && selected.contains(otherOccurrence) &&
            selected.contains(first) && selected.contains(second) && selected.contains(third),
        "first click adds all three interiors and preserves unrelated selections and occurrences");
  check(toggleSelectionGroup(selected, profiles.groups[1]) == unrelated, "second click restores the starting selection");
  selected = toggleSelectionGroup({face, first}, profiles.groups[1]);
  check(selected == Selection({face, second, third}), "partial selection toggles every candidate independently");
  selected = toggleSelectionGroup(selected, profiles.groups[3]);
  check(selected == Selection({face, first, surround}), "All profiles operates on the actual current selection");
  check(toggleSelectionGroup({}, {first, first, second}) == Selection({first, second}), "repeated candidates toggle only once");

  // The exact keys from profiles are authoritative after an edit, undo or
  // marker change; a historical alias must not select a new merged region.
  SelectionItem old = first;
  old.name = QStringLiteral("circle-before-split");
  SelectionItem otherSketch = first;
  otherSketch.owner = QStringLiteral("F3");
  const QJsonArray current{QJsonObject{{"sketch", "F4"}, {"region", "circle1"}},
                           QJsonObject{{"sketch", "F4"}, {"region", "circle2"}}};
  check(retainCurrentProfiles({face, first, old, second, third, otherSketch}, current) == Selection({face, first, second}),
        "refresh removes stale, removed and rolled-back profiles without touching unrelated items");
  check(retainCurrentProfiles({face, first}, {}) == Selection({face}), "rolling back the sketch removes its selection");

  AutoselectPanel panel;
  panel.setProfiles(profiles);
  check(panel.isHidden(), "eligible profiles stay hidden outside Solid idle mode");
  panel.setActive(true);
  auto* drawings = panel.findChild<QPushButton*>(QStringLiteral("autoselect_drawn"));
  auto* hybrid = panel.findChild<QPushButton*>(QStringLiteral("autoselect_hybrid"));
  auto* projected = panel.findChild<QPushButton*>(QStringLiteral("autoselect_projected"));
  auto* all = panel.findChild<QPushButton*>(QStringLiteral("autoselect_all"));
  auto* clear = panel.findChild<QPushButton*>(QStringLiteral("autoselect_clear"));
  check(!panel.isHidden() && projected->isHidden() && !drawings->isHidden() && !hybrid->isHidden(),
        "only nonempty origin groups are shown");
  check(drawings->text() == QStringLiteral("Drawings (3)") && hybrid->text() == QStringLiteral("Hybrid (1)") &&
            all->text() == QStringLiteral("All profiles (4)"), "candidate counts describe the current groups");
  selected = {face, first};
  Selection preview;
  int selectionChanges = 0;
  QObject::connect(&panel, &AutoselectPanel::toggleRequested, &panel, [&](const Selection& candidates) {
    selected = toggleSelectionGroup(selected, candidates);
    ++selectionChanges;
  });
  QObject::connect(&panel, &AutoselectPanel::clearRequested, &panel, [&] {
    selected.clear();
    ++selectionChanges;
  });
  QObject::connect(&panel, &AutoselectPanel::previewChanged, &panel, [&](const Selection& candidates) { preview = candidates; });
  const auto enter = [](QWidget* button) {
    QEnterEvent event{QPointF(), QPointF(), QPointF()};
    QCoreApplication::sendEvent(button, &event);
  };
  const auto leave = [](QWidget* button) {
    QEvent event(QEvent::Leave);
    QCoreApplication::sendEvent(button, &event);
  };
  enter(drawings);
  check(preview == profiles.groups[1] && selected == Selection({face, first}) && selectionChanges == 0,
        "hover previews three candidates without changing the actual selection");
  drawings->click();
  check(selected == Selection({face, second, third}) && preview == profiles.groups[1], "click changes selection while hover remains independent");
  leave(drawings);
  check(preview.isEmpty() && selected == Selection({face, second, third}), "leaving clears only the preview");
  selected.removeAll(second); // a normal viewport click between group clicks
  all->click();
  check(selected == Selection({face, first, second, surround}), "manual changes affect the very next group operation");
  clear->click();
  check(selected.isEmpty(), "Clear selection clears all actual items, including the unrelated face");

  enter(hybrid);
  QJsonObject replacement = answer;
  replacement.insert(QStringLiteral("sketch"), QStringLiteral("F7"));
  replacement.insert(QStringLiteral("drawn"), QJsonArray{"new-circle"});
  replacement.insert(QStringLiteral("hybrid"), QJsonArray());
  replacement.insert(QStringLiteral("all"), QJsonArray{"new-circle"});
  panel.setProfiles(AutoselectProfiles::read(replacement, QString()));
  check(preview.isEmpty() && hybrid->isHidden() && drawings->text() == QStringLiteral("Drawings (1)"),
        "a new sketch clears hover and updates buttons without retaining old IDs");
  drawings->click();
  check(selected.size() == 1 && selected[0].owner == QStringLiteral("F7") && selected[0].name == QStringLiteral("new-circle") &&
            selected[0].occurrence.isEmpty(), "new groups select the replacement sketch in the root occurrence");
  enter(drawings);
  panel.setActive(false);
  check(panel.isHidden() && preview.isEmpty(), "starting a command or sketch clears preview and hides Autoselect");
  const Selection saved = selected;
  drawings->click();
  check(selected == saved, "inactive presenters cannot change selection");
  panel.setActive(true);
  enter(drawings);
  panel.setProfiles(AutoselectProfiles::read({{"eligible", false}}, QString()));
  check(panel.isHidden() && preview.isEmpty(), "consuming or suppressing the sketch clears preview and hides the box");
  return failures;
}
MITCAD_UNIT_TEST(autoselectTests);
