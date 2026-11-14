// SPDX-License-Identifier: MIT
#include "Theme.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <optional>
#include <vector>

#include <QAbstractScrollArea>
#include <QEvent>
#include <QObject>
#include <QPointer>
#include <QRegularExpression>
#include <QStyleHints>
#include <QTimer>
#include <QVariant>
#include <QWidget>

namespace mitcad {
namespace {

// Re-applies a widget's themed style sheet when the palette changes. It is a
// child of the widget, so it goes with it. Applying a sheet changes the
// widget's palette too, but a sheet that is already in force is not set
// again, so that does not go round. Qt does not pass a parent's palette
// change on to a child with a style sheet of its own (mitcad#961: a badge
// stayed the light theme's dark red on a dark window), so the sheet is
// also checked when the widget shows and paints, and set again after the
// paint when it is stale. A scroll area (the command panel's message, a
// QTextEdit) paints in its viewport, not itself: the viewport's paints are
// watched too. And as neither event need reach a widget with a sheet of its
// own when the colour scheme switches (mitcad#1152: the command panel's
// error box kept the light theme's colours after a switch to the dark one
// in the Floating layout on Windows, where the new palette reaches the
// widgets only after the switch is announced, and a window that is not
// exposed, as in the build VM's tests, paints nothing), the parent's
// palette changes are watched too (the parent has no sheet of its own that
// keeps its palette), and the sheet is also checked after each switch of
// the colour scheme.
class ThemeWatcher : public QObject {
public:
  explicit ThemeWatcher(QWidget* widget) : QObject(widget), m_widget(widget) {
    widget->installEventFilter(this);
    if (auto* area = qobject_cast<QAbstractScrollArea*>(widget)) {
      m_viewport = area->viewport();
      m_viewport->installEventFilter(this);
    }
    watchParent();
    connect(QGuiApplication::styleHints(), &QStyleHints::colorSchemeChanged, this, [this] { refresh(); });
  }

  void setFormat(const QString& format, ColorRole role, std::optional<ColorRole> second) {
    m_format = format;
    m_role = role;
    m_second = second;
    m_applied.reset();
    apply();
  }

  bool eventFilter(QObject* watched, QEvent* event) override {
    if (watched == m_parent.data() && watched != m_widget) {
      if (event->type() == QEvent::PaletteChange) {
        refresh();
      }
      return false;
    }
    if (watched != m_widget && (watched != m_viewport || event->type() != QEvent::Paint)) {
      return false;
    }
    switch (event->type()) {
    case QEvent::ApplicationPaletteChange:
    case QEvent::PaletteChange:
    case QEvent::ParentChange:
      watchParent();
      apply();
      break;
    case QEvent::Show:
      apply();
      break;
    case QEvent::Paint:
      if (m_applied.has_value() && *m_applied != sheet()) {
        refresh();
      }
      break;
    default:
      break;
    }
    return false;
  }

private:
  void watchParent() {
    QWidget* parent = m_widget->parentWidget();
    if (parent == m_parent.data()) {
      return;
    }
    if (m_parent != nullptr) {
      m_parent->removeEventFilter(this);
    }
    m_parent = parent;
    if (parent != nullptr) {
      parent->installEventFilter(this);
    }
  }

  // The format with the colour for what the widget sits on (mitcad#961): a
  // dark card or window may be on an application palette that is light.
  // The first ancestor with an opaque background (the widget's own sheet
  // does not change theirs), else the application's.
  QString sheet() const {
    if (!m_format.contains(QLatin1String("%1"))) {
      return m_format;
    }
    QPalette palette = QGuiApplication::palette();
    for (const QWidget* up = m_widget->parentWidget(); up != nullptr; up = up->parentWidget()) {
      if (up->palette().color(QPalette::Window).alpha() == 255) {
        palette = up->palette();
        break;
      }
    }
    // A second role where it says %2 (mitcad#1152): a box's fill follows
    // the palette with its text, not only the text on the old fill.
    if (m_second.has_value() && m_format.contains(QLatin1String("%2"))) {
      return m_format.arg(themeColor(m_role, palette).name(), themeColor(*m_second, palette).name());
    }
    return m_format.arg(themeColor(m_role, palette).name());
  }

  void apply() {
    const QString wanted = sheet();
    if (!m_applied.has_value() || *m_applied != wanted) {
      m_applied = wanted;
      m_widget->setStyleSheet(wanted);
    }
  }

  // The sheet set again a moment later (after a paint, or after the colour
  // scheme's palette has reached the widgets), and painted.
  void refresh() {
    if (m_pending) {
      return;
    }
    m_pending = true;
    QTimer::singleShot(0, this, [this] {
      m_pending = false;
      apply();
      m_widget->update();
      if (m_viewport != nullptr) {
        m_viewport->update();
      }
    });
  }

  QWidget* m_widget;
  QString m_format;
  ColorRole m_role = ColorRole::ErrorText;
  std::optional<ColorRole> m_second; // the colour where the format says %2
  QPointer<QWidget> m_viewport; // a scroll area's, where it paints
  QPointer<QWidget> m_parent; // whose palette changes are watched
  bool m_pending = false; // a stale sheet seen in a paint, set again after it
  std::optional<QString> m_applied;
};

void setThemedStyleSheet(QWidget* widget, const QString& format, ColorRole role,
                         std::optional<ColorRole> second = std::nullopt) {
  if (widget == nullptr) {
    return;
  }
  ThemeWatcher* watcher = nullptr;
  for (QObject* child : widget->children()) {
    if (auto* found = dynamic_cast<ThemeWatcher*>(child)) {
      watcher = found;
      break;
    }
  }
  if (watcher == nullptr) {
    watcher = new ThemeWatcher(widget);
  }
  watcher->setFormat(format, role, second);
  // The state, not the colour, for presenters without style sheets.
  static const QRegularExpression textColour(QStringLiteral("(^|[\\s;{])color\\s*:\\s*%1"));
  const bool status = role == ColorRole::ErrorText || role == ColorRole::WarningText;
  const QVariant tone =
      status && textColour.match(format).hasMatch()
          ? QVariant(role == ColorRole::WarningText ? QStringLiteral("warning") : QStringLiteral("error"))
          : QVariant();
  if (widget->property(kSemanticTextToneProperty) != tone) {
    widget->setProperty(kSemanticTextToneProperty, tone);
  }
}

} // namespace

namespace {

// The role table (mitcad#1041). Values are sRGB, except those written with
// linear(): the 3D view's own colors, given in OCCT's linear RGB as they
// always were and converted here, so that they show as before.
QColor hex(unsigned rgb, int alpha = 255) {
  return QColor(static_cast<int>((rgb >> 16) & 0xff), static_cast<int>((rgb >> 8) & 0xff),
                static_cast<int>(rgb & 0xff), alpha);
}

double encode(double value) {
  return value <= 0.0031308 ? 12.92 * value : 1.055 * std::pow(value, 1.0 / 2.4) - 0.055;
}

QColor linear(double r, double g, double b) {
  return QColor::fromRgbF(static_cast<float>(encode(r)), static_cast<float>(encode(g)),
                          static_cast<float>(encode(b)));
}

struct RoleEntry {
  ColorRole role;
  const char* name;
  QColor light;
  QColor dark;
  ColorUse use;
  ColorRole partner;
};

#define MITCAD_ROLE(name, light, dark, use, partner)                                                           \
  RoleEntry { ColorRole::name, #name, light, dark, ColorUse::use, ColorRole::partner }
#define MITCAD_SAME(name, value, use) MITCAD_ROLE(name, value, value, use, name)

const std::vector<RoleEntry>& roleTable() {
  static const std::vector<RoleEntry> table = {
      // Status: the levels of mitcad#1040. Text at least 4.5:1 on the light
      // and dark windows and on the Floating card (#464748, mitcad#961);
      // the warning's hue is yellow (mitcad#852 C2): its text the darkest
      // yellow that reads on a light window.
      MITCAD_ROLE(ErrorText, hex(0xb00020), hex(0xff9c9c), Text, ErrorText),
      MITCAD_ROLE(ErrorBorder, hex(0xd02020), hex(0xff5a5a), Indicator, ErrorBorder),
      MITCAD_ROLE(ErrorFill, hex(0xfbe3e3), hex(0x4a2025), Fill, Text),
      MITCAD_ROLE(ErrorBadge, hex(0xd32f2f), hex(0xd32f2f), Badge, BadgeMark),
      MITCAD_SAME(BadgeMark, hex(0xffffff), View),
      MITCAD_ROLE(WarningText, hex(0x8a5a00), hex(0xe6b450), Text, WarningText),
      MITCAD_ROLE(WarningBorder, hex(0xa87d00), hex(0xf2c230), Indicator, WarningBorder),
      MITCAD_ROLE(WarningFill, hex(0xfff1c8), hex(0x4a3a14), Fill, Text),
      MITCAD_ROLE(WarningBadge, hex(0xf5c400), hex(0xf5c400), Badge, WarningMark),
      MITCAD_SAME(WarningMark, hex(0x2a2000), View),
      MITCAD_ROLE(SuccessText, hex(0x16702f), hex(0x6ccf86), Text, SuccessText),
      MITCAD_ROLE(SuccessBadge, hex(0x1a7f37), hex(0x1a7f37), Badge, BadgeMark),
      MITCAD_ROLE(InfoText, hex(0x4c525c), hex(0xb4b8be), Text, InfoText),
      MITCAD_ROLE(InfoBadge, hex(0x1a6ee0), hex(0x1a6ee0), Badge, BadgeMark),
      // Interaction. The 3D view's selection is one fixed blue (C3); the
      // chrome's follows the system's accent on macOS (accentColor).
      MITCAD_ROLE(Action, hex(0x1a6ee0), hex(0x5aa2ff), Indicator, Action),
      MITCAD_SAME(Select, hex(0x1f7aff), View),
      // About 4.6:1 against the dark background, where the plain blue has
      // about 2.6:1 (mitcad#957).
      MITCAD_ROLE(SelectLine, hex(0x1f7aff), hex(0x66b2ff), View, SelectLine),
      MITCAD_SAME(Hover, hex(0x2f9bff), View),
      MITCAD_ROLE(HoverFill, hex(0xe9f1fc), hex(0x343c48), Fill, Text),
      MITCAD_ROLE(SearchMatch, hex(0x00838f), hex(0x4dd0e1), Indicator, SearchMatch),
      MITCAD_ROLE(DropAllowed, hex(0x1a7f37), hex(0x6ccf86), Indicator, DropAllowed),
      MITCAD_ROLE(DropRefused, hex(0xd02020), hex(0xff5a5a), Indicator, DropRefused),
      // Neutral: the Docked chrome's tokens (WindowColors), Floating's too.
      MITCAD_ROLE(Window, hex(0xe4e7ec), hex(0x141518), View, Window),
      MITCAD_ROLE(Surface, hex(0xf8f9fb), hex(0x1f2024), View, Surface),
      MITCAD_ROLE(Raised, hex(0xe9ecf1), hex(0x2a2c31), View, Raised),
      MITCAD_ROLE(Text, hex(0x1d2026), hex(0xe6e8ec), Text, Text),
      MITCAD_ROLE(TextSecondary, hex(0x4c525c), hex(0xb4b8be), Text, TextSecondary),
      // A hidden Browser row's name (mitcad#1101): dimmed, yet at least
      // 4.5:1 also on a Floating row's card, the window's colour at 80%
      // over the view, which over a white view is about #5b5b5b on a dark
      // theme (the secondary grey #b4b8be read 3.4:1 there); still about
      // 1.2:1 dimmer than Text.
      MITCAD_ROLE(TextHidden, hex(0x4c525c), hex(0xd0d4da), Text, TextHidden),
      MITCAD_ROLE(TitleText, hex(0x5f6670), hex(0x9098a3), Text, TitleText),
      MITCAD_ROLE(Line, hex(0x000000, 22), hex(0xffffff, 18), View, Line),
      MITCAD_ROLE(AccentText, hex(0xffffff), hex(0x0b1220), View, AccentText),
      MITCAD_ROLE(TimelineMarker, hex(0x2b3a4a), hex(0xc8d0da), Indicator, TimelineMarker),
      MITCAD_SAME(GroupBand, hex(0x7d8c9e), View),
      MITCAD_SAME(ViewInk, hex(0x2b3a4a), View),
      // Object states.
      MITCAD_ROLE(BoundText, hex(0x174ea6), hex(0xd6e4ff), Text, BoundText),
      MITCAD_ROLE(BoundFill, hex(0xdce8fb), hex(0x1e3a66), Fill, BoundText),
      // Command feedback in the 3D view: the four inputs, one table for Qt
      // and AppKit.
      MITCAD_SAME(Input1, hex(0x1f7aff), View),
      MITCAD_SAME(Input2, hex(0xff8c00), View),
      MITCAD_SAME(Input3, hex(0x2ca02c), View),
      MITCAD_SAME(Input4, hex(0x9b30ff), View),
      MITCAD_SAME(PreviewAdd, linear(0.30, 0.55, 0.95), View),
      MITCAD_SAME(PreviewRemove, linear(0.90, 0.20, 0.15), View),
      MITCAD_ROLE(PreviewFailed, hex(0xd02020), hex(0xff5a5a), View, PreviewFailed),
      MITCAD_SAME(PreviewTarget, hex(0x00a6b4), View),
      MITCAD_SAME(ExtendedTool, hex(0xff8c00), View),
      MITCAD_ROLE(Interference, hex(0xd02020), hex(0xff5a5a), View, Interference),
      MITCAD_SAME(Handle, hex(0xe07b1a), View),
      MITCAD_SAME(Provisional, linear(0.95, 0.62, 0.12), View),
      MITCAD_SAME(ProvisionalEdge, linear(0.80, 0.42, 0.02), View),
      MITCAD_SAME(FeatureFaces, hex(0xf5a524), View),
      MITCAD_SAME(FeatureFacesHover, hex(0xffcf86), View),
      // White over the handle's dark halo: no axis' hue, no status hue and
      // not the selection's blue, which the Z axis' is close to (mitcad#1707).
      MITCAD_SAME(HandleActive, hex(0xffffff), View),
      // The 3D view's own objects.
      MITCAD_SAME(Body, linear(0.70, 0.74, 0.80), View),
      MITCAD_SAME(Edge, linear(0.12, 0.13, 0.15), View),
      MITCAD_SAME(HiddenEdge, linear(0.45, 0.48, 0.54), View),
      MITCAD_SAME(Thread, linear(0.30, 0.33, 0.40), View),
      MITCAD_SAME(BuildVolume, linear(0.25, 0.45, 0.75), View),
      MITCAD_SAME(PrintBed, hex(0x2eb872), View),
      MITCAD_SAME(SectionCap, hex(0xf29e1a), View),
      MITCAD_SAME(DatumPlane, linear(0.95, 0.70, 0.35), View),
      MITCAD_SAME(DatumAxis, linear(0.85, 0.45, 0.10), View),
      MITCAD_SAME(ProfileFill, linear(0.55, 0.70, 0.95), View),
      // Sketch: one value for the 3D view and the overlay.
      MITCAD_SAME(SketchFree, linear(0.10, 0.35, 0.85), View),
      MITCAD_ROLE(SketchFixed, linear(0.08, 0.09, 0.11), linear(0.88, 0.90, 0.94), View, SketchFixed),
      MITCAD_ROLE(SketchCenterline, linear(0.25, 0.30, 0.40), linear(0.60, 0.66, 0.76), View, SketchCenterline),
      MITCAD_SAME(SketchConstruction, linear(0.85, 0.50, 0.10), View),
      MITCAD_SAME(SketchProjected, linear(0.55, 0.20, 0.70), View),
      MITCAD_ROLE(SketchInactive, hex(0xd63c8f), hex(0xf29cc8), View, SketchInactive),
      MITCAD_SAME(SketchInactiveFill, hex(0xfde4f1, 0xe6), View),
      MITCAD_ROLE(SketchDriven, hex(0x707680), hex(0xa0a8b4), View, SketchDriven),
      MITCAD_ROLE(SketchDimension, hex(0x2a303a), hex(0xd8dee8), View, SketchDimension),
      // The sketch's axes in the overlay as the origin's axes in the view.
      MITCAD_SAME(SketchAxis, linear(0.85, 0.45, 0.10), View),
      MITCAD_SAME(SketchPicked, hex(0x2ca02c), View),
      MITCAD_SAME(SketchPreview, hex(0x266ee8), View),
      MITCAD_SAME(SketchGuide, hex(0x808896), View),
      MITCAD_SAME(SketchTrim, hex(0xe03020), View),
      MITCAD_ROLE(SketchHalo, QColor::fromRgbF(0.98f, 0.98f, 0.99f), QColor::fromRgbF(0.13f, 0.14f, 0.16f), View,
                  SketchHalo),
      // Domain palettes. The components' without a red or an olive, which
      // read as an error and a warning (mitcad#852 C4).
      MITCAD_SAME(Component0, hex(0xe0892a), View),
      MITCAD_SAME(Component1, hex(0x3aa35b), View),
      MITCAD_SAME(Component2, hex(0x8e5bd0), View),
      MITCAD_SAME(Component3, hex(0x2aa7a7), View),
      MITCAD_SAME(Component4, hex(0x5560c8), View),
      MITCAD_SAME(Component5, hex(0x8d6e63), View),
      MITCAD_SAME(Component6, hex(0xc45a9a), View),
      MITCAD_SAME(Component7, hex(0x3d8bd9), View),
      MITCAD_SAME(AxisX, hex(0xe84f4f), View),
      MITCAD_SAME(AxisY, hex(0x4cb05e), View),
      MITCAD_SAME(AxisZ, hex(0x3f8cff), View),
      MITCAD_SAME(JointA, hex(0xf28c28), View),
      MITCAD_SAME(JointB, hex(0x2ab57d), View),
      MITCAD_SAME(JointOrigin, hex(0x17a2b8), View),
      // Print Analysis' categories, violet, magenta and teal, by the view's
      // background.
      MITCAD_ROLE(PrintOverhang, hex(0x7e3fd0), hex(0xb48cff), View, PrintOverhang),
      MITCAD_ROLE(PrintThin, hex(0xc2188b), hex(0xff6ec7), View, PrintThin),
      // Narrow gaps between bodies printed in place (mitcad#1519): teal,
      // apart from the other two and from the status hues.
      MITCAD_ROLE(PrintGap, hex(0x00897b), hex(0x4dd0c0), View, PrintGap),
      // Rule blocks: category hues no status uses, each at least 4.5:1
      // under white text, the same on both themes.
      MITCAD_ROLE(RuleFrame, hex(0x7b55bd), hex(0x7b55bd), Badge, RuleText),
      MITCAD_ROLE(RuleName, hex(0x23766f), hex(0x23766f), Badge, RuleText),
      MITCAD_ROLE(RuleLiteral, hex(0x904a7c), hex(0x904a7c), Badge, RuleText),
      MITCAD_ROLE(RuleFormula, hex(0x69707a), hex(0x69707a), Badge, RuleText),
      MITCAD_ROLE(RuleLogic, hex(0x3d6bc2), hex(0x3d6bc2), Badge, RuleText),
      MITCAD_ROLE(RuleAction, hex(0x8b4f3a), hex(0x8b4f3a), Badge, RuleText),
      MITCAD_SAME(RuleText, hex(0xffffff), View),
      // Simulate's stock (mitcad#1013): a grey stock (nothing left over the
      // part reads as the stock), the material left from teal to violet,
      // a cut into the part pink; none of the status hues.
      MITCAD_ROLE(CamStock, hex(0xb0aaa2), hex(0x8c877f), View, CamStock),
      MITCAD_ROLE(CamLeftThin, hex(0x4fb3bf), hex(0x6fd0db), View, CamLeftThin),
      MITCAD_ROLE(CamLeftThick, hex(0x5e35b1), hex(0x9575cd), View, CamLeftThick),
      MITCAD_ROLE(CamGouge, hex(0xd81b60), hex(0xff5c8d), View, CamGouge),
  };
  return table;
}

#undef MITCAD_SAME
#undef MITCAD_ROLE

const RoleEntry& entry(ColorRole role) { return roleTable()[static_cast<std::size_t>(role)]; }

double luminance(const QColor& color) {
  const auto channel = [](double c) { return c <= 0.04045 ? c / 12.92 : std::pow((c + 0.055) / 1.055, 2.4); };
  const QColor rgb = color.toRgb();
  return 0.2126 * channel(rgb.redF()) + 0.7152 * channel(rgb.greenF()) + 0.0722 * channel(rgb.blueF());
}

} // namespace

const std::vector<ColorRole>& allColorRoles() {
  static const std::vector<ColorRole> roles = [] {
    std::vector<ColorRole> all;
    for (const RoleEntry& row : roleTable()) {
      all.push_back(row.role);
    }
    return all;
  }();
  return roles;
}

QString colorRoleName(ColorRole role) { return QString::fromLatin1(entry(role).name); }

ColorUse colorRoleUse(ColorRole role) { return entry(role).use; }

ColorRole colorRolePartner(ColorRole role) { return entry(role).partner; }

QColor themeColor(ColorRole role, const QPalette& palette) { return themeColor(role, isDarkPalette(palette)); }

QColor themeColor(ColorRole role, bool dark) { return dark ? entry(role).dark : entry(role).light; }

QColor viewColor(ColorRole role, bool darkBackground) { return themeColor(role, darkBackground); }

QColor componentPaletteColor(int index) {
  static const ColorRole palette[] = {ColorRole::Component0, ColorRole::Component1, ColorRole::Component2,
                                      ColorRole::Component3, ColorRole::Component4, ColorRole::Component5,
                                      ColorRole::Component6, ColorRole::Component7};
  return themeColor(palette[((index % 8) + 8) % 8], false);
}

QColor commandInputColor(int index) {
  static const ColorRole inputs[] = {ColorRole::Input1, ColorRole::Input2, ColorRole::Input3, ColorRole::Input4};
  return themeColor(inputs[((index % 4) + 4) % 4], false);
}

double contrastRatio(const QColor& a, const QColor& b) {
  const double la = luminance(a);
  const double lb = luminance(b);
  return (std::max(la, lb) + 0.05) / (std::min(la, lb) + 0.05);
}

bool isDarkPalette(const QPalette& palette) { return palette.color(QPalette::Window).lightness() < 128; }

QPalette themePalette(bool dark) {
  const auto role = [dark](ColorRole r) { return themeColor(r, dark); };
  const QColor window = role(ColorRole::Surface); // the panels' and the dialogs' colour
  const QColor base = role(ColorRole::Window);    // fields and lists, recessed below it
  const QColor raised = role(ColorRole::Raised);  // buttons and tooltips
  const QColor text = role(ColorRole::Text);
  const QColor secondary = role(ColorRole::TextSecondary);
  // Disabled text: further from the text than the secondary text, on either.
  const QColor disabledText = dark ? secondary.darker(150) : secondary.lighter(140);
  const QColor action = role(ColorRole::Action);
  const QColor accentText = role(ColorRole::AccentText);

  QPalette palette;
  const auto set = [&palette](QPalette::ColorRole which, const QColor& enabled, const QColor& disabled) {
    palette.setColor(QPalette::Active, which, enabled);
    palette.setColor(QPalette::Inactive, which, enabled);
    palette.setColor(QPalette::Disabled, which, disabled);
  };
  set(QPalette::Window, window, window);
  set(QPalette::WindowText, text, disabledText);
  set(QPalette::Base, base, window);
  set(QPalette::AlternateBase, window, window);
  set(QPalette::ToolTipBase, raised, raised);
  set(QPalette::ToolTipText, text, text);
  set(QPalette::PlaceholderText, secondary, disabledText);
  set(QPalette::Text, text, disabledText);
  set(QPalette::Button, raised, window);
  set(QPalette::ButtonText, text, disabledText);
  set(QPalette::BrightText, QColor(Qt::white), QColor(Qt::white));
  set(QPalette::Light, raised.lighter(150), raised.lighter(150));
  set(QPalette::Midlight, raised.lighter(125), raised.lighter(125));
  set(QPalette::Mid, raised.darker(130), raised.darker(130));
  set(QPalette::Dark, raised.darker(200), raised.darker(200));
  set(QPalette::Shadow, QColor(Qt::black), QColor(Qt::black));
  set(QPalette::Highlight, action, raised);
  set(QPalette::HighlightedText, accentText, secondary);
  set(QPalette::Link, action, secondary);
  set(QPalette::LinkVisited, action, secondary);
  set(QPalette::Accent, action, raised);
  return palette;
}

QColor errorColor(const QPalette& palette) {
  // At least 4.5:1 on a dark window, panel or Floating card (#464748, mitcad#961).
  return themeColor(ColorRole::ErrorText, palette);
}

QColor warningColor(const QPalette& palette) { return themeColor(ColorRole::WarningText, palette); }

QColor accentColor(const QPalette& palette) {
#ifdef MITCAD_MAC_UI
  return palette.color(QPalette::Accent);
#else
  return actionColor(palette);
#endif
}

QColor actionColor(const QPalette& palette) { return themeColor(ColorRole::Action, palette); }

QColor cardFillColor(const QPalette& palette) {
  QColor color = palette.color(QPalette::Window);
  color.setAlphaF(isDarkPalette(palette) ? 0.80f : 0.85f);
  return color;
}

QColor cardBorderColor(const QPalette& palette) {
  return isDarkPalette(palette) ? QColor(255, 255, 255, 31) : QColor(0, 0, 0, 26);
}

QColor cardShadowColor(const QPalette& palette) {
  return QColor(0, 0, 0, isDarkPalette(palette) ? 115 : 46);
}

void setErrorStyleSheet(QWidget* widget, const QString& format) {
  setThemedStyleSheet(widget, format, ColorRole::ErrorText);
}

void setWarningStyleSheet(QWidget* widget, const QString& format) {
  setThemedStyleSheet(widget, format, ColorRole::WarningText);
}

void setErrorStyleSheet(QWidget* widget, const QString& format, ColorRole fill) {
  setThemedStyleSheet(widget, format, ColorRole::ErrorText, fill);
}

void setWarningStyleSheet(QWidget* widget, const QString& format, ColorRole fill) {
  setThemedStyleSheet(widget, format, ColorRole::WarningText, fill);
}

void setRoleStyleSheet(QWidget* widget, ColorRole role, const QString& format) {
  setThemedStyleSheet(widget, format, role);
}

} // namespace mitcad
