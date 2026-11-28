// SPDX-License-Identifier: MIT
#include "CamToolFit.hpp"

#include <QJsonValue>

namespace mitcad::cam {

namespace {

QString kindOf(const QJsonObject& tool) {
  return tool.value(QStringLiteral("shape")).toObject().value(QStringLiteral("kind")).toString();
}

int numberOf(const QJsonObject& tool) { return tool.value(QStringLiteral("number")).toInt(); }

QString numberText(const QJsonObject& tool) { return QString::number(numberOf(tool)); }

} // namespace

Cutters cuttersOf(const QString& type, const QString& side) {
  if (type == QStringLiteral("drill")) {
    return Cutters::Any;
  }
  if (type == QStringLiteral("raster_finish") || type == QStringLiteral("waterline_finish")) {
    return Cutters::Finishing;
  }
  if (type == QStringLiteral("contour") && side == QStringLiteral("on")) {
    return Cutters::EndMillOrVBit;
  }
  // Facing, Contour beside the outlines, Pocket, Adaptive Clearing, Rest
  // Machining and 3D Roughing: check_mill.
  return Cutters::EndMill;
}

bool isVBit(const QJsonObject& tool) {
  const QString kind = kindOf(tool);
  return kind == QStringLiteral("engraver") || kind == QStringLiteral("chamfer");
}

bool toolFits(const QJsonObject& tool, Cutters cutters) {
  const QString kind = kindOf(tool);
  const bool endMill = kind == QStringLiteral("flat") || kind == QStringLiteral("bull");
  switch (cutters) {
  case Cutters::Any:
    return true;
  case Cutters::EndMill:
    return endMill;
  case Cutters::EndMillOrVBit:
    return endMill || isVBit(tool);
  case Cutters::Finishing:
    return endMill || kind == QStringLiteral("ball");
  }
  return false;
}

QJsonObject toolOf(const QJsonArray& tools, int number) {
  for (const QJsonValue& value : tools) {
    if (numberOf(value.toObject()) == number) {
      return value.toObject();
    }
  }
  return {};
}

QJsonArray offeredTools(const QJsonArray& tools, Cutters cutters, int current) {
  QJsonArray offered;
  for (const QJsonValue& value : tools) {
    const QJsonObject tool = value.toObject();
    if (toolFits(tool, cutters) || numberOf(tool) == current) {
      offered.append(tool);
    }
  }
  return offered;
}

QString defaultTool(const QJsonArray& tools, const QString& type, const QString& side) {
  const Cutters cutters = cuttersOf(type, side);
  if (cutters == Cutters::Any) {
    for (const QJsonValue& value : tools) {
      if (kindOf(value.toObject()) == QStringLiteral("drill")) {
        return numberText(value.toObject());
      }
    }
    return tools.isEmpty() ? QString() : numberText(tools.first().toObject());
  }
  if (cutters == Cutters::Finishing) {
    for (const QJsonValue& value : tools) {
      if (kindOf(value.toObject()) == QStringLiteral("ball")) {
        return numberText(value.toObject());
      }
    }
  }
  // An end mill first; on the line an engraving cutter or a chamfer mill
  // when the machine has no end mill.
  for (const QJsonValue& value : tools) {
    if (toolFits(value.toObject(), Cutters::EndMill)) {
      return numberText(value.toObject());
    }
  }
  for (const QJsonValue& value : tools) {
    if (toolFits(value.toObject(), cutters)) {
      return numberText(value.toObject());
    }
  }
  return QString();
}

} // namespace mitcad::cam
