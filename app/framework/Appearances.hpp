// SPDX-License-Identifier: MIT
#pragma once

// The appearances the Appearance command offers (U4): the model stores a
// body's appearance as an id (`set_body_appearance`); the view colours the
// body with it. Physical materials are the model's own list (`steel`, ...).

#include <QColor>
#include <QPair>
#include <QString>
#include <QVector>

namespace mitcad {

struct Appearance {
  QString id;
  QString name;
  QColor color;
};

// Mitcad's appearance library, in the order the command lists it.
const QVector<Appearance>& appearances();
// The colour of an appearance id; invalid for none or an unknown id (an
// imported appearance), which shows the default colour.
QColor appearanceColor(const QString& id);

// The model's physical materials (id, name), steel first as the default.
const QVector<QPair<QString, QString>>& physicalMaterials();

} // namespace mitcad
