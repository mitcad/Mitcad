// SPDX-License-Identifier: MIT
#include "framework/CutTargetsPanel.hpp"

#include <cstdio>
#include <QJsonArray>
#include <QLabel>
#include <QPushButton>
#include <QTreeWidget>

namespace {
QJsonObject identity(const QString& component, const QString& occurrence) {
  return {{QStringLiteral("component"), component}, {QStringLiteral("occurrence"), occurrence}};
}
QJsonObject candidate(const QString& component, const QString& occurrence, bool checked, bool intersects = true,
                       bool eligible = true) {
  QJsonObject result = identity(component, occurrence);
  result.insert(QStringLiteral("path_name"), QStringLiteral("Root/Part ") + component);
  result.insert(QStringLiteral("eligible"), eligible);
  result.insert(QStringLiteral("checked"), checked);
  result.insert(QStringLiteral("intersects"), intersects);
  result.insert(QStringLiteral("reason"), eligible ? QJsonValue() : QJsonValue(QStringLiteral("Repeated component definition")));
  result.insert(QStringLiteral("bodies"), QJsonArray{QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                                                               {QStringLiteral("checked"), checked}}});
  return result;
}

QJsonObject withBodies(QJsonObject row, const QStringList& bodies) {
  QJsonArray list;
  for (const QString& body : bodies) {
    list.append(QJsonObject{{QStringLiteral("body"), body}, {QStringLiteral("checked"), true}});
  }
  row.insert(QStringLiteral("bodies"), list);
  return row;
}

QJsonObject report(const QJsonArray& rows) {
  return {{QStringLiteral("cut_targets"), QJsonObject{{QStringLiteral("rows"), rows}}}};
}

QJsonArray targetList(const QJsonObject& scope, const char* key) {
  return scope.value(QStringLiteral("targets")).toObject().value(QLatin1String(key)).toArray();
}

// A new cut's hidden targets start unchecked (mitcad#475).
int hiddenTargetsTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) { std::fprintf(stderr, "hidden cut targets: %s\n", message); ++failures; }
  };
  // Root (C0) with Body F1.b0 shown and F3.b0 hidden; Shown:1 (O1, C1) with
  // Inner:1 (O3, C3) whose light bulb is off; Hidden:1 (O2, C2) off.
  const QJsonObject components{
      {QStringLiteral("occurrences"),
       QJsonArray{QJsonObject{{QStringLiteral("uid"), QStringLiteral("O1")}, {QStringLiteral("visible"), true},
                              {QStringLiteral("children"),
                               QJsonArray{QJsonObject{{QStringLiteral("uid"), QStringLiteral("O3")},
                                                      {QStringLiteral("visible"), false}}}}},
                  QJsonObject{{QStringLiteral("uid"), QStringLiteral("O2")}, {QStringLiteral("visible"), false}}}}};
  const QJsonArray bodies{QJsonObject{{QStringLiteral("uid"), QStringLiteral("F1.b0")}, {QStringLiteral("visible"), true}},
                          QJsonObject{{QStringLiteral("uid"), QStringLiteral("F3.b0")}, {QStringLiteral("visible"), false}}};
  QJsonObject view = mitcad::cutTargetVisibility(components, bodies, {});
  check(view.value(QStringLiteral("hidden_occurrences")).toArray() == QJsonArray{QStringLiteral("O1/O3"), QStringLiteral("O2")},
        "the occurrences whose light bulbs are off, by path");
  check(view.value(QStringLiteral("hidden_bodies")).toArray() == QJsonArray{QStringLiteral("F3.b0")}, "the hidden bodies");

  const QJsonObject context = identity(QStringLiteral("C0"), QString());
  QJsonObject scope = mitcad::selectCutTargets({{QStringLiteral("context"), context}}, true);
  const QJsonObject root = withBodies(candidate(QStringLiteral("C0"), QString(), true), {QStringLiteral("F1.b0"), QStringLiteral("F3.b0")});
  const QJsonObject shown = withBodies(candidate(QStringLiteral("C1"), QStringLiteral("O1"), true), {QStringLiteral("F5.b0")});
  const QJsonObject hidden = withBodies(candidate(QStringLiteral("C2"), QStringLiteral("O2"), true), {QStringLiteral("F7.b0")});
  QJsonObject answer = report({root, shown, hidden});
  check(mitcad::excludeHiddenCutTargets(scope, view, answer), "hidden targets change a new scope");
  check(targetList(scope, "excluded") == QJsonArray{identity(QStringLiteral("C2"), QStringLiteral("O2"))},
        "a hidden occurrence is excluded as a whole");
  QJsonObject hiddenBody = context;
  hiddenBody.insert(QStringLiteral("body"), QStringLiteral("F3.b0"));
  check(targetList(scope, "excluded_bodies") == QJsonArray{hiddenBody}, "a hidden body of a shown row only by itself");
  const QJsonArray rows = answer.value(QStringLiteral("cut_targets")).toObject().value(QStringLiteral("rows")).toArray();
  check(rows[2].toObject().value(QStringLiteral("hidden")).toBool() && !rows[1].toObject().contains(QStringLiteral("hidden")) &&
            !rows[0].toObject().contains(QStringLiteral("hidden")),
        "the report marks the hidden row");
  check(rows[0].toObject().value(QStringLiteral("bodies")).toArray()[1].toObject().value(QStringLiteral("hidden")).toBool(),
        "and the hidden body");

  // The user checks the hidden one again: later previews keep it.
  scope = mitcad::setCutTargetChecked(scope, identity(QStringLiteral("C2"), QStringLiteral("O2")), QString(), true);
  answer = report({root, shown, hidden});
  check(!mitcad::excludeHiddenCutTargets(scope, view, answer) && targetList(scope, "excluded").isEmpty(),
        "a row seen before keeps the user's choice");

  // A hidden target that joins later (a new intersection) starts unchecked.
  const QJsonObject inner = withBodies(candidate(QStringLiteral("C3"), QStringLiteral("O1/O3"), true), {QStringLiteral("F9.b0")});
  answer = report({root, shown, hidden, inner});
  check(mitcad::excludeHiddenCutTargets(scope, view, answer) &&
            targetList(scope, "excluded") == QJsonArray{identity(QStringLiteral("C3"), QStringLiteral("O1/O3"))},
        "an occurrence below a hidden one joining later is excluded");

  // An explicit scope (Select none) takes nothing new; Isolate hides what
  // it leaves out.
  QJsonObject isolatedView = mitcad::cutTargetVisibility(
      {}, {}, {{QStringLiteral("display"),
                QJsonObject{{QStringLiteral("isolated"), QJsonArray{QJsonObject{{QStringLiteral("occurrence"), QStringLiteral("O1")}}}}}}});
  QJsonObject isolatedScope = mitcad::selectCutTargets({{QStringLiteral("context"), context}}, true);
  answer = report({root, shown});
  check(mitcad::excludeHiddenCutTargets(isolatedScope, isolatedView, answer) &&
            targetList(isolatedScope, "excluded") == QJsonArray{context} && targetList(isolatedScope, "excluded_bodies").isEmpty(),
        "Isolate counts as hidden for what it hides");
  QJsonObject explicitView = mitcad::cutTargetVisibility(components, bodies, {});
  QJsonObject none = mitcad::selectCutTargets({{QStringLiteral("context"), context}}, false);
  const QJsonObject before = none;
  answer = report({hidden});
  check(!mitcad::excludeHiddenCutTargets(none, explicitView, answer) && none == before,
        "an explicit scope is left as it is");
  return failures;
}

// A target an automatic scope leaves out because it moves through joints
// can be checked: the scope turns explicit with it (mitcad#1789).
int movingTargetsTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) { std::fprintf(stderr, "moving cut targets: %s\n", message); ++failures; }
  };
  const QString moves = QStringLiteral("Moves against this component through joints, so it is left out; "
                                       "check it to cut it as well.");
  const QJsonObject context = identity(QStringLiteral("C1"), QStringLiteral("O1"));
  const QJsonObject turning = identity(QStringLiteral("C2"), QStringLiteral("O2"));
  const QJsonObject excludedByHand = identity(QStringLiteral("C3"), QStringLiteral("O3"));
  QJsonObject own = withBodies(candidate(QStringLiteral("C1"), QStringLiteral("O1"), true), {QStringLiteral("F1.b0"), QStringLiteral("F3.b0")});
  own.insert(QStringLiteral("partial"), true);
  QJsonObject moving = withBodies(candidate(QStringLiteral("C2"), QStringLiteral("O2"), false, true, false),
                                  {QStringLiteral("F5.b0"), QStringLiteral("F6.b0")});
  moving.insert(QStringLiteral("reason"), moves);
  moving.insert(QStringLiteral("joint_motion"), true);
  const QJsonObject unchecked = withBodies(candidate(QStringLiteral("C3"), QStringLiteral("O3"), false), {QStringLiteral("F7.b0")});
  const QJsonObject linked = candidate(QStringLiteral("C4"), QStringLiteral("O4"), false, true, false);
  const QJsonObject report{{QStringLiteral("rows"), QJsonArray{own, moving, unchecked, linked}},
                           {QStringLiteral("selected_components"), 1},
                           {QStringLiteral("selected_bodies"), 1}};
  QJsonObject exception = context;
  exception.insert(QStringLiteral("body"), QStringLiteral("F3.b0"));
  QJsonObject scope = mitcad::selectCutTargets({{QStringLiteral("context"), context}}, true);
  QJsonObject targets = scope.value(QStringLiteral("targets")).toObject();
  targets.insert(QStringLiteral("excluded"), QJsonArray{excludedByHand});
  targets.insert(QStringLiteral("excluded_bodies"), QJsonArray{exception});
  scope.insert(QStringLiteral("targets"), targets);

  // The pure step: what is checked now, its body exceptions, and the row.
  const QJsonObject whole = mitcad::includeMovingCutTarget(scope, report, turning, QString());
  check(whole.value(QStringLiteral("context")).toObject() == context, "the context stays");
  check(whole.value(QStringLiteral("targets")).toObject().value(QStringLiteral("type")) == QStringLiteral("explicit"),
        "the scope turns explicit");
  check(targetList(whole, "included") == QJsonArray{context, turning},
        "everything checked now plus the moving row, nothing unchecked or unavailable");
  check(targetList(whole, "excluded_bodies") == QJsonArray{exception}, "the checked rows' body exceptions stay");
  const QJsonObject one = mitcad::includeMovingCutTarget(scope, report, turning, QStringLiteral("F5.b0"));
  QJsonObject otherBody = turning;
  otherBody.insert(QStringLiteral("body"), QStringLiteral("F6.b0"));
  check(targetList(one, "included") == QJsonArray{context, turning} &&
            targetList(one, "excluded_bodies") == QJsonArray{exception, otherBody},
        "checking one body of it includes only that body");

  // The list: the moving row enabled and unchecked, its reason under its
  // name; the linked one disabled; no ids in the body tooltips.
  mitcad::CutTargetsPanel panel;
  auto* tree = panel.findChild<QTreeWidget*>(QStringLiteral("cut_targets_list"));
  QJsonObject emitted;
  QObject::connect(&panel, &mitcad::CutTargetsPanel::scopeChanged, &panel,
                   [&emitted](const QJsonObject& value) { emitted = value; });
  panel.setScope(scope);
  panel.setPreview(report);
  QTreeWidgetItem* row = tree->topLevelItem(1);
  check(row->flags().testFlag(Qt::ItemIsEnabled) && row->checkState(0) == Qt::Unchecked,
        "the moving row is enabled and unchecked");
  check(row->text(0) == QStringLiteral("Root/Part C2\n") + moves, "its reason in full under its name");
  check(row->child(0)->flags().testFlag(Qt::ItemIsEnabled), "its bodies can be checked too");
  check(!tree->topLevelItem(3)->flags().testFlag(Qt::ItemIsEnabled), "a row unavailable for another reason stays disabled");
  bool ids = false;
  for (int i = 0; i < tree->topLevelItemCount(); ++i) {
    for (int b = 0; b < tree->topLevelItem(i)->childCount(); ++b) {
      const QString tip = tree->topLevelItem(i)->child(b)->toolTip(0);
      ids = ids || tip.contains(QStringLiteral(".b0")) || !tip.contains(QStringLiteral("Root/Part "));
    }
  }
  check(!ids, "body tooltips name the body and its component, no ids");
  row->setCheckState(0, Qt::Checked);
  check(emitted == whole, "checking it emits the explicit scope in one step");
  return failures;
}
} // namespace

int cutTargetsTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) { std::fprintf(stderr, "cut targets: %s\n", message); ++failures; }
  };
  const QJsonObject context = identity(QStringLiteral("C0"), QString());
  const QJsonObject b = identity(QStringLiteral("C2"), QStringLiteral("O2"));
  QJsonObject scope = mitcad::selectCutTargets({{QStringLiteral("context"), context}}, true);
  mitcad::CutTargetsPanel panel;
  auto* tree = panel.findChild<QTreeWidget*>(QStringLiteral("cut_targets_list"));
  int mutations = 0;
  mitcad::Selection highlighted;
  QObject::connect(&panel, &mitcad::CutTargetsPanel::scopeChanged, &panel, [&](const QJsonObject& value) {
    ++mutations;
    scope = value;
    panel.setScope(scope);
    panel.setPreview({}); // Same immediate stale transition as CommandSession.
  });
  QObject::connect(&panel, &mitcad::CutTargetsPanel::highlightChanged, &panel,
                   [&](const mitcad::Selection& value) { highlighted = value; });
  const auto preview = [&](QJsonArray rows, int selected) {
    panel.setPreview({{QStringLiteral("rows"), rows}, {QStringLiteral("selected_components"), selected},
                      {QStringLiteral("selected_bodies"), selected}, {QStringLiteral("can_commit"), selected > 0}});
  };
  panel.setScope(scope);
  preview({candidate(QStringLiteral("C0"), QString(), true), candidate(QStringLiteral("C2"), QStringLiteral("O2"), true)}, 2);
  check(tree && tree->maximumHeight() <= 180, "checklist has a bounded height");
  tree->topLevelItem(1)->setCheckState(0, Qt::Unchecked);
  check(mutations == 1 && !tree->isEnabled(), "a checkbox submits one change and disables stale candidates");
  check(scope.value(QStringLiteral("context")).toObject() == context, "root occurrence survives explicitly empty");
  check(scope.value(QStringLiteral("targets")).toObject().value(QStringLiteral("excluded")).toArray() == QJsonArray{b},
        "unchecking stores the component occurrence, not all instances of its definition");
  const QJsonObject excluded = scope;
  preview({candidate(QStringLiteral("C0"), QString(), true), candidate(QStringLiteral("C2"), QStringLiteral("O2"), false, false)}, 1);
  check(scope == excluded && mutations == 1, "outside intersection metadata never rewrites persistent exclusions");
  preview({candidate(QStringLiteral("C0"), QString(), true), candidate(QStringLiteral("C2"), QStringLiteral("O2"), false)}, 1);
  check(tree->topLevelItem(1)->checkState(0) == Qt::Unchecked && scope == excluded, "excluded component stays unchecked on re-entry");
  tree->setCurrentItem(tree->topLevelItem(1));
  check(highlighted.size() == 1 && highlighted.first().owner == QStringLiteral("F2.b0") &&
        highlighted.first().occurrence == QStringLiteral("O2") && mutations == 1,
        "keyboard focus highlights the precise occurrence without changing scope");
  QMetaObject::invokeMethod(tree, "entered", Qt::DirectConnection, Q_ARG(QModelIndex, QModelIndex()));
  check(highlighted.isEmpty() && mutations == 1, "hover exit clears only highlights");
  panel.findChild<QPushButton*>(QStringLiteral("cut_targets_none"))->click();
  const QJsonObject none = scope.value(QStringLiteral("targets")).toObject();
  check(none.value(QStringLiteral("type")) == QStringLiteral("explicit") && none.value(QStringLiteral("included")).toArray().isEmpty(),
        "select none is explicit empty, never automatic fallback");
  preview({candidate(QStringLiteral("C2"), QStringLiteral("O2"), false)}, 0);
  check(panel.findChild<QPushButton*>(QStringLiteral("cut_targets_all"))->isEnabled(), "empty/error preview remains repairable");
  panel.findChild<QPushButton*>(QStringLiteral("cut_targets_all"))->click();
  check(scope.value(QStringLiteral("targets")).toObject().value(QStringLiteral("type")) == QStringLiteral("automatic"),
        "select all explicitly restores automatic discovery");
  preview({candidate(QStringLiteral("C2"), QStringLiteral("O2"), false, true, false)}, 0);
  check(!tree->topLevelItem(0)->flags().testFlag(Qt::ItemIsEnabled) &&
        tree->topLevelItem(0)->toolTip(0).contains(QStringLiteral("Repeated")), "unsupported rows are disabled with a reason");
  scope = mitcad::setCutTargetChecked(scope, b, QStringLiteral("F2.b0"), false);
  check(scope.value(QStringLiteral("targets")).toObject().value(QStringLiteral("excluded_bodies")).toArray().size() == 1,
        "body exclusion is retained separately");
  scope = mitcad::setCutTargetChecked(scope, b, QString(), true);
  check(scope.value(QStringLiteral("targets")).toObject().value(QStringLiteral("excluded_bodies")).toArray().isEmpty(),
        "explicitly enabling the whole component clears its finer exclusions");
  // A cut that misses but would cut flipped suggests Flip (mitcad#1676).
  auto* status = panel.findChild<QLabel*>(QStringLiteral("cut_targets_status"));
  panel.setPreview({{QStringLiteral("rows"), QJsonArray{}}, {QStringLiteral("selected_components"), 0},
                    {QStringLiteral("selected_bodies"), 0}, {QStringLiteral("can_commit"), false},
                    {QStringLiteral("flip_would_cut"), true}});
  check(status && status->text().contains(QStringLiteral("Try Flip")), "a miss that would cut flipped suggests Flip");
  preview({}, 0);
  check(status && status->text() == QStringLiteral("No eligible components intersect the cut."),
        "a miss both ways does not suggest Flip");
  failures += hiddenTargetsTests();
  failures += movingTargetsTests();
  return failures;
}
