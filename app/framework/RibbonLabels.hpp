// SPDX-License-Identifier: MIT
#pragma once

// Preferences › Display › Toolbar labels (mitcad#1936, GitHub
// mitcad/Mitcad#356): the ribbon's command buttons show their icons only
// (the default) or a short label under each icon, in both chrome layouts;
// the Floating layout of Windows and Linux shows the labels by default
// (mitcad#1990). Kept in the application's settings (appearance/
// ribbonLabels: "icons" or "labels"; no key is the default) and applied at once: saveRibbonLabels
// tells RibbonLabelsNotifier, which every window's ribbon follows
// (Ribbon::setButtonLabels). Plain Qt Core.

#include <optional>

#include <QObject>
#include <QString>
#include <QStringList>

namespace mitcad {

enum class RibbonLabels { IconsOnly, UnderIcons };

// "icons", "labels": the setting's values and the log's words.
QString ribbonLabelsId(RibbonLabels labels);
std::optional<RibbonLabels> ribbonLabelsFromId(const QString& id);

QString ribbonLabelsKey();
// The choice without a saved one: Icons only, unless this run's layout set
// another (main: Labels under icons in the Floating layout of Windows and
// Linux, whose toolbar panel shows them by default, mitcad#1990).
RibbonLabels defaultRibbonLabels();
void setDefaultRibbonLabels(RibbonLabels labels);
// The saved choice; the default without one (or with an unknown value).
RibbonLabels savedRibbonLabels();
// Saves it at once and tells RibbonLabelsNotifier; false when it was
// already so.
bool saveRibbonLabels(RibbonLabels labels);

// Tells the ribbons of every window that the choice changed
// (saveRibbonLabels, from Preferences or an import of the settings).
class RibbonLabelsNotifier : public QObject {
  Q_OBJECT

public:
  static RibbonLabelsNotifier& instance();

signals:
  void labelsChanged(bool underIcons);
};

// A button's label: the command's short label (CommandDef::shortLabel)
// when it has one, else its name, without the mnemonic's "&" and a
// trailing ellipsis ("Printers…" -> "Printers"), on one line.
QString ribbonButtonLabel(const QString& name, const QString& shortLabel = QString());

// The short label of a toolbar command whose name is long (mitcad#1975, as
// "Offset" for Offset Plane, "Rectangle" for 2-Point Rectangle), in the
// interface language (context RibbonLabel); empty for a command without
// one. A command's own CommandDef::shortLabel comes first.
QString defaultShortLabel(const QString& id);
// Every command id the table above has, for the tests.
QStringList shortLabelIds();

// The labels' font as a factor of the ribbon's font: the group captions'
// 0.85, at Icon size rather than Text size (the ribbon's font is at Text
// size already), so that a label grows with its icon.
double ribbonLabelFontScale(int iconPercent, int textPercent);

// The widest a label is before it is elided, for an icon of `iconPixels`:
// two and a half icons.
int ribbonLabelWidthLimit(int iconPixels);

} // namespace mitcad
