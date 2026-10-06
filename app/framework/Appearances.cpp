// SPDX-License-Identifier: MIT
#include "Appearances.hpp"

#include <QObject>

namespace mitcad {

const QVector<Appearance>& appearances() {
  static const QVector<Appearance> list = {
      {QStringLiteral("steel_satin"), QObject::tr("Steel - Satin"), QColor(0xb4, 0xb8, 0xbe)},
      {QStringLiteral("aluminum_anodized"), QObject::tr("Aluminum - Anodized"), QColor(0xd2, 0xd6, 0xdc)},
      {QStringLiteral("brass_polished"), QObject::tr("Brass - Polished"), QColor(0xc8, 0xa8, 0x4b)},
      {QStringLiteral("copper"), QObject::tr("Copper"), QColor(0xc0, 0x6c, 0x3e)},
      {QStringLiteral("cast_iron"), QObject::tr("Iron - Cast"), QColor(0x5a, 0x5c, 0x60)},
      {QStringLiteral("paint_red"), QObject::tr("Paint - Red"), QColor(0xc8, 0x28, 0x28)},
      {QStringLiteral("paint_blue"), QObject::tr("Paint - Blue"), QColor(0x2a, 0x5c, 0xc0)},
      {QStringLiteral("paint_green"), QObject::tr("Paint - Green"), QColor(0x3a, 0x9a, 0x48)},
      {QStringLiteral("paint_yellow"), QObject::tr("Paint - Yellow"), QColor(0xe8, 0xc4, 0x20)},
      {QStringLiteral("paint_black"), QObject::tr("Paint - Black"), QColor(0x2a, 0x2c, 0x30)},
      {QStringLiteral("paint_white"), QObject::tr("Paint - White"), QColor(0xf0, 0xf0, 0xec)},
      {QStringLiteral("plastic_black"), QObject::tr("Plastic - Black"), QColor(0x34, 0x36, 0x3a)},
      {QStringLiteral("plastic_orange"), QObject::tr("Plastic - Orange"), QColor(0xe8, 0x7a, 0x1e)},
      {QStringLiteral("rubber"), QObject::tr("Rubber"), QColor(0x22, 0x22, 0x24)},
      {QStringLiteral("glass"), QObject::tr("Glass"), QColor(0xa8, 0xd0, 0xe0)},
      {QStringLiteral("wood_oak"), QObject::tr("Wood - Oak"), QColor(0xb0, 0x84, 0x50)},
  };
  return list;
}

QColor appearanceColor(const QString& id) {
  for (const Appearance& appearance : appearances()) {
    if (appearance.id == id) {
      return appearance.color;
    }
  }
  return QColor();
}

const QVector<QPair<QString, QString>>& physicalMaterials() {
  // core/model/src/analysis.rs, MATERIALS.
  static const QVector<QPair<QString, QString>> list = {
      {QStringLiteral("steel"), QObject::tr("Steel")},
      {QStringLiteral("stainless_steel"), QObject::tr("Stainless Steel")},
      {QStringLiteral("cast_iron"), QObject::tr("Cast Iron")},
      {QStringLiteral("aluminum"), QObject::tr("Aluminum")},
      {QStringLiteral("copper"), QObject::tr("Copper")},
      {QStringLiteral("brass"), QObject::tr("Brass")},
      {QStringLiteral("titanium"), QObject::tr("Titanium")},
      {QStringLiteral("abs"), QObject::tr("ABS Plastic")},
      {QStringLiteral("pla"), QObject::tr("PLA Plastic")},
      {QStringLiteral("nylon"), QObject::tr("Nylon")},
      {QStringLiteral("polycarbonate"), QObject::tr("Polycarbonate")},
      {QStringLiteral("glass"), QObject::tr("Glass")},
      {QStringLiteral("water"), QObject::tr("Water")},
  };
  return list;
}

} // namespace mitcad
