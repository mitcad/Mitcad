// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QList>
#include <QString>
#include <QStringList>
#include <QToolButton>
#include <QWidget>

class QAction;
class QEvent;
class QMenu;
class QPaintEvent;
class QResizeEvent;
class QShowEvent;
class QStackedWidget;
class QTabBar;

namespace mitcad {

class CommandRegistry;

// The toolbar: workspace tabs (SOLID, and SKETCH while sketching),
// each with groups (CREATE, MODIFY, ...) of command buttons; the group's
// name opens a menu of all its commands.
//
// Presentation::Mac is the Floating chrome's: the tabs are a segmented control
// that the title bar hosts (tabBar()), the groups are rounded capsules and the
// buttons are painted without bevels. The objects, their names and the logged
// positions are the same in both.
class Ribbon : public QWidget {
  Q_OBJECT

public:
  enum class Presentation { Classic, Mac };

  explicit Ribbon(CommandRegistry& registry, Presentation presentation = Presentation::Classic,
                  QWidget* parent = nullptr);

  // Entries that are not commands (the SELECT group's filters): `menu`
  // entries go after the group's commands, `pinned` ones are buttons.
  void addGroupExtras(const QString& tab, const QString& group, const QList<QAction*>& menu,
                      const QList<QAction*>& pinned = {});
  // Builds the tabs from the registered commands.
  void build();
  // Shows the SKETCH tab and switches to it, or hides it.
  void setSketchMode(bool sketching);
  // Logs where the group menus are, for UI tests.
  void logLayout() const;

  static const QStringList& groups(const QString& tab);

  Presentation presentation() const { return m_presentation; }
  // The workspace tabs. In the Mac presentation the bar is not part of the
  // ribbon's layout: whoever hosts it (the title bar) places it.
  QTabBar* tabBar() const { return m_tabs; }

signals:
  // The SKETCH tab was shown (true) or hidden.
  void sketchModeChanged(bool sketching);

protected:
  // The SKETCH tab's colour follows the palette.
  void changeEvent(QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void showEvent(QShowEvent* event) override;

private:
  // A group capsule of the Mac presentation and what collapsing it takes.
  struct GroupInfo {
    QWidget* capsule = nullptr;
    QWidget* buttons = nullptr; // the row of pinned commands
    int fullWidth = 0;
    int collapsedWidth = 0;
    bool collapsible = false;   // it has pinned commands to hide
  };
  struct Extras {
    QList<QAction*> menu;
    QList<QAction*> pinned;
  };

  QWidget* createPage(const QString& tab);
  QWidget* createGroup(const QString& tab, const QString& group, GroupInfo* info);
  // Mac presentation: when the groups do not fit the width, the ones at the
  // right show their names only; the names still open their menus.
  void fitGroups();

  CommandRegistry& m_registry;
  Presentation m_presentation;
  QTabBar* m_tabs = nullptr;
  QStackedWidget* m_pages = nullptr;
  QWidget* m_sketchPage = nullptr;
  QHash<QString, Extras> m_extras;   // by "tab/group"
  QHash<QString, QWidget*> m_groupButtons; // by "tab/group", for logLayout
  QHash<QWidget*, QList<GroupInfo>> m_groupsOfPage;
};

// A command button of the Mac presentation (also the title bar's Undo and
// Redo): no bevel, a rounded highlight on hover, a darker one pressed, the
// accent tint when checked. The default action gives icon, tooltip and state.
class RibbonButton : public QToolButton {
  Q_OBJECT

public:
  explicit RibbonButton(QAction* action, int iconSize, QWidget* parent = nullptr);

  QSize sizeHint() const override;

protected:
  void paintEvent(QPaintEvent* event) override;
  void changeEvent(QEvent* event) override;

private:
  int m_iconSize;
};

// A rounded panel in the palette's material (the group capsules, the Undo and
// Redo pair): fill, hairline border and a faint shadow below.
class CapsuleFrame : public QWidget {
  Q_OBJECT

public:
  // A radius below zero is half the height.
  explicit CapsuleFrame(int radius = 12, QWidget* parent = nullptr);

protected:
  void paintEvent(QPaintEvent* event) override;

private:
  int m_radius;
};

// "CREATE" -> "Create", "FINISH SKETCH" -> "Finish Sketch": the names the Mac
// presentation shows. The internal names stay in capitals.
QString displayName(const QString& name);

} // namespace mitcad
