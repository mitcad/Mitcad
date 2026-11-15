// SPDX-License-Identifier: MIT
#include "view/SelectionLook.hpp"

#include <QSettings>

#include "framework/Theme.hpp"

namespace mitcad {
namespace {

constexpr const char* kColorKey = "view/selectionColor";
constexpr const char* kWidthKey = "view/selectedLineWidth";

// The application's choice as read or last saved (savedSelectionLook).
std::optional<SelectionLook>& cachedLook() {
  static std::optional<SelectionLook> look;
  return look;
}

} // namespace

QColor defaultSelectionColor() { return viewColor(ColorRole::Select, false); }

QString selectedLineWidthId(SelectedLineWidth width) {
  switch (width) {
  case SelectedLineWidth::Thin:
    return QStringLiteral("thin");
  case SelectedLineWidth::Thick:
    return QStringLiteral("thick");
  case SelectedLineWidth::Normal:
    break;
  }
  return QStringLiteral("normal");
}

std::optional<SelectedLineWidth> selectedLineWidthFromId(const QString& id) {
  for (const SelectedLineWidth width : {SelectedLineWidth::Thin, SelectedLineWidth::Normal, SelectedLineWidth::Thick}) {
    if (id == selectedLineWidthId(width)) {
      return width;
    }
  }
  return std::nullopt;
}

double selectedEdgeWidth(SelectedLineWidth width) {
  switch (width) {
  case SelectedLineWidth::Thin:
    return 3.0;
  case SelectedLineWidth::Thick:
    return 6.0;
  case SelectedLineWidth::Normal:
    break;
  }
  return 4.0;
}

double selectedBoundaryWidth(SelectedLineWidth width) {
  switch (width) {
  case SelectedLineWidth::Thin:
    return 2.0;
  case SelectedLineWidth::Thick:
    return 4.0;
  case SelectedLineWidth::Normal:
    break;
  }
  return 3.0;
}

SelectionLook SelectionLook::load(const QSettings& settings) {
  SelectionLook look;
  const QColor color(settings.value(QLatin1String(kColorKey)).toString());
  if (color.isValid()) {
    look.color = QColor(color.rgb()); // opaque
  }
  look.width = selectedLineWidthFromId(settings.value(QLatin1String(kWidthKey)).toString())
                   .value_or(SelectedLineWidth::Normal);
  return look;
}

void SelectionLook::save(QSettings& settings) const {
  settings.setValue(QLatin1String(kColorKey), color.name());
  settings.setValue(QLatin1String(kWidthKey), selectedLineWidthId(width));
}

QString SelectionLook::describe() const {
  return QStringLiteral("color %1, line width %2").arg(color.name(), selectedLineWidthId(width));
}

SelectionLook savedSelectionLook() {
  std::optional<SelectionLook>& cached = cachedLook();
  if (!cached) {
    cached = SelectionLook::load(QSettings());
  }
  return *cached;
}

bool saveSelectionLook(const SelectionLook& look) {
  if (savedSelectionLook() == look) {
    return false;
  }
  {
    QSettings settings;
    look.save(settings);
  }
  cachedLook() = look;
  emit SelectionLookNotifier::instance().changed();
  return true;
}

QColor selectionColor() { return savedSelectionLook().color; }

SelectionLookNotifier& SelectionLookNotifier::instance() {
  static SelectionLookNotifier notifier;
  return notifier;
}

} // namespace mitcad
