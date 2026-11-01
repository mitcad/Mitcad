// SPDX-License-Identifier: MIT
#include "PrintLibrary.hpp"

#include <exception>
#include <utility>

#include <QAction>
#include <QFont>
#include <QHBoxLayout>
#include <QJsonValue>
#include <QLabel>
#include <QLatin1String>
#include <QLineEdit>
#include <QMenu>
#include <QMessageBox>
#include <QObject>
#include <QPalette>
#include <QPoint>
#include <QPushButton>
#include <QRegularExpression>
#include <QSet>
#include <QSettings>
#include <QTreeWidget>
#include <QWidget>
#include <QtLogging>

#include "../framework/Appearances.hpp"
#include "../framework/Command.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Theme.hpp"
#include "PrintKeys.hpp"

namespace mitcad {
namespace {

QString text(const QJsonObject& object, const char* field) {
  return object.value(QLatin1String(field)).toString();
}

bool isBuiltInMaterial(const QString& id) {
  for (const auto& material : physicalMaterials()) {
    if (material.first == id) {
      return true;
    }
  }
  return false;
}

// The items asked about a change elsewhere in this session, by kind and
// the design's copy.
QSet<QString>& asked() {
  static QSet<QString> keys;
  return keys;
}

QString askedKey(const LibraryItem& item) { return item.kind + QLatin1Char('/') + item.designId(); }

const char* kPlaceSetting = "print/creationPlace";

} // namespace

QString LibraryItem::designId() const {
  if (!inDesign.isEmpty()) {
    return inDesign;
  }
  return place == QStringLiteral("design") || library.isEmpty() ? id : QString();
}

bool LibraryItem::saved() const { return library == QStringLiteral("user") || library == QStringLiteral("project"); }

bool LibraryItem::unsaved() const { return !readOnly && library.isEmpty(); }

QVector<LibraryItem> queryLibraryItems(const CommandContext& model, const QString& kind, const QString& project,
                                       QString* error) {
  QJsonObject query{{QStringLiteral("query"), QStringLiteral("library_items")}, {QStringLiteral("kind"), kind}};
  if (!project.isEmpty()) {
    query.insert(QStringLiteral("project"), project);
  }
  QJsonArray rows;
  try {
    const QJsonValue answer = model.query(query);
    rows = answer.isArray() ? answer.toArray() : answer.toObject().value(QStringLiteral("items")).toArray();
  } catch (const std::exception& e) {
    if (error != nullptr) {
      *error = QString::fromUtf8(e.what());
    }
    return {};
  }
  QVector<LibraryItem> items;
  for (const QJsonValue& value : std::as_const(rows)) {
    const QJsonObject row = value.toObject();
    LibraryItem item;
    item.row = row;
    item.id = text(row, "id");
    item.name = text(row, "name");
    if (item.name.isEmpty()) {
      item.name = item.id;
    }
    item.kind = text(row, "kind");
    if (item.kind.isEmpty()) {
      item.kind = kind;
    }
    item.place = text(row, "place");
    item.readOnly = row.value(QStringLiteral("read_only")).toBool(item.place == QStringLiteral("mitcad"));
    const QJsonValue copy = row.value(QStringLiteral("in_design"));
    item.inDesign = copy.isArray() ? copy.toArray().at(0).toString() : copy.toString();
    item.changed = row.value(QStringLiteral("changed")).toBool();
    item.chosen = row.value(QStringLiteral("chosen")).toBool();
    item.group = text(row, "group");
    item.materialClass = text(row, "class");
  item.classFixed = row.value(QStringLiteral("class_fixed")).toBool();
    const QJsonValue color = row.value(QStringLiteral("display_color"));
    if (color.isArray()) {
      item.displayColor = colorOf(color);
    } else if (color.isString()) {
      item.displayColor = QColor(color.toString());
    }
    // The library a command names: the row's (null for a built-in material
    // and the design's own), else its place's (Mitcad's own are the
    // built-in materials or the base library).
    item.library = text(row, "library");
    if (!row.contains(QStringLiteral("library"))) {
      if (item.place == QStringLiteral("user") || item.place == QStringLiteral("project")) {
        item.library = item.place;
      } else if (item.place == QStringLiteral("mitcad") && !row.value(QStringLiteral("built_in")).toBool() &&
                 !(item.kind == QStringLiteral("material") && isBuiltInMaterial(item.id))) {
        item.library = QStringLiteral("base");
      }
    }
    if (item.library == QStringLiteral("design")) {
      item.library.clear();
    }
    if (!item.id.isEmpty()) {
      items.append(item);
    }
  }
  return items;
}

const LibraryItem* findItem(const QVector<LibraryItem>& items, const QString& key) {
  for (const LibraryItem& item : items) {
    if (item.key() == key) {
      return &item;
    }
  }
  return nullptr;
}

const LibraryItem* findDesignCopy(const QVector<LibraryItem>& items, const QString& designId) {
  if (designId.isEmpty()) {
    return nullptr;
  }
  for (const LibraryItem& item : items) {
    if (item.designId() == designId) {
      return &item;
    }
  }
  return nullptr;
}

QString placeName(const QString& place) {
  if (place == QStringLiteral("mitcad")) {
    return QObject::tr("Built-in"); // mitcad#655: not the program's name
  }
  if (place == QStringLiteral("user")) {
    return QObject::tr("My library");
  }
  if (place == QStringLiteral("project")) {
    return QObject::tr("This project's library");
  }
  return QObject::tr("This design");
}

QString materialGroupTitle(const QString& group) {
  if (group == QStringLiteral("metals")) {
    return QObject::tr("Metals");
  }
  if (group == QStringLiteral("plastics")) {
    return QObject::tr("General Plastics");
  }
  if (group == QStringLiteral("fdm_plastics")) {
    return QObject::tr("FDM Plastics");
  }
  if (group == QStringLiteral("resins")) {
    return QObject::tr("Resins");
  }
  return QObject::tr("Other");
}

QString materialClassTitle(const QString& materialClass) {
  if (materialClass == QStringLiteral("metal")) {
    return QObject::tr("Metal");
  }
  if (materialClass == QStringLiteral("plastic")) {
    return QObject::tr("General Plastic");
  }
  if (materialClass == QStringLiteral("fdm_plastic")) {
    return QObject::tr("FDM Plastic");
  }
  if (materialClass == QStringLiteral("resin")) {
    return QObject::tr("Resin");
  }
  return QObject::tr("Other");
}

const QStringList& materialClasses() {
  // The model's order (MaterialClass::ALL, mitcad#1102).
  static const QStringList classes{QStringLiteral("metal"), QStringLiteral("plastic"), QStringLiteral("fdm_plastic"),
                                   QStringLiteral("resin"), QStringLiteral("other")};
  return classes;
}

const QStringList& materialGroups() {
  static const QStringList groups{QStringLiteral("metals"), QStringLiteral("plastics"), QStringLiteral("fdm_plastics"),
                                  QStringLiteral("resins"), QStringLiteral("other")};
  return groups;
}

void nameItem(QJsonObject& command, const LibraryItem& item, const QString& project, const QString& field) {
  if (item.library.isEmpty()) {
    command.insert(field, item.designId());
    return;
  }
  command.insert(field, item.id);
  command.insert(QStringLiteral("library"), item.library);
  if (!project.isEmpty()) {
    command.insert(QStringLiteral("project"), project);
  }
}

QJsonObject itemFields(const LibraryItem& item, const QJsonArray& designItems, const QJsonObject& library) {
  if (item.row.value(QStringLiteral("item")).isObject()) {
    return item.row.value(QStringLiteral("item")).toObject();
  }
  if (!item.library.isEmpty()) {
    for (const QJsonValue& value : library.value(QStringLiteral("items")).toArray()) {
      const QJsonObject entry = value.toObject();
      if (text(entry, "library") == item.library && text(entry, "kind") == item.kind && text(entry, "id") == item.id) {
        return entry.value(QStringLiteral("item")).toObject();
      }
    }
  }
  const QString id = item.designId();
  for (const QJsonValue& value : designItems) {
    if (text(value.toObject(), "id") == id && !id.isEmpty()) {
      return value.toObject();
    }
  }
  return item.row;
}

QJsonObject designFields(const LibraryItem& item, const QJsonArray& designItems, const QJsonObject& library) {
  // What the design prints with (mitcad#655): its copy.
  if (!item.inDesign.isEmpty()) {
    for (const QJsonValue& value : designItems) {
      if (text(value.toObject(), "id") == item.inDesign) {
        return value.toObject();
      }
    }
  }
  return itemFields(item, designItems, library);
}

QHBoxLayout* itemHeader(QLineEdit** name, QLabel** used, QLabel** saved, QWidget** builtIn, QPushButton** duplicate,
                        QPushButton** saveTo) {
  auto* row = new QHBoxLayout;
  *name = new QLineEdit;
  (*name)->setPlaceholderText(QObject::tr("Name"));
  (*name)->setAccessibleName(QObject::tr("Name"));
  QFont bold = (*name)->font();
  bold.setBold(true);
  (*name)->setFont(bold);
  // The item's states as quiet text next to its name (mitcad#655), one
  // form in both windows: the lists' mark in words, and where it is kept.
  const auto quiet = [](const QString& text) {
    auto* label = new QLabel(text);
    setRoleStyleSheet(label, ColorRole::TextSecondary);
    label->hide();
    return label;
  };
  *used = quiet(QObject::tr("● Used in this design"));
  *saved = quiet(QString()); // showSavedState
  // Built-in with its lock, small and grey, and Duplicate (mitcad#655).
  *builtIn = new QWidget;
  auto* builtInRow = new QHBoxLayout(*builtIn);
  builtInRow->setContentsMargins(0, 0, 0, 0);
  builtInRow->setSpacing(4);
  auto* lock = new QLabel;
  lock->setPixmap(themeIcon(QStringLiteral("lock")).pixmap(16, 16));
  auto* words = new QLabel(QObject::tr("Built-in"));
  setRoleStyleSheet(words, ColorRole::TextSecondary);
  // The way on beside it (mitcad#655): the user's own copy.
  *duplicate = new QPushButton(QObject::tr("Duplicate"));
  builtInRow->addWidget(lock);
  builtInRow->addWidget(words);
  builtInRow->addSpacing(6);
  builtInRow->addWidget(*duplicate);
  (*builtIn)->hide();
  *saveTo = new QPushButton;
  row->addWidget(*name, 1);
  row->addWidget(*used);
  row->addWidget(*saved);
  row->addWidget(*builtIn);
  row->addWidget(*saveTo);
  return row;
}

void showSavedState(QLabel* saved, const LibraryItem* item) {
  const bool shown = item != nullptr && item->saved();
  saved->setVisible(shown);
  if (shown) {
    saved->setText(item->library == QStringLiteral("project") ? QObject::tr("In this project's library")
                                                              : QObject::tr("In My library"));
  }
}

QString saveToText(bool inProject) {
  return inProject ? QObject::tr("Save to ▾") : QObject::tr("Save to My library");
}

void markInDesign(QTreeWidgetItem* row, bool used, const QString& tooltip) {
  QFont font = row->font(0);
  font.setBold(used);
  row->setFont(0, font);
  row->setText(1, used ? QStringLiteral("●") : QString());
  row->setToolTip(1, used ? tooltip : QString());
  row->setToolTip(0, used ? tooltip : QString());
}

void styleItemList(QTreeWidget* list) {
  QPalette palette = list->palette();
  for (const QPalette::ColorRole role : {QPalette::Highlight, QPalette::HighlightedText}) {
    palette.setColor(QPalette::Inactive, role, palette.color(QPalette::Active, role));
  }
  list->setPalette(palette);
  // The selected row at full strength, also without the keyboard (mitcad#655).
  // No focus frame round the row: it reads as a rename field.
  list->setStyleSheet(QStringLiteral("QTreeView { outline: 0; }"
                                     "QTreeView::item:selected { background: palette(highlight);"
                                     " color: palette(highlighted-text); border: none; }"));
  list->setSelectionBehavior(QAbstractItemView::SelectRows);
}

bool confirmDelete(QWidget* parent, const LibraryItem& item, const QString& logName, const QStringList& users,
                   const QString& fallback) {
  // What follows, in a few words, naming who uses it (mitcad#655); never
  // the root as a component, never a rule.
  const auto any = [&item](const char* field) { return !item.row.value(QLatin1String(field)).toArray().isEmpty(); };
  QString question;
  if (item.saved()) {
    // Saved in a library: it goes from there, the design keeps its copy.
    const QString from =
        item.library == QStringLiteral("project") ? QObject::tr("this project's library") : QObject::tr("My library");
    question = QObject::tr("Delete %1 from %2?").arg(item.name, from);
    if (any("bodies") || any("components") || any("chosen_by") || item.chosen) {
      question += QLatin1Char(' ') + QObject::tr("This design keeps using it.");
    }
  } else if (item.kind != QStringLiteral("material")) {
    question = QObject::tr("Delete %1 from this design?").arg(item.name);
    if (any("chosen_by") || item.chosen) {
      question += QLatin1Char(' ') + QObject::tr("This design then uses Slicer default.");
    }
  } else {
    question = QObject::tr("Delete %1?").arg(item.name);
    if (!users.isEmpty()) {
      const QString who = users.size() == 1   ? users.first()
                          : users.size() == 2 ? QObject::tr("%1 and %2").arg(users.at(0), users.at(1))
                                              : QObject::tr("%1 bodies").arg(users.size());
      question += QLatin1Char(' ') + (users.size() == 1 ? QObject::tr("%1 then uses %2.")
                                                        : QObject::tr("%1 then use %2."))
                                         .arg(who, fallback);
    }
  }
  qDebug().noquote() << QStringLiteral("%1 delete: %2").arg(logName, question);
  // What cannot be undone asks in a Warning dialog (mitcad#1040).
  QMessageBox box(QMessageBox::Warning, QObject::tr("Delete"), question, QMessageBox::Cancel, parent);
  QPushButton* confirm = box.addButton(QObject::tr("&Delete"), QMessageBox::DestructiveRole);
  box.setDefaultButton(QMessageBox::Cancel);
  prepareModal(&box);
  execDialog(&box);
  const bool deleted = box.clickedButton() == confirm;
  if (!deleted) {
    qDebug().noquote() << QStringLiteral("%1 delete cancelled").arg(logName);
  }
  return deleted;
}

QJsonObject askChangedElsewhere(QWidget* parent, const LibraryItem& item, const QString& logName,
                                const QString& project) {
  // Once (mitcad#655): the answer is the design's, and this session does
  // not ask about the item again either way.
  asked().insert(askedKey(item));
  const QString question =
      QObject::tr("%1 was changed in another design. Use the changes or keep this design's settings?").arg(item.name);
  qDebug().noquote() << QStringLiteral("%1 changed elsewhere: %2").arg(logName, item.name);
  QMessageBox box(QMessageBox::Question, QObject::tr("Changed in Another Design"), question, QMessageBox::NoButton,
                  parent);
  QPushButton* use = box.addButton(QObject::tr("&Use the changes"), QMessageBox::AcceptRole);
  QPushButton* keep = box.addButton(QObject::tr("&Keep this design's settings"), QMessageBox::RejectRole);
  box.setDefaultButton(keep);
  box.setEscapeButton(keep);
  prepareModal(&box);
  execDialog(&box);
  const bool take = box.clickedButton() == use;
  qDebug().noquote() << QStringLiteral("%1 changed elsewhere: %2").arg(logName, take ? QStringLiteral("use")
                                                                                      : QStringLiteral("keep"));
  QJsonObject command{{QStringLiteral("cmd"), take ? QStringLiteral("update_from_print_library")
                                                   : QStringLiteral("decline_print_library_update")},
                      {QStringLiteral("kind"), item.kind},
                      {QStringLiteral("id"), item.designId()}};
  if (!project.isEmpty()) {
    command.insert(QStringLiteral("project"), project);
  }
  return command;
}

bool changeAsked(const LibraryItem& item) { return asked().contains(askedKey(item)); }

bool usedByDesign(const LibraryItem& item) {
  for (const char* field : {"bodies", "components", "chosen_by"}) {
    if (!item.row.value(QLatin1String(field)).toArray().isEmpty()) {
      return true;
    }
  }
  return item.chosen;
}

QString choosePlace(QWidget* anchor, const QString& logName, bool inProject, const QPoint* at) {
  if (!inProject) {
    return QStringLiteral("user");
  }
  QSettings settings;
  const QString last = settings.value(QLatin1String(kPlaceSetting), QStringLiteral("user")).toString();
  QMenu menu(anchor);
  QAction* user = menu.addAction(placeName(QStringLiteral("user")));
  QAction* project = menu.addAction(placeName(QStringLiteral("project")));
  menu.setActiveAction(last == QStringLiteral("project") ? project : user);
  qDebug().noquote() << QStringLiteral("%1 menu: %2 | %3").arg(logName, user->text(), project->text());
  QAction* chosen = execMenu(&menu, at != nullptr ? *at : anchor->mapToGlobal(QPoint(0, anchor->height())));
  if (chosen == nullptr) {
    return QString();
  }
  const QString library = chosen == project ? QStringLiteral("project") : QStringLiteral("user");
  settings.setValue(QLatin1String(kPlaceSetting), library);
  return library;
}

QString designDialectOf(const CommandContext& model) {
  // The slicer the design's printer names a preset of, or whose block it
  // keeps (mitcad#655).
  try {
    const QString id = model.queryObject({{QStringLiteral("query"), QStringLiteral("print_settings")}})
                           .value(QStringLiteral("printer"))
                           .toObject()
                           .value(QStringLiteral("profile"))
                           .toObject()
                           .value(QStringLiteral("id"))
                           .toString();
    for (const QJsonValue& value : model.queryArray(QStringLiteral("print_profiles"))) {
      const QJsonObject profile = value.toObject();
      if (id.isEmpty() || profile.value(QStringLiteral("id")).toString() != id) {
        continue;
      }
      const QJsonObject presets = profile.value(QStringLiteral("values"))
                                      .toObject()
                                      .value(QStringLiteral("printer_settings_id"))
                                      .toObject();
      if (!presets.isEmpty()) {
        return presets.keys().first();
      }
      const QJsonArray raw = profile.value(QStringLiteral("raw_slicers")).toArray();
      if (!raw.isEmpty()) {
        return raw.at(0).toString();
      }
    }
  } catch (const std::exception&) {
    // Bambu Studio's.
  }
  return QStringLiteral("bambu_studio");
}

QString plainModelError(const QString& error, const PrintKeyTable& keys, const QJsonObject& command,
                        const QString& name) {
  // The model's known refusals, matched whole (mitcad#655), in plain words
  // with a next step: no paths, no ids. Anything else keeps its own words,
  // by plain names, never a guessed meaning.
  const QString library = command.value(QStringLiteral("library")).toString();
  const QString place = library == QStringLiteral("project") ? QObject::tr("the project's library")
                        : library == QStringLiteral("user")  ? QObject::tr("My library")
                                                             : QObject::tr("the library");
  const QString message = error.trimmed();
  const auto is = [&message](const char* pattern) {
    return QRegularExpression(QStringLiteral("^%1$").arg(QLatin1String(pattern))).match(message).hasMatch();
  };
  if (is("cannot (?:write|make) .+: .+")) {
    return QObject::tr("Could not save to %1. Check that its folder can be written.").arg(place);
  }
  if (is("cannot remove .+: .+")) {
    return QObject::tr("Could not delete it from %1. Check that its folder can be written.").arg(place);
  }
  if (is("there is no user library folder")) {
    return QObject::tr("My library has no folder. Check that MitCad's data folder can be written.");
  }
  if (is("the design is in no project")) {
    return QObject::tr("This design is in no project. Save it in a project first.");
  }
  if (is("the base library is MitCad's component library: save to the user's or the project's")) {
    return QObject::tr("Not changed: the item is built in.");
  }
  if (is("the (?:printer|process|material) .* came from no library")) {
    return name.isEmpty() ? QObject::tr("It is up to date.") : QObject::tr("%1 is up to date.").arg(name);
  }
  if (is("the (?:user|project|base) library has no (?:material|printer|process|profile) .*") ||
      is("there is no (?:material|profile|printer profile|process profile) .*") ||
      is("the design has no (?:printer|process|material) .*")) {
    return QObject::tr("This item was removed meanwhile. Choose another one.");
  }
  // Else the plain names, without paths and with a capital letter, when
  // short; a long one only says what was not done (mitcad#655).
  static const QRegularExpression path(QStringLiteral("(?<!\\w)(?:[A-Za-z]:)?[\\\\/][^\\s:]+"));
  QString text = keys.plainText(message);
  text.replace(path, QObject::tr("the file"));
  if (text.isEmpty() || text.size() > 80) {
    return name.isEmpty() ? QObject::tr("Could not make the change. Check the values and try again.")
                          : QObject::tr("Could not change %1. Check its values and try again.").arg(name);
  }
  text[0] = text[0].toUpper();
  return text;
}

} // namespace mitcad
