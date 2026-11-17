// SPDX-License-Identifier: MIT
#include "RibbonLabels.hpp"

#include <QChar>
#include <QSettings>

namespace mitcad {

QString ribbonLabelsId(RibbonLabels labels) {
  return labels == RibbonLabels::UnderIcons ? QStringLiteral("labels") : QStringLiteral("icons");
}

std::optional<RibbonLabels> ribbonLabelsFromId(const QString& id) {
  if (id == QLatin1String("icons")) {
    return RibbonLabels::IconsOnly;
  }
  if (id == QLatin1String("labels")) {
    return RibbonLabels::UnderIcons;
  }
  return std::nullopt;
}

QString ribbonLabelsKey() { return QStringLiteral("appearance/ribbonLabels"); }

RibbonLabels savedRibbonLabels() {
  return ribbonLabelsFromId(QSettings().value(ribbonLabelsKey()).toString()).value_or(RibbonLabels::IconsOnly);
}

bool saveRibbonLabels(RibbonLabels labels) {
  if (savedRibbonLabels() == labels) {
    return false;
  }
  {
    QSettings settings;
    settings.setValue(ribbonLabelsKey(), ribbonLabelsId(labels));
  }
  emit RibbonLabelsNotifier::instance().labelsChanged(labels == RibbonLabels::UnderIcons);
  return true;
}

RibbonLabelsNotifier& RibbonLabelsNotifier::instance() {
  static RibbonLabelsNotifier notifier;
  return notifier;
}

QString ribbonButtonLabel(const QString& name, const QString& shortLabel) {
  QString label = shortLabel.isEmpty() ? name : shortLabel;
  // The mnemonic's "&" goes; "&&" is a literal ampersand.
  QString plain;
  plain.reserve(label.size());
  for (int i = 0; i < label.size(); ++i) {
    if (label.at(i) == QLatin1Char('&') && i + 1 < label.size()) {
      ++i;
    }
    plain.append(label.at(i));
  }
  label = plain.simplified();
  if (label.endsWith(QChar(0x2026))) {
    label.chop(1);
  } else if (label.endsWith(QLatin1String("..."))) {
    label.chop(3);
  }
  return label.trimmed();
}

double ribbonLabelFontScale(int iconPercent, int textPercent) {
  if (iconPercent <= 0 || textPercent <= 0) {
    return 0.85;
  }
  return 0.85 * iconPercent / textPercent;
}

int ribbonLabelWidthLimit(int iconPixels) { return iconPixels * 5 / 2; }

} // namespace mitcad
