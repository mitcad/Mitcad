// SPDX-License-Identifier: MIT
#include "ConvertToFeatures.hpp"

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QMessageBox>
#include <QPushButton>
#include <QStringList>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "DocumentHost.hpp"
#include "DocumentSnapshot.hpp"

namespace mitcad {
namespace {

struct Text {
  Q_DECLARE_TR_FUNCTIONS(ConvertToFeatures)
};

QJsonObject convertCommand(const QString& component, bool check) {
  return {{QStringLiteral("cmd"), QStringLiteral("convert_derived_part")},
          {QStringLiteral("component"), component},
          {QStringLiteral("check"), check}};
}

} // namespace

QString convertToFeaturesText() { return Text::tr("Convert to Features"); }

bool isDerivedPartComponent(const DocumentSnapshot& snapshot, const QString& component) {
  for (const DocumentSnapshot::Feature& feature : snapshot.features) {
    if (feature.component == component && feature.type == QStringLiteral("derived_part")) {
      return true;
    }
  }
  return false;
}

void convertToFeatures(DocumentHost& host, QWidget* parent, const QString& component, const QString& name) {
  if (!host.canChangeModel()) {
    const QString readOnly = host.readOnlyReason();
    host.showStatus(readOnly.isEmpty() ? host.changeBlockedReason() : readOnly, NoticeLevel::Error);
    return;
  }
  // The model's check first, which changes nothing: a part it refuses (a
  // reference no feature of it would make) is not asked about; the host
  // shows the reason as any refused command's.
  QJsonObject checked;
  if (!host.runModelCommand(convertCommand(component, true), &checked)) {
    qDebug().noquote() << QStringLiteral("Convert to Features of %1 refused").arg(name);
    return;
  }
  const bool history = checked.value(QStringLiteral("history")).toBool(true);
  qDebug().noquote() << QStringLiteral("Convert to Features %1? (%2)")
                            .arg(name, history ? QStringLiteral("features") : QStringLiteral("a plain body"));
  QMessageBox box(QMessageBox::Question, Text::tr("Convert to Features"), Text::tr("Convert %1 to features?").arg(name),
                  QMessageBox::Cancel, parent);
  const QString becomes =
      history ? Text::tr("Its section becomes a sketch and an extrusion that can be edited like any other. "
                         "Everything placed on it stays where it is.")
              : Text::tr("Its library design has no section to extrude: the part becomes a plain body of "
                         "its component. Everything placed on it stays where it is.");
  const QString lost =
      Text::tr("It will no longer follow its library: library updates and configuration rows will not change it, "
               "and the BUILD tools that need a member (spacing, fitting fasteners by section, Timber Frame "
               "membership) will no longer treat it as one. The cut list and the bill of materials keep its "
               "material, grade, section and length.");
  box.setInformativeText(becomes + QStringLiteral("\n\n") + lost);
  QPushButton* convert = box.addButton(Text::tr("Convert"), QMessageBox::AcceptRole);
  box.setDefaultButton(convert);
  prepareModal(&box);
  execDialog(&box);
  if (box.clickedButton() != convert) {
    qDebug().noquote() << QStringLiteral("Convert to Features of %1 cancelled").arg(name);
    return;
  }
  QJsonObject result;
  if (!host.runModelCommand(convertCommand(component, false), &result)) {
    return;
  }
  QStringList made;
  for (const QJsonValue& value : result.value(QStringLiteral("features")).toArray()) {
    made << value.toObject().value(QStringLiteral("name")).toString();
  }
  qDebug().noquote() << QStringLiteral("Converted %1 to features: %2").arg(name, made.join(QStringLiteral(", ")));
}

} // namespace mitcad
