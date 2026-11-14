// SPDX-License-Identifier: MIT
// CommandSession: parameter names picked from the view into a value field,
// a value field's full tooltip (mitcad#250), and the picks a failed
// preview's error names that are gone from the model (mitcad#1780). The
// rest of the session is in CommandSession.cpp.
#include <algorithm>
#include <exception>
#include "ModelMessages.hpp"
#include <utility>
#include <vector>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QRegularExpression>
#include <QTimer>
#include <QtLogging>

#include "CommandSession.hpp"
#include "ParameterPicker.hpp"

namespace mitcad {
namespace {

// How long the labels stay after their field lost the keyboard: a click in
// the view takes it before the view sees the click.
constexpr int kPickGraceMs = 400;

// A number with or without its unit ("20", "-4 mm", "30 deg").
bool plainNumber(const QString& text) {
  static const QRegularExpression number(
      QStringLiteral(R"(^\s*[-+]?(\d+[.,]?\d*|[.,]\d+)([eE][-+]?\d+)?\s*(mm|cm|m|um|in|ft|deg|rad|°)?\s*$)"));
  return number.match(text).hasMatch();
}

} // namespace

void CommandSession::valueFocused(const QString& key, bool focused) {
  if (m_done) {
    return;
  }
  const auto slot = slotOf(key);
  if (!slot || slot->def->type != InputDef::Type::Value) {
    return;
  }
  m_valueTyped = false; // nothing typed in the field with the keyboard now (mitcad#1421)
  if (focused) {
    m_valueKeyboard = true;
    const bool onOpen = std::exchange(m_openFocusPending, false);
    if (!m_autoFocusing && !onOpen) {
      m_valueTouched = true;
    }
    const bool same = key == m_pickTarget && !m_picker.isNull();
    m_pickTarget = key;
    m_pickFocused = true;
    // The labels shown for this field stay as they are (it takes the
    // keyboard back after a name went in, or is clicked again). None while
    // the keyboard went here by itself as the panel opened (mitcad#587), nor
    // after a pick when the selection takes more items: the next click in
    // the view is its next pick, not a label's name (mitcad#467). A click in
    // the field (CommandPanel) or typing in it shows them.
    if (!same && !onOpen && !(m_autoFocusing && activeTakesMore())) {
      showParameterLabels();
    }
    return;
  }
  m_valueKeyboard = false;
  if (key != m_pickTarget) {
    return;
  }
  m_pickFocused = false;
  QTimer::singleShot(kPickGraceMs, this, [this] {
    if (!m_pickFocused) {
      hideParameterLabels();
    }
  });
}

bool CommandSession::activeTakesMore() const {
  const InputDef* input = activeInput();
  if (input == nullptr || input->type != InputDef::Type::Selection || input->max == 1) {
    return false;
  }
  return input->max == 0 || m_state.items(m_active).size() < input->max;
}

void CommandSession::showParameterLabels() {
  if (m_done || m_pickTarget.isEmpty()) {
    return;
  }
  if (m_host.modelBusy()) {
    m_host.whenIdle(this, [this] {
      if (!m_done && m_pickFocused) {
        showParameterLabels();
      }
    });
    return;
  }
  std::vector<ParameterPicker::Sketch> sketches;
  QHash<QString, QString> texts;
  gatherParameterLabels(m_host, sketches, texts);
  if (!m_picker) {
    auto* picker = new ParameterPicker(m_host.viewer(), m_def.name);
    m_picker = picker;
    connect(picker, &ParameterPicker::picked, this, [this](const QString& name) { insertParameter(name); });
    // Gone with the command, also when it ends while the field has the
    // keyboard.
    connect(this, &CommandSession::finished, picker, &QObject::deleteLater);
    connect(this, &QObject::destroyed, picker, &QObject::deleteLater);
  }
  m_picker->setContent(std::move(sketches), std::move(texts));
}

void CommandSession::hideParameterLabels() {
  m_pickTarget.clear();
  m_pickFocused = false;
  if (m_picker) {
    m_picker->hide();
    m_picker->deleteLater();
    m_picker = nullptr;
    qDebug().noquote() << QStringLiteral("Parameter labels for %1 hidden").arg(m_def.name);
  }
}

void CommandSession::insertParameter(const QString& name) {
  if (m_done || m_pickTarget.isEmpty() || m_presenter == nullptr) {
    return;
  }
  m_inserting = true;
  const bool inserted = m_presenter->insertIntoValue(m_pickTarget, name);
  m_inserting = false;
  if (!inserted) {
    qDebug().noquote() << QStringLiteral("%1: %2 not inserted, %3 is not shown").arg(m_def.name, name, m_pickTarget);
  }
}

void CommandSession::describeParameter(const QString& key, const QString& rename, const QString& text,
                                       const QString& shown) {
  const QString own = m_state.parameterName(key);
  if (own.isEmpty()) {
    return;
  }
  if (shown.isEmpty()) {
    m_state.setParameterTooltip(key, QString());
    return;
  }
  if (!m_parameterComments.contains(own)) {
    try {
      m_parametersWithLimits.clear();
      for (const QJsonValue& entry : m_host.queryArray(QStringLiteral("parameters"))) {
        const QJsonObject p = entry.toObject();
        m_parameterComments.insert(p.value(QStringLiteral("name")).toString(),
                                   p.value(QStringLiteral("comment")).toString());
        if (p.contains(QStringLiteral("limits"))) {
          m_parametersWithLimits.insert(p.value(QStringLiteral("name")).toString());
        }
      }
    } catch (const std::exception&) {
      // Without its comment then.
    }
    if (!m_parameterComments.contains(own)) {
      m_parameterComments.insert(own, QString());
    }
  }
  // Under the name it is given ("depth = 12" renames it on OK).
  const QString name = rename.isEmpty() ? own : rename;
  QString about = plainNumber(text) ? QStringLiteral("%1 = %2").arg(name, shown)
                                    : QStringLiteral("%1 = %2 → %3").arg(name, text, shown);
  if (const QString comment = m_parameterComments.value(own); !comment.isEmpty()) {
    about += QStringLiteral(" (%1)").arg(comment);
  }
  m_state.setParameterTooltip(key, about);
}

QString CommandSession::limitHint(const QString& key, const QString& text) {
  const QString own = m_state.parameterName(key);
  if (own.isEmpty() || text.isEmpty() || !m_parametersWithLimits.contains(own)) {
    return {};
  }
  QString hint;
  QString about;
  try {
    const QJsonObject answer = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("evaluate")},
                                                   {QStringLiteral("expression"), text},
                                                   {QStringLiteral("parameter"), own}});
    const QString limited = answer.value(QStringLiteral("limited")).toString();
    if (!limited.isEmpty()) {
      const QString mark = limited == QStringLiteral("max")   ? QStringLiteral("▲max")
                           : limited == QStringLiteral("min") ? QStringLiteral("▼min")
                                                              : QStringLiteral("◆step");
      const QString to = answer.value(QStringLiteral("limited_text")).toString();
      hint = QStringLiteral("→ %1 %2").arg(to, mark);
      about = tr("%1 will be limited to %2 (%3)").arg(answer.value(QStringLiteral("text")).toString(), to, mark);
    }
  } catch (const std::exception& e) {
    // A strict parameter refuses it: said where the value is, as other
    // problems are told when the panel's OK is pressed.
    about = tr("Refused by the limits of %1: %2").arg(own, mitcad::modelMessage(e.what()));
    hint = tr("refused by its limits");
  }
  if (!about.isEmpty()) {
    const QString tooltip = m_state.parameterTooltip(key);
    m_state.setParameterTooltip(key, tooltip.isEmpty() ? about : QString(tooltip + QLatin1Char('\n') + about));
    qDebug().noquote() << QStringLiteral("%1 %2: %3 %4").arg(m_def.name, own, text, about);
  }
  return hint;
}

// ---------------------------------------------------------------------------
// Picks that are gone (mitcad#1780)

namespace {

// Whether a place of a notice (core/model-api/src/commands.md, "Notices")
// is the pick: a face, edge or vertex by its name on its body, a profile by
// its region in its sketch (a place that names no body or sketch: by the
// name alone).
bool namesPick(const QJsonObject& place, const SelectionItem& item) {
  const QString kind = place.value(QStringLiteral("kind")).toString();
  const QString name = place.value(QStringLiteral("name")).toString();
  if (name.isEmpty() || name != item.name) {
    return false;
  }
  const auto owner = [&place, &item](const char* field) {
    const QString value = place.value(QLatin1String(field)).toString();
    return value.isEmpty() || value == item.owner;
  };
  switch (item.kind) {
  case SelectKind::Face:
    return kind == QStringLiteral("face") && owner("body");
  case SelectKind::Edge:
    return kind == QStringLiteral("edge") && owner("body");
  case SelectKind::Vertex:
    return kind == QStringLiteral("vertex") && owner("body");
  case SelectKind::Profile:
    return kind == QStringLiteral("region") && owner("sketch");
  default:
    return false;
  }
}

// What the message says of an input's picks of one kind that are gone, the
// input by its label: "A face of Faces is gone: pick it again".
QString goneText(SelectKind kind, int count, const QString& input) {
  const bool one = count == 1;
  switch (kind) {
  case SelectKind::Face:
    return one ? QObject::tr("A face of %1 is gone: pick it again").arg(input)
               : QObject::tr("Some faces of %1 are gone: pick them again").arg(input);
  case SelectKind::Edge:
    return one ? QObject::tr("An edge of %1 is gone: pick it again").arg(input)
               : QObject::tr("Some edges of %1 are gone: pick them again").arg(input);
  case SelectKind::Vertex:
    return one ? QObject::tr("A vertex of %1 is gone: pick it again").arg(input)
               : QObject::tr("Some vertices of %1 are gone: pick them again").arg(input);
  case SelectKind::Profile:
    return one ? QObject::tr("A profile of %1 is gone: pick it again").arg(input)
               : QObject::tr("Some profiles of %1 are gone: pick them again").arg(input);
  default:
    return one ? QObject::tr("A pick of %1 is gone: pick it again").arg(input)
               : QObject::tr("Some picks of %1 are gone: pick them again").arg(input);
  }
}

} // namespace

QString CommandSession::markMissingPicks(const QJsonObject& report) {
  // The places the error notices name: the references the model did not
  // find (a gone face is painted nowhere, but its place is still named).
  std::vector<QJsonObject> places;
  for (const QJsonValue& value : report.value(QStringLiteral("notices")).toArray()) {
    const QJsonObject notice = value.toObject();
    if (notice.value(QStringLiteral("level")).toString() == QStringLiteral("error")) {
      for (const QJsonValue& place : notice.value(QStringLiteral("places")).toArray()) {
        places.push_back(place.toObject());
      }
    }
  }
  QStringList lines;
  bool changed = false;
  for (const Slot& slot : slotsOf(false)) {
    if (slot.def->type != InputDef::Type::Selection) {
      continue;
    }
    const Selection& items = std::as_const(m_state).items(slot.key);
    QList<int> missing;
    for (int i = 0; i < items.size(); ++i) {
      const SelectionItem& item = items[i];
      const bool named = std::any_of(places.cbegin(), places.cend(), [&item](const QJsonObject& place) {
        return namesPick(place, item);
      });
      // Named, and not in the model as the feature finds it (a face the
      // error is about but which is there, too small or not round, is no
      // missing pick).
      if (named && m_host.itemShape(item).IsNull()) {
        missing << i;
      }
    }
    if (missing != m_state.missingItems(slot.key)) {
      m_state.setMissingItems(slot.key, missing);
      changed = true;
    }
    if (missing.isEmpty()) {
      continue;
    }
    // By kind, in the order of the picks.
    QList<SelectKind> kinds;
    QHash<unsigned, int> counts;
    QStringList logged;
    for (const int index : std::as_const(missing)) {
      const SelectKind kind = items[index].kind;
      if (!kinds.contains(kind)) {
        kinds << kind;
      }
      ++counts[static_cast<unsigned>(kind)];
      logged << items[index].logDescription();
    }
    const QString label = slot.def->labelIn(scopeOf(slot));
    for (const SelectKind kind : std::as_const(kinds)) {
      lines << goneText(kind, counts.value(static_cast<unsigned>(kind)), label);
    }
    // The panel's list names each pick in words (mitcad#1741), the missing
    // ones marked.
    if (std::as_const(m_state).itemWords(slot.key).size() != items.size()) {
      QStringList words;
      for (const SelectionItem& item : items) {
        words << m_host.itemText(item);
      }
      m_state.setItemWords(slot.key, words);
    }
    qDebug().noquote() << QStringLiteral("%1 %2: %3 of %4 pick(s) gone: %5")
                              .arg(m_def.name, slot.def->label)
                              .arg(missing.size())
                              .arg(items.size())
                              .arg(logged.join(QStringLiteral("; ")));
  }
  if (changed && m_presenter) {
    m_presenter->refresh(m_state);
  }
  return lines.join(QLatin1Char('\n'));
}

bool CommandSession::clearMissingPicks(const QString& key) {
  bool cleared = false;
  for (const Slot& slot : slotsOf(false)) {
    if ((key.isEmpty() || slot.key == key) && !m_state.missingItems(slot.key).isEmpty()) {
      m_state.setMissingItems(slot.key, {});
      cleared = true;
    }
  }
  return cleared;
}

} // namespace mitcad
