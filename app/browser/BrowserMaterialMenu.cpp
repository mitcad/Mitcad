// SPDX-License-Identifier: MIT
// The context menus' Material ▸ submenu (mitcad#1046; app/COMMANDS.md,
// "Material ▸ in the context menus"): the design's materials, the library
// materials assigned last, and More… (the Material tool, mitcad#1059), for
// the view's and the Browser's menus of bodies and components. A pick
// assigns at once, without the tool's panel. A material that offers two or
// more appearances is a submenu of them (mitcad#1055): Material ▸ PLA
// Basic ▸ Black, picked by hovering, assigns the material and that
// appearance.
#include <exception>
#include <utility>

#include <QAction>
#include <QColor>
#include <QIcon>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QMenu>
#include <QPair>
#include <QVariant>
#include <QVector>
#include <QtLogging>

#include "../framework/Appearances.hpp"
#include "../framework/Command.hpp"
#include "../framework/Theme.hpp"
#include "../framework/Icons.hpp"
#include "../print/MaterialMenuChoices.hpp"
#include "../print/MaterialsDialog.hpp"
#include "../print/PrintLibrary.hpp"
#include "Bindings.hpp"
#include "BrowserController.hpp"
#include "DocumentHost.hpp"

namespace mitcad {
namespace {

const char* kFilled = "materialMenuFilled";

// A design's built-in material (steel, PLA, ...): always there, the
// design's only while its bodies or components use it.
bool isBuiltIn(const LibraryItem& item) {
  return item.place == QStringLiteral("mitcad") && item.library.isEmpty();
}

// The appearances a material offers (mitcad#1055), its default first: the
// design's copy's (the `materials` query), else the library row's.
QStringList offeredAppearances(const LibraryItem& item, const QJsonArray& designMaterials) {
  QJsonObject fields = item.row;
  const QString designId = item.designId();
  for (const QJsonValue& value : designMaterials) {
    if (!designId.isEmpty() && value.toObject().value(QStringLiteral("id")).toString() == designId) {
      fields = value.toObject();
    }
  }
  QStringList list;
  for (const QJsonValue& value : fields.value(QStringLiteral("appearances")).toArray()) {
    if (!value.toString().isEmpty() && !list.contains(value.toString())) {
      list << value.toString();
    }
  }
  return list;
}

// A label for a menu entry: & doubled.
QString menuLabel(QString text) {
  text.replace(QLatin1Char('&'), QStringLiteral("&&"));
  return text;
}

} // namespace

QMenu* BrowserController::materialMenu(QWidget* owner, const QStringList& bodies, const QString& component,
                                       bool bound) {
  auto* submenu = new QMenu(tr("Material"), owner);
  // While a target's material follows an expression (mitcad#861): the fx
  // mark as on Appearance….
  submenu->menuAction()->setIcon(bound ? boundIcon(themeIcon(QStringLiteral("material")))
                                       : themeIcon(QStringLiteral("material")));
  if (bound) {
    submenu->menuAction()->setText(tr("Material") + QStringLiteral("\tfx"));
  }
  // More… is there from the start; the materials are read when the
  // submenu opens (the library folders are read then, not on every right
  // click, mitcad#981).
  QAction* more = new QAction(tr("More…"), submenu);
  connect(more, &QAction::triggered, submenu, [this, bodies, component] { openMaterial(bodies, component); });
  submenu->addAction(more);
  connect(submenu, &QMenu::aboutToShow, submenu, [this, submenu, more, bodies, component] {
    if (submenu->property(kFilled).toBool()) {
      return;
    }
    submenu->setProperty(kFilled, true);
    fillMaterialMenu(submenu, more, bodies, component);
  });
  return submenu;
}

void BrowserController::fillMaterialMenu(QMenu* submenu, QAction* more, const QStringList& bodies,
                                         const QString& component) {
  const QVector<LibraryItem> items =
      queryLibraryItems(m_host.model(), QStringLiteral("material"), m_host.projectFolder());
  // What each target is made of: a component its own default material, a
  // body the material it is made of (its own, else its component's).
  // The design's materials and the appearances (mitcad#1055): what each
  // offers and their names and swatches.
  QJsonArray designMaterials;
  QJsonArray bodyRows;
  QVector<Appearance> appearances;
  try {
    designMaterials = m_host.model().queryArray(QStringLiteral("materials"));
    appearances = appearancesOf(m_host.model().queryArray(QStringLiteral("appearances")));
    if (!bodies.isEmpty()) {
      bodyRows = m_host.model().queryArray(QStringLiteral("bodies"));
    }
  } catch (const std::exception&) {
    // Without them: no appearance submenus.
  }
  // The design material's default appearance by id.
  const auto defaultLook = [&designMaterials](const QString& id) {
    for (const QJsonValue& value : std::as_const(designMaterials)) {
      if (value.toObject().value(QStringLiteral("id")).toString() == id) {
        return value.toObject().value(QStringLiteral("appearance")).toString();
      }
    }
    return QString();
  };
  QStringList perTarget;
  QStringList perTargetLook; // the appearance each target has (mitcad#1055)
  if (!component.isEmpty()) {
    QString own;
    QString look;
    for (const LibraryItem& item : items) {
      for (const QJsonValue& uid : item.row.value(QStringLiteral("components")).toArray()) {
        if (uid.toString() == component) {
          own = item.key();
        }
      }
    }
    for (const QJsonValue& value : std::as_const(designMaterials)) {
      const QJsonObject material = value.toObject();
      if (material.value(QStringLiteral("components")).toArray().contains(component)) {
        look = material.value(QStringLiteral("component_appearances")).toObject().value(component).toString();
        if (look.isEmpty()) {
          look = material.value(QStringLiteral("appearance")).toString();
        }
      }
    }
    perTarget << own;
    perTargetLook << look;
  } else {
    for (const QString& body : bodies) {
      QString id;
      try {
        id = m_host.model()
                 .queryObject({{QStringLiteral("query"), QStringLiteral("body_material")},
                               {QStringLiteral("body"), body}})
                 .value(QStringLiteral("id"))
                 .toString();
      } catch (const std::exception&) {
        // Made of nothing known: nothing checked.
      }
      const LibraryItem* found = findDesignCopy(items, id);
      perTarget << (found != nullptr ? found->key() : QString());
      QString look;
      for (const QJsonValue& value : std::as_const(bodyRows)) {
        if (value.toObject().value(QStringLiteral("uid")).toString() == body) {
          look = value.toObject().value(QStringLiteral("appearance")).toString();
        }
      }
      perTargetLook << (look.isEmpty() ? defaultLook(id) : look);
    }
  }
  QVector<MaterialChoice> choices;
  for (const LibraryItem& item : items) {
    MaterialChoice choice;
    choice.key = item.key();
    choice.inDesign = !item.designId().isEmpty() && (!isBuiltIn(item) || usedByDesign(item));
    choices.append(choice);
  }
  const MaterialMenuRows rows = materialMenuRows(choices, perTarget, recentMaterials());
  const QString checked = commonMaterial(perTarget);
  // The appearance all targets share, when they share the material too.
  QString checkedLook = perTargetLook.value(0);
  for (const QString& look : std::as_const(perTargetLook)) {
    if (look != checkedLook) {
      checkedLook.clear();
    }
  }

  QStringList entries;
  // Each entry's action with its material's name: the log's checked entry
  // is read from the actions as shown (mitcad#1149).
  QVector<QPair<QAction*, QString>> entryActions;
  const auto addEntry = [this, submenu, more, &bodies, &component, &entries, &entryActions, &checked,
                         &checkedLook, &designMaterials, &appearances](const LibraryItem& item) {
    // Without a color of its own: a body's default look (the view's role).
    const QIcon icon =
        swatchIcon(item.displayColor.isValid() ? item.displayColor : viewColor(ColorRole::Body, false));
    const QStringList offered = offeredAppearances(item, designMaterials);
    const bool isChecked = item.key() == checked;
    // As the language shows a built-in material (mitcad#1689).
    const QString shown = item.shownName();
    entries << shown;
    if (offered.size() >= 2) {
      // Its appearances (mitcad#1055): Material ▸ PLA Basic ▸ Black, the
      // targets' checked; filled and logged when it opens.
      auto* looks = new QMenu(menuLabel(shown), submenu);
      looks->menuAction()->setIcon(icon);
      looks->menuAction()->setIconVisibleInMenu(true);
      // The targets' material is checked as a plain entry is
      // (mitcad#1149), with its appearance checked inside.
      if (isChecked) {
        looks->menuAction()->setCheckable(true);
        looks->menuAction()->setChecked(true);
      }
      entryActions.append({looks->menuAction(), shown});
      const QString checkedHere = isChecked ? checkedLook : QString();
      // A library's item names and colours the appearances it carries,
      // which the design may not have yet (mitcad#1067).
      QVector<Appearance> known = appearances;
      known += appearancesOf(item.row.value(QStringLiteral("carried_appearances")).toArray());
      connect(looks, &QMenu::aboutToShow, looks,
              [this, looks, item, offered, checkedHere, bodies, component, known] {
                if (!looks->actions().isEmpty()) {
                  return;
                }
                QStringList names;
                QString checkedLookName;
                for (const QString& id : offered) {
                  const Appearance* found = mitcad::findAppearance(known, id);
                  const QString name = found != nullptr ? found->name : id;
                  auto* pick = new QAction(
                      swatchIcon(found != nullptr ? found->displayColor : viewColor(ColorRole::Body, false)),
                      menuLabel(name), looks);
                  pick->setIconVisibleInMenu(true);
                  if (id == checkedHere) {
                    pick->setCheckable(true);
                    pick->setChecked(true);
                    checkedLookName = name;
                  }
                  connect(pick, &QAction::triggered, looks, [this, item, bodies, component, id] {
                    assignMaterialFromMenu(item, bodies, component, id);
                  });
                  looks->addAction(pick);
                  names << name;
                }
                qDebug().noquote() << QStringLiteral("Context menu Material %1: %2")
                                          .arg(item.shownName(), names.join(QStringLiteral(" | ")));
                qDebug().noquote() << QStringLiteral("Context menu Material %1 checked: %2")
                                          .arg(item.shownName(), checkedLookName.isEmpty() ? QStringLiteral("none")
                                                                                           : checkedLookName);
              });
      submenu->insertMenu(more, looks);
      return;
    }
    QAction* action = new QAction(icon, menuLabel(shown), submenu);
    // The swatch is the material's look, shown in every style (also where
    // menus hide icons by default).
    action->setIconVisibleInMenu(true);
    if (isChecked) {
      action->setCheckable(true);
      action->setChecked(true);
    }
    entryActions.append({action, shown});
    connect(action, &QAction::triggered, submenu,
            [this, item, bodies, component] { assignMaterialFromMenu(item, bodies, component); });
    submenu->insertAction(more, action);
  };
  for (const int index : rows.design) {
    addEntry(items[index]);
  }
  if (!rows.recent.isEmpty()) {
    if (!entries.isEmpty()) {
      submenu->insertSeparator(more);
    }
    for (const int index : rows.recent) {
      addEntry(items[index]);
    }
  }
  if (!entries.isEmpty()) {
    submenu->insertSeparator(more);
  }
  entries << more->text();
  // A material of two or more appearances is a submenu of its own
  // (mitcad#1055), logged as "Context menu Material <name>: ..." when it
  // opens.
  qDebug().noquote() << QStringLiteral("Context menu Material: %1").arg(entries.join(QStringLiteral(" | ")));
  // The entry the menu shows checked, a plain one or a material's submenu
  // (mitcad#1149).
  QString checkedName;
  for (const auto& [action, name] : std::as_const(entryActions)) {
    if (action->isCheckable() && action->isChecked()) {
      checkedName = name;
    }
  }
  qDebug().noquote() << QStringLiteral("Context menu Material checked: %1")
                            .arg(checkedName.isEmpty() ? QStringLiteral("none") : checkedName);
  if (!rows.recent.isEmpty()) {
    QStringList recent;
    for (const int index : rows.recent) {
      recent << items[index].shownName();
    }
    qDebug().noquote() << QStringLiteral("Context menu Material recent: %1").arg(recent.join(QStringLiteral(" | ")));
  }
}

void BrowserController::assignMaterialFromMenu(const LibraryItem& item, const QStringList& bodies,
                                               const QString& component, const QString& appearance) {
  if ((bodies.isEmpty() && component.isEmpty()) || !canChange()) {
    return;
  }
  const QString project = m_host.projectFolder();
  // "PLA Basic" or "PLA Basic, Black" (mitcad#1055).
  QString picked = item.name;
  if (!appearance.isEmpty()) {
    QString lookName = appearance;
    try {
      QVector<Appearance> known = appearancesOf(m_host.model().queryArray(QStringLiteral("appearances")));
      known += appearancesOf(item.row.value(QStringLiteral("carried_appearances")).toArray());
      if (const Appearance* found = mitcad::findAppearance(known, appearance)) {
        lookName = found->name;
      }
    } catch (const std::exception&) {
      // Named by its id.
    }
    picked += QStringLiteral(", ") + lookName;
  }
  if (!component.isEmpty()) {
    // The component's default material, as Assign gives it (a library's
    // item copied in by the same undo step), with the appearance picked.
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_component_material")},
                        {QStringLiteral("component"), component}};
    nameItem(command, item, project, QStringLiteral("material"));
    if (!appearance.isEmpty()) {
      command.insert(QStringLiteral("appearance_id"), appearance);
    }
    if (m_host.runModelCommand(command)) {
      m_host.commandInputsChanged();
      rememberRecentMaterial(item.place, item.key());
      qDebug().noquote() << QStringLiteral("Material assigned from menu: %1 to component %2")
                                .arg(picked, m_snapshot.componentName(component));
    }
    return;
  }
  if (!appearance.isEmpty()) {
    // The material and the appearance picked: no question about an
    // appearance set by hand, the user chose this one.
    QJsonArray commands;
    for (const QString& body : bodies) {
      QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_body_material")},
                          {QStringLiteral("uid"), body},
                          {QStringLiteral("appearance_id"), appearance}};
      nameItem(command, item, project, QStringLiteral("material"));
      commands.append(command);
    }
    const bool done = commands.size() == 1 ? m_host.runModelCommand(commands.first().toObject())
                                           : m_host.runModelCommands(commands, tr("Material"));
    if (!done) {
      qDebug().noquote() << QStringLiteral("Material from menu refused: %1").arg(m_host.lastError());
      return;
    }
    m_host.commandInputsChanged();
    rememberRecentMaterial(item.place, item.key());
    qDebug().noquote() << QStringLiteral("Material assigned from menu: %1 to %2 bodies").arg(picked).arg(bodies.size());
    return;
  }
  QJsonArray bodyRows;
  QJsonArray designMaterials;
  try {
    bodyRows = m_host.model().queryArray(QStringLiteral("bodies"));
    designMaterials = m_host.model().queryArray(QStringLiteral("materials"));
  } catch (const std::exception&) {
    // No appearance set by hand is known: none is asked about.
  }
  const QStringList offered = materialAppearances(designFields(item, designMaterials, QJsonObject()));
  const BodyMaterialCommands assignment = bodyMaterialCommands(m_window, &item, offered, bodyRows, bodies, project);
  if (assignment.cancelled || assignment.commands.isEmpty()) {
    return;
  }
  const QJsonArray& commands = assignment.commands;
  // One undo step, named as Assign's: Set Material of Body1, or Material.
  const bool done = commands.size() == 1 ? m_host.runModelCommand(commands.first().toObject())
                                         : m_host.runModelCommands(commands, tr("Material"));
  if (!done) {
    qDebug().noquote() << QStringLiteral("Material from menu refused: %1").arg(m_host.lastError());
    return;
  }
  m_host.commandInputsChanged();
  rememberRecentMaterial(item.place, item.key());
  qDebug().noquote() << QStringLiteral("Material assigned from menu: %1 to %2 bodies").arg(item.name).arg(bodies.size());
}

} // namespace mitcad
