// SPDX-License-Identifier: MIT
#include "HybridAssembly.hpp"

#include <exception>

#include <QCheckBox>
#include <QHash>
#include <QMessageBox>
#include <QObject>
#include <QPushButton>
#include <QStringList>
#include <QWidget>
#include <QtLogging>

#include "Dialogs.hpp"

namespace mitcad {
namespace {

QString title() { return QObject::tr("Hybrid Assembly"); }

QStringList names(const QJsonArray& items) {
  QStringList out;
  for (const QJsonValue& item : items) {
    out << item.toObject().value(QStringLiteral("name")).toString();
  }
  return out;
}

QStringList uids(const QJsonArray& items) {
  QStringList out;
  for (const QJsonValue& item : items) {
    out << item.toObject().value(QStringLiteral("uid")).toString();
  }
  return out;
}

// Feature names by uid, from a plan's groups.
QHash<QString, QString> featureNames(const QJsonObject& plan) {
  QHash<QString, QString> out;
  for (const QString& key : {QStringLiteral("groups"), QStringLiteral("together")}) {
    for (const QJsonValue& group : plan.value(key).toArray()) {
      for (const QJsonValue& feature : group.toObject().value(QStringLiteral("features")).toArray()) {
        const QJsonObject f = feature.toObject();
        out.insert(f.value(QStringLiteral("uid")).toString(), f.value(QStringLiteral("name")).toString());
      }
    }
  }
  return out;
}

// Hybrid Assembly deliberately creates one new component per planned group.
// Explicit context-menu moves choose their destination in MoveToComponentDialog.
bool moveToNewComponents(QWidget* parent, const HybridModel& model, const QJsonObject& selection) {
  const QString what = QObject::tr("Move to Component");
  QJsonObject query = selection;
  query.insert(QStringLiteral("query"), QStringLiteral("move_to_component"));
  QJsonObject plan;
  try {
    plan = model.query(query);
  } catch (const std::exception& e) {
    const QString error = QString::fromUtf8(e.what());
    qDebug().noquote() << QStringLiteral("Move to Component refused: %1").arg(error);
    sheetWarning(parent, what, error);
    return false;
  }
  const QJsonArray problems = plan.value(QStringLiteral("problems")).toArray();
  if (!problems.isEmpty()) {
    QStringList lines;
    for (const QJsonValue& problem : problems) {
      lines << problem.toString();
    }
    qDebug().noquote() << QStringLiteral("Move to Component refused: %1").arg(lines.join(QStringLiteral("; ")));
    sheetWarning(parent, what, lines.join(QLatin1Char('\n')));
    return false;
  }
  bool together = false;
  const QJsonArray conflicts = plan.value(QStringLiteral("conflicts")).toArray();
  if (!conflicts.isEmpty()) {
    // The user chooses: those bodies go together, or nothing goes.
    QStringList lines;
    for (const QJsonValue& conflict : conflicts) {
      lines << conflict.toObject().value(QStringLiteral("message")).toString();
    }
    QStringList groups;
    for (const QJsonValue& group : plan.value(QStringLiteral("together")).toArray()) {
      groups << names(group.toObject().value(QStringLiteral("bodies")).toArray()).join(QStringLiteral(", "));
    }
    qDebug().noquote() << QStringLiteral("Move to Component: %1").arg(lines.join(QStringLiteral("; ")));
    QMessageBox box(QMessageBox::Warning, what,
                    QObject::tr("These features cannot go into one component alone:"), QMessageBox::Cancel,
                    parent);
    box.setInformativeText(
        QObject::tr("%1\n\nKept together, the components get: %2")
            .arg(lines.join(QLatin1Char('\n')), groups.join(QStringLiteral("; "))));
    QPushButton* keep = box.addButton(QObject::tr("&Keep Them Together"), QMessageBox::AcceptRole);
    box.setDefaultButton(keep);
    prepareModal(&box);
    execDialog(&box);
    if (box.clickedButton() != keep) {
      qDebug().noquote() << QStringLiteral("Move to Component cancelled");
      return false;
    }
    together = true;
  }
  QJsonObject command = selection;
  command.insert(QStringLiteral("cmd"), QStringLiteral("move_to_component"));
  if (together) {
    command.insert(QStringLiteral("together"), true);
  }
  QJsonObject result;
  if (!model.run(command, &result)) {
    qDebug().noquote() << QStringLiteral("Move to Component not done");
    return false;
  }
  const QHash<QString, QString> known = featureNames(plan);
  for (const QJsonValue& value : result.value(QStringLiteral("components")).toArray()) {
    const QJsonObject made = value.toObject();
    QStringList features;
    for (const QJsonValue& f : made.value(QStringLiteral("features")).toArray()) {
      features << known.value(f.toString(), f.toString());
    }
    qDebug().noquote() << QStringLiteral("Moved %1 into %2")
                              .arg(features.join(QStringLiteral(", ")),
                                   made.value(QStringLiteral("name")).toString());
  }
  return true;
}

} // namespace

ComponentAnswer askHybridAssembly(QWidget* parent, const HybridModel& model, const ComponentRequest& request) {
  ComponentAnswer answer;
  if (request.kind != ComponentRequest::Kind::FromBodies && request.parent != QStringLiteral("C0")) {
    return answer;
  }
  QJsonObject plan;
  try {
    // Allowed for the design: no plan needed.
    if (model.query({{QStringLiteral("query"), QStringLiteral("document")}})
            .value(QStringLiteral("hybrid_assembly"))
            .toBool()) {
      return answer;
    }
    plan = model.query({{QStringLiteral("query"), QStringLiteral("move_to_component")}});
  } catch (const std::exception&) {
    return answer; // nothing of the root's to move
  }
  const QJsonArray rootBodies = plan.value(QStringLiteral("bodies")).toArray();
  if (rootBodies.isEmpty()) {
    return answer;
  }
  const QStringList rootUids = uids(rootBodies);
  QJsonArray selected; // FromBodies: the root's bodies among them
  QJsonArray others;
  if (request.kind == ComponentRequest::Kind::FromBodies) {
    for (const QJsonValue& body : request.bodies) {
      (rootUids.contains(body.toString()) ? selected : others).append(body);
    }
    if (selected.isEmpty()) {
      return answer;
    }
  }
  const QString root = plan.value(QStringLiteral("name")).toString();
  const QString bodies = names(rootBodies).join(QStringLiteral(", "));
  const bool fromBodies = request.kind == ComponentRequest::Kind::FromBodies;
  qDebug().noquote() << QStringLiteral("Hybrid assembly question: %1 has %2").arg(root, bodies);
  QMessageBox box(QMessageBox::Question, title(),
                  fromBodies ? QObject::tr("%1 has modeled bodies (%2). Move the features that made the "
                                           "selected bodies into their new components?")
                                   .arg(root, bodies)
                             : QObject::tr("%1 has modeled bodies (%2). Move their features into a "
                                           "component before continuing?")
                                   .arg(root, bodies),
                  QMessageBox::Cancel, parent);
  box.setInformativeText(
      fromBodies
          ? QObject::tr("Moved, each component has the history of its body and can be edited and "
                        "reused on its own. As is, the components hold the bodies only, and the "
                        "features that made them stay in %1.")
                .arg(root)
          : QObject::tr("A component made where modeled bodies are makes a hybrid assembly. Moved, "
                        "every modeled body of %1, with the features, sketches and parameters that "
                        "made them, goes into one new component where it is now, and %1 holds "
                        "only components.")
                .arg(root));
  QPushButton* move = box.addButton(fromBodies ? QObject::tr("&Move with Their Features")
                                               : QObject::tr("&Move into a New Component"),
                                    QMessageBox::AcceptRole);
  QPushButton* keep = box.addButton(QObject::tr("C&ontinue as Is"), QMessageBox::ActionRole);
  auto* again = new QCheckBox(QObject::tr("&Don't ask again for this design"), &box);
  box.setCheckBox(again);
  box.setDefaultButton(move);
  prepareModal(&box);
  execDialog(&box);
  if (box.clickedButton() != move && box.clickedButton() != keep) {
    qDebug().noquote() << QStringLiteral("Hybrid assembly: cancelled");
    answer.outcome = ComponentAnswer::Outcome::Cancel;
    return answer;
  }
  const bool moving = box.clickedButton() == move;
  qDebug().noquote() << QStringLiteral("Hybrid assembly: %1%2")
                            .arg(moving ? QStringLiteral("move") : QStringLiteral("continue as is"),
                                 again->isChecked() ? QStringLiteral(", don't ask again") : QString());
  if (again->isChecked() && model.run({{QStringLiteral("cmd"), QStringLiteral("set_hybrid_assembly")}}, nullptr)) {
    qDebug().noquote() << QStringLiteral("Hybrid assembly allowed in this design");
  }
  if (!moving) {
    return answer;
  }
  if (fromBodies) {
    QJsonArray groups;
    for (const QJsonValue& body : selected) {
      groups.append(QJsonArray{body});
    }
    if (!moveToNewComponents(parent, model, {{QStringLiteral("groups"), groups}})) {
      answer.outcome = ComponentAnswer::Outcome::Cancel;
      return answer;
    }
    // Bodies of other components become components as asked.
    if (!others.isEmpty()) {
      model.run({{QStringLiteral("cmd"), QStringLiteral("components_from_bodies")},
                 {QStringLiteral("bodies"), others}},
                nullptr);
    }
    answer.outcome = ComponentAnswer::Outcome::Done;
    return answer;
  }
  QJsonObject result;
  if (!model.run({{QStringLiteral("cmd"), QStringLiteral("move_to_component")}}, &result)) {
    qDebug().noquote() << QStringLiteral("Move to Component not done");
    answer.outcome = ComponentAnswer::Outcome::Cancel;
    return answer;
  }
  const QJsonObject made = result.value(QStringLiteral("components")).toArray().first().toObject();
  answer.moved = made.value(QStringLiteral("component")).toString();
  const QHash<QString, QString> known = featureNames(plan);
  QStringList features;
  for (const QJsonValue& f : made.value(QStringLiteral("features")).toArray()) {
    features << known.value(f.toString(), f.toString());
  }
  qDebug().noquote() << QStringLiteral("Moved %1 into %2")
                            .arg(features.join(QStringLiteral(", ")), made.value(QStringLiteral("name")).toString());
  return answer;
}

} // namespace mitcad
