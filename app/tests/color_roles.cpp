// SPDX-License-Identifier: MIT
// The color roles (mitcad#1041, docs/ui-conventions.md "Colors"): the role
// table is complete and in the enum's order; every text role reads at
// least 4.5:1 and every indicator 3:1 on the light and the dark chrome's
// surfaces, every fill under its text and every badge under its mark; the
// status hues stay reserved: the domain palettes keep away from the error's
// red and the warning's yellow; the AppKit chrome's shared tables (the
// command inputs, the components) read the roles.
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <set>
#include <vector>

#include <QColor>
#include <QPalette>
#include <QString>

#include "framework/NoticeLevel.hpp"
#include "framework/Theme.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line, const QString& what = QString()) {
  if (!condition) {
    std::fprintf(stderr, "color_roles.cpp:%d: check failed: %s %s\n", line, expression, qPrintable(what));
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)
#define CHECK_ROLE(expr, role) check((expr), #expr, __LINE__, colorRoleName(role))

using namespace mitcad;

// The surfaces the chrome draws text and indicators on, light and dark:
// the window, the panels and the raised controls (WindowColors), white,
// and on a dark theme the Floating card (#464748, mitcad#961) for the
// status texts.
std::vector<QColor> surfaces(bool dark) {
  std::vector<QColor> all = {themeColor(ColorRole::Window, dark), themeColor(ColorRole::Surface, dark),
                             themeColor(ColorRole::Raised, dark)};
  if (!dark) {
    all.push_back(QColor(Qt::white));
  }
  return all;
}

void tableTests() {
  const std::vector<ColorRole>& roles = allColorRoles();
  CHECK(!roles.empty());
  std::set<QString> names;
  for (std::size_t i = 0; i < roles.size(); ++i) {
    // In the enum's order: the table is read by the role's value.
    CHECK_ROLE(static_cast<std::size_t>(roles[i]) == i, roles[i]);
    const QString name = colorRoleName(roles[i]);
    CHECK(!name.isEmpty());
    CHECK_ROLE(names.insert(name).second, roles[i]);
    CHECK_ROLE(themeColor(roles[i], false).isValid() && themeColor(roles[i], true).isValid(), roles[i]);
  }
  CHECK(roles.back() == ColorRole::RuleText);
  // A palette's lightness picks the value; the 3D view's by its background.
  QPalette night;
  night.setColor(QPalette::Window, QColor(0x14, 0x15, 0x18));
  QPalette day;
  day.setColor(QPalette::Window, QColor(0xe4, 0xe7, 0xec));
  CHECK(themeColor(ColorRole::ErrorText, night) == themeColor(ColorRole::ErrorText, true));
  CHECK(themeColor(ColorRole::ErrorText, day) == themeColor(ColorRole::ErrorText, false));
  CHECK(errorColor(day) == themeColor(ColorRole::ErrorText, false));
  CHECK(warningColor(night) == themeColor(ColorRole::WarningText, true));
  CHECK(actionColor(day) == themeColor(ColorRole::Action, false));
  CHECK(viewColor(ColorRole::SelectLine, true) != viewColor(ColorRole::SelectLine, false));
  CHECK(viewColor(ColorRole::PrintOverhang, true) != viewColor(ColorRole::PrintOverhang, false));
  // One selection blue and one hover blue in the 3D view on any background
  // (mitcad#852 C3).
  CHECK(viewColor(ColorRole::Select, true) == viewColor(ColorRole::Select, false));
  CHECK(viewColor(ColorRole::Hover, true) == viewColor(ColorRole::Hover, false));
  // The levels read the table (mitcad#1040).
  CHECK(noticeColor(NoticeLevel::Success, day) == themeColor(ColorRole::SuccessText, false));
  CHECK(noticeColor(NoticeLevel::Hint, night) == themeColor(ColorRole::InfoText, true));
  CHECK(noticeFill(NoticeLevel::Warning, night) == themeColor(ColorRole::WarningFill, true));
  // The command inputs' and the components' shared tables, Qt's and
  // AppKit's, cycling.
  CHECK(commandInputColor(0) == themeColor(ColorRole::Input1, false));
  CHECK(commandInputColor(5) == themeColor(ColorRole::Input2, false));
  CHECK(componentPaletteColor(0) == themeColor(ColorRole::Component0, false));
  CHECK(componentPaletteColor(9) == themeColor(ColorRole::Component1, false));
  CHECK(componentPaletteColor(-1) == themeColor(ColorRole::Component7, false));
  CHECK(kStateDimOpacity > 0.0 && kStateDimOpacity < 1.0);
}

void contrastTests() {
  // The ratio itself: black on white is 21:1, a colour on itself 1:1.
  CHECK(std::abs(contrastRatio(QColor(Qt::black), QColor(Qt::white)) - 21.0) < 0.01);
  CHECK(std::abs(contrastRatio(QColor(0x80, 0x80, 0x80), QColor(0x80, 0x80, 0x80)) - 1.0) < 1e-9);
  for (const ColorRole role : allColorRoles()) {
    for (const bool dark : {false, true}) {
      const QColor value = themeColor(role, dark);
      const QColor partner = themeColor(colorRolePartner(role), dark);
      switch (colorRoleUse(role)) {
      case ColorUse::Text:
        for (const QColor& surface : surfaces(dark)) {
          CHECK_ROLE(contrastRatio(value, surface) >= 4.5, role);
        }
        break;
      case ColorUse::Indicator:
        for (const QColor& surface : surfaces(dark)) {
          CHECK_ROLE(contrastRatio(value, surface) >= 3.0, role);
        }
        break;
      case ColorUse::Fill:
        // Under its text (the window's text, or its own).
        CHECK_ROLE(colorRolePartner(role) != role, role);
        CHECK_ROLE(contrastRatio(partner, value) >= 4.5, role);
        break;
      case ColorUse::Badge:
        // Under its mark or its text.
        CHECK_ROLE(colorRolePartner(role) != role, role);
        CHECK_ROLE(contrastRatio(partner, value) >= 4.5, role);
        break;
      case ColorUse::View:
        break;
      }
    }
  }
  // The status texts also on the Floating card (mitcad#961).
  const QColor card(0x46, 0x47, 0x48);
  for (const ColorRole role :
       {ColorRole::ErrorText, ColorRole::WarningText, ColorRole::SuccessText, ColorRole::InfoText}) {
    CHECK_ROLE(contrastRatio(themeColor(role, true), card) >= 4.5, role);
  }
  // The bound state's text on its own fill (the fx badges).
  for (const bool dark : {false, true}) {
    CHECK(contrastRatio(themeColor(ColorRole::BoundText, dark), themeColor(ColorRole::BoundFill, dark)) >= 4.5);
  }
}

// How far apart two hues are, in degrees (0 to 180).
double hueDistance(const QColor& a, const QColor& b) {
  const double d = std::abs(a.hsvHueF() - b.hsvHueF()) * 360.0;
  return std::min(d, 360.0 - d);
}

void reservedHueTests() {
  // The domain palettes with a legend keep away from the error's red and
  // the warning's yellow (mitcad#852 C2, C4, C5), or are grey enough not
  // to read as them.
  const QColor red = themeColor(ColorRole::ErrorBadge, false);
  const QColor yellow = themeColor(ColorRole::WarningBadge, false);
  const ColorRole domain[] = {
      ColorRole::Component0, ColorRole::Component1, ColorRole::Component2, ColorRole::Component3,
      ColorRole::Component4, ColorRole::Component5, ColorRole::Component6, ColorRole::Component7,
      ColorRole::RuleFrame,  ColorRole::RuleName,   ColorRole::RuleLiteral, ColorRole::RuleFormula,
      ColorRole::RuleLogic,  ColorRole::RuleAction, ColorRole::PrintOverhang, ColorRole::PrintThin,
      ColorRole::SearchMatch, ColorRole::JointA,    ColorRole::JointB,     ColorRole::JointOrigin,
  };
  for (const ColorRole role : domain) {
    for (const bool dark : {false, true}) {
      const QColor value = themeColor(role, dark);
      const bool grey = value.hsvSaturationF() < 0.35;
      CHECK_ROLE(grey || hueDistance(value, red) >= 12.0, role);
      CHECK_ROLE(grey || hueDistance(value, yellow) >= 12.0, role);
    }
  }
  // The warning is yellow, apart from the handles' orange (C2).
  CHECK(hueDistance(yellow, themeColor(ColorRole::Handle, false)) >= 15.0);
  // The rule blocks' actions and literals off the status hues (C5): not
  // the success's green either.
  const QColor green = themeColor(ColorRole::SuccessBadge, false);
  CHECK(hueDistance(themeColor(ColorRole::RuleLiteral, false), green) >= 40.0);
  CHECK(hueDistance(themeColor(ColorRole::RuleAction, false), yellow) >= 20.0);
}

} // namespace

int colorRoleTests() {
  failures = 0;
  tableTests();
  contrastTests();
  reservedHueTests();
  return failures;
}
