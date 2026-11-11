// SPDX-License-Identifier: MIT
#include "DrawingTemplates.hpp"

#include <QCoreApplication>
#include <QJsonArray>
#include <QSettings>
#include <QStringList>

namespace mitcad::drawings {

QString sheetSizeText(const QJsonValue& size) {
  if (size.isString()) {
    return size.toString();
  }
  const QJsonObject custom = size.toObject();
  const double width = custom.value(QStringLiteral("width")).toDouble();
  const double height = custom.value(QStringLiteral("height")).toDouble();
  if (width > 0.0 && height > 0.0) {
    return QStringLiteral("%1x%2").arg(QString::number(width, 'g', 10), QString::number(height, 'g', 10));
  }
  return QString();
}

bool customSheetSize(const QString& text, double* width, double* height) {
  const QStringList parts = text.split(QLatin1Char('x'));
  if (parts.size() != 2) {
    return false;
  }
  bool okWidth = false;
  bool okHeight = false;
  const double w = parts.at(0).toDouble(&okWidth);
  const double h = parts.at(1).toDouble(&okHeight);
  if (!okWidth || !okHeight || !(w > 0.0) || !(h > 0.0)) {
    return false;
  }
  if (width != nullptr) {
    *width = w;
  }
  if (height != nullptr) {
    *height = h;
  }
  return true;
}

QJsonValue sheetSizeValue(const QString& text) {
  double width = 0.0;
  double height = 0.0;
  if (customSheetSize(text, &width, &height)) {
    return QJsonObject{{QStringLiteral("width"), width}, {QStringLiteral("height"), height}};
  }
  return text;
}

QList<TemplateChoice> templateChoices(const QJsonObject& answer) {
  QList<TemplateChoice> out;
  for (const QJsonValue& entry : answer.value(QStringLiteral("templates")).toArray()) {
    QJsonObject data = entry.toObject();
    const QString choice = data.value(QStringLiteral("choice")).toString();
    const QString library = data.value(QStringLiteral("library")).toString();
    const QString name = data.value(QStringLiteral("name")).toString();
    if (choice.isEmpty() || name.isEmpty()) {
      continue;
    }
    TemplateChoice item;
    item.choice = choice;
    item.size = sheetSizeText(data.value(QStringLiteral("size")));
    item.projection = data.value(QStringLiteral("projection")).toString();
    if (library == QStringLiteral("user")) {
      item.label = QCoreApplication::translate("DrawingTemplates", "%1 (User)").arg(name);
    } else if (library == QStringLiteral("project")) {
      item.label = QCoreApplication::translate("DrawingTemplates", "%1 (Project)").arg(name);
    } else {
      item.label = name;
    }
    if (library == QStringLiteral("user") || library == QStringLiteral("project")) {
      // A library's template goes to the model as its data: the sheet keeps
      // a copy of it.
      data.remove(QStringLiteral("choice"));
      item.value = data;
    } else {
      item.value = data.value(QStringLiteral("id")).toString();
    }
    out.append(item);
  }
  TemplateChoice simple;
  simple.label = QCoreApplication::translate("DrawingTemplates", "Simple Title Block");
  simple.value = QString();
  out.append(simple);
  return out;
}

int templateIndex(const QList<TemplateChoice>& choices, const QString& choice) {
  for (int i = 0; i < choices.size(); ++i) {
    if (choices.at(i).choice == choice) {
      return i;
    }
  }
  return -1;
}

QString defaultTemplate(const QString& size) {
  return size.startsWith(QStringLiteral("ANSI")) ? QStringLiteral("ansi") : QStringLiteral("iso");
}

namespace {

const QString kChoiceGroup = QStringLiteral("createDrawing");

} // namespace

std::optional<NewDrawingChoice> NewDrawingChoice::load(QSettings& settings) {
  settings.beginGroup(kChoiceGroup);
  std::optional<NewDrawingChoice> out;
  if (settings.contains(QStringLiteral("template"))) {
    NewDrawingChoice c;
    c.choice = settings.value(QStringLiteral("template")).toString();
    c.size = settings.value(QStringLiteral("size")).toString();
    c.orientation = settings.value(QStringLiteral("orientation")).toString();
    c.projection = settings.value(QStringLiteral("projection")).toString();
    out = c;
  }
  settings.endGroup();
  return out;
}

void NewDrawingChoice::save(QSettings& settings) const {
  settings.beginGroup(kChoiceGroup);
  settings.setValue(QStringLiteral("template"), choice);
  settings.setValue(QStringLiteral("size"), size);
  settings.setValue(QStringLiteral("orientation"), orientation);
  settings.setValue(QStringLiteral("projection"), projection);
  settings.endGroup();
}

StartingChoice startingChoice(const QList<TemplateChoice>& templates, const std::optional<NewDrawingChoice>& last) {
  StartingChoice out;
  if (last && templateIndex(templates, last->choice) >= 0) {
    out.choice = *last;
    out.source = StartSource::Last;
    return out;
  }
  // Today's default: ISO, which brings its sheet and projection method.
  const int iso = templateIndex(templates, defaultTemplate(QStringLiteral("A4")));
  if (iso >= 0) {
    out.choice.choice = templates.at(iso).choice;
  } else if (!templates.isEmpty()) {
    out.choice.choice = templates.first().choice;
  }
  if (last) {
    out.note = QStringLiteral("Create Drawing: the last template %1 is not offered; %2 instead")
                   .arg(last->choice.isEmpty() ? QStringLiteral("simple") : last->choice,
                        out.choice.choice.isEmpty() ? QStringLiteral("simple") : out.choice.choice);
  }
  return out;
}

QString startSourceName(StartSource source) {
  return source == StartSource::Last ? QStringLiteral("the last choice") : QStringLiteral("the default");
}

} // namespace mitcad::drawings
