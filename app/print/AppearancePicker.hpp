// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QColor>
#include <QFrame>
#include <QHash>
#include <QIcon>
#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Appearances.hpp"

class QLabel;
class QLineEdit;
class QListWidget;
class QListWidgetItem;

namespace mitcad {

// The appearance library as a popover (mitcad#1055, app/COMMANDS.md
// "Appearances of a material"): a search field over the swatches, first
// those of the material's class (mitcad#1102, appearanceSections), then
// the design's appearances and then the library's, each with its name, and
// Edit Appearances… at the end when the window has it. A click, Return
// or a double-click chooses one (chosen()); Escape or a click outside
// closes it. Typing in the search field keeps the first match current,
// so that Return chooses it; Down goes into the list. A frameless window
// of its own, not a Qt popup, which closes once another window takes the
// activation (as the tools' LibraryPicker): a popup closes as soon as it
// opens in the Windows UI tests' session, and their posted input reaches
// a window of its own as a dialog's (tools/ui-windows-lib.ps1,
// mitcad#1071). The AppKit frontend shows it as a popover under the
// anchor's native control (showNativePicker, mitcad#1068). Logged as
// "<logName> picker opened: N appearances", "<logName> picker sections:
// FDM Plastic 1 | General Plastic 6 | This design 1 | Library 21",
// "<logName> picker search: <text>, M shown" and "<logName> picker
// chosen: <name> (<id>)".
class AppearancePopover : public QFrame {
  Q_OBJECT

public:
  AppearancePopover(const QString& logName, QWidget* parent);

  // The design's and the library's appearances (the `appearances` query).
  void setAppearances(const QVector<Appearance>& appearances);
  // Shows it under `anchor` with `current` selected (empty: none) and
  // `marked` (appearances already there, e.g. of a material's list)
  // marked with a check; `materialClass` (the material's, mitcad#1102)
  // lists the appearances of its class first.
  void open(QWidget* anchor, const QString& current, const QStringList& marked = QStringList(),
            const QString& materialClass = QString());
  // Edit Appearances… at the end; none: no such entry.
  std::function<void()> editAppearances;
  // "None" at the top (a material's default can be none).
  bool offerNone = false;

signals:
  void chosen(const QString& id);

protected:
  bool event(QEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void fill(const QString& current, const QStringList& marked, const QString& materialClass);
  void filter();
  void choose(QListWidgetItem* item);

  QString m_log;
  QVector<Appearance> m_appearances;
  QHash<QString, QIcon> m_icons; // swatches by id and look
  QLineEdit* m_search = nullptr;
  QListWidget* m_list = nullptr;
  bool m_activated = false; // it had the activation since it opened: losing it closes it
};

// The appearances of a picker in sections by a material's class
// (mitcad#1102, app/COMMANDS.md "Appearances of a material"): first those
// of the class, the design's and the library's together, under its title
// ("FDM Plastic"); for FDM Plastic and Resin then General Plastic's; then
// the rest under `designTitle` (the design's) and `libraryTitle`. Empty
// sections are left out; without a class only the last two.
struct AppearanceSection {
  QString title;
  QVector<const Appearance*> appearances;
};
QVector<AppearanceSection> appearanceSections(const QVector<Appearance>& appearances, const QString& materialClass,
                                              const QString& designTitle, const QString& libraryTitle);
// The sections as logged: "FDM Plastic 1 | General Plastic 6 | This design 1".
QString appearanceSectionsText(const QVector<AppearanceSection>& sections);

// The swatch of an appearance as the pickers and a material's list show
// it: a small lit sphere that shows gloss and translucency; `marked` adds
// a ring (a material's default).
QIcon appearanceIcon(const Appearance& appearance, int size, bool marked = false);
// A flat swatch of a colour for an appearance that is not known here (a
// library material's carried one); gray without a colour.
QIcon colorIcon(const QColor& color, int size, bool marked = false);
// The 1 px edge of every swatch, from the role table (ColorRole::TitleText),
// so that a dark swatch stays visible on the dark panel (mitcad#1083).
QColor swatchEdgeColor();

} // namespace mitcad
