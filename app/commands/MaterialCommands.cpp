// SPDX-License-Identifier: MIT
// The Material and Appearance tools of the Solid tab's Modify group
// (mitcad#1059; app/COMMANDS.md "Material and Appearance tools"): pick the
// material or the appearance with its picker button, pick the bodies (and
// components, or faces), OK; one undo step. The fx beside the picker binds
// the value to an option of the configuration (mitcad#861's bindings): a
// row per value of the option and its result, the picker's value for every
// value without one. The Materials and Appearances windows only manage the
// libraries.
#include <algorithm>
#include <exception>
#include <functional>
#include <memory>
#include <utility>

#include <QApplication>
#include <QColor>
#include <QCursor>
#include <QHash>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QPainter>
#include <QPen>
#include <QPointer>
#include <QRectF>
#include <QStringList>
#include <QWidget>
#include <QtLogging>

#include "../browser/Bindings.hpp"
#include "../browser/DocumentHost.hpp"
#include "../browser/LibraryPicker.hpp"
#include "../framework/Appearances.hpp"
#include "../print/AppearancePicker.hpp"
#include "../print/MaterialMenuChoices.hpp"
#include "../print/PrintLibrary.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "MaterialBinding.hpp"

namespace mitcad::cmd {
namespace {

// The rows of values the panel has room for; an option with more values
// keeps the results of the others as they are.
constexpr int kRows = 16;
constexpr int kSwatch = 16;
constexpr int kTileSwatch = 40;

// A value of the picker's input: Default (none of its own), not chosen yet
// (empty), or an expression the lists do not have ("=<expression>").
QString defaultKey() { return QStringLiteral("__default"); }
QString rowKey(int row) { return QStringLiteral("row%1").arg(row); }
QString expressionKey(const QString& expression) { return QLatin1Char('=') + expression; }
bool isExpressionKey(const QString& key) { return key.startsWith(QLatin1Char('=')); }

// What the tools read of the model when they start (the libraries are read
// from their folders once, not at every refresh).
struct MaterialCache {
  QVector<LibraryItem> items; // the Material tool's
  QJsonArray designMaterials;
  QJsonArray bodies; // rows of the `bodies` query
  QVector<Appearance> appearances;
  QVector<BindingOption> options;
  QString project;
  QString rootDefault = QStringLiteral("steel"); // the root's default material's id
  QString rootDefaultName;
};

const DocumentHost* hostOf(const CommandContext& context) { return dynamic_cast<const DocumentHost*>(&context); }

void readCache(MaterialCache& cache, const CommandContext& context, bool materials) {
  cache = MaterialCache();
  if (const DocumentHost* host = hostOf(context)) {
    cache.project = host->projectFolder();
  }
  try {
    cache.bodies = context.queryArray(QStringLiteral("bodies"));
    cache.appearances = appearancesOf(context.queryArray(QStringLiteral("appearances")));
    // Both tools: the Appearance tool orders its picker by the targets'
    // material's class (mitcad#1102).
    cache.designMaterials = context.queryArray(QStringLiteral("materials"));
    if (materials) {
      cache.items = queryLibraryItems(context, QStringLiteral("material"), cache.project);
    }
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Material tools: %1").arg(QString::fromUtf8(e.what()));
  }
  QJsonObject configure;
  QJsonArray parameters;
  try {
    configure = context.queryObject({{QStringLiteral("query"), QStringLiteral("configure")}});
  } catch (const std::exception&) {
    // No configuration: the user parameters only.
  }
  try {
    parameters = context.queryArray(QStringLiteral("parameters"));
  } catch (const std::exception&) {
    // No options.
  }
  cache.options = bindingOptions(configure, parameters);
  for (const QJsonValue& value : std::as_const(cache.designMaterials)) {
    const QJsonObject material = value.toObject();
    if (material.value(QStringLiteral("components")).toArray().contains(QStringLiteral("C0"))) {
      cache.rootDefault = str(material, "id");
      cache.rootDefaultName = str(material, "name");
    }
    if (cache.rootDefaultName.isEmpty() && str(material, "id") == cache.rootDefault) {
      cache.rootDefaultName = str(material, "name");
    }
  }
  if (cache.rootDefaultName.isEmpty()) {
    cache.rootDefaultName = QStringLiteral("Steel"); // as stored; shown translated (mitcad#1689)
  }
}

QJsonObject bodyRow(const MaterialCache& cache, const QString& uid) {
  for (const QJsonValue& value : cache.bodies) {
    if (str(value.toObject(), "uid") == uid) {
      return value.toObject();
    }
  }
  return {};
}

// A rounded square of a colour (none: the default look's grey).
QImage colorSwatch(const QColor& color, int size) {
  QImage image(size, size, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter painter(&image);
  painter.setRenderHint(QPainter::Antialiasing);
  const QColor fill = color.isValid() ? color : QColor(204, 204, 204);
  painter.setBrush(fill);
  painter.setPen(QPen(fill.darker(140), 1));
  const double edge = 1.5;
  painter.drawRoundedRect(QRectF(edge, edge, size - 2 * edge, size - 2 * edge), size / 6.0, size / 6.0);
  return image;
}

// The window the pickers open over, and the host that chooses in a panel.
QWidget* windowOf(const CommandContext& context) {
  return const_cast<QWidget*>(dynamic_cast<const QWidget*>(&context));
}
DocumentHost* chooserOf(const CommandContext& context) { return const_cast<DocumentHost*>(hostOf(context)); }

// ---------------------------------------------------------------------------
// Materials

bool isBuiltIn(const LibraryItem& item) { return item.place == QStringLiteral("mitcad") && item.library.isEmpty(); }

// The design's own, a library's copied in, or a built-in one it uses.
bool inDesign(const LibraryItem& item) {
  return !item.designId().isEmpty() && (!isBuiltIn(item) || usedByDesign(item));
}

// The item a key names; Default: the root's default material.
const LibraryItem* materialItem(const MaterialCache& cache, const QString& key) {
  if (key == defaultKey()) {
    return findDesignCopy(cache.items, cache.rootDefault);
  }
  return findItem(cache.items, key);
}

QString defaultMaterialName(const MaterialCache& cache) {
  // The default's name as the language shows a built-in one (mitcad#1689).
  return QObject::tr("Default (%1)").arg(shownMaterialName(cache.rootDefault, cache.rootDefaultName));
}

QString materialName(const MaterialCache& cache, const QString& key) {
  if (key == defaultKey()) {
    return defaultMaterialName(cache);
  }
  if (isExpressionKey(key)) {
    return unquotedText(key.mid(1));
  }
  const LibraryItem* item = findItem(cache.items, key);
  return item != nullptr ? item->shownName() : key;
}

// The appearance a material gives its bodies.
QString materialLook(const MaterialCache& cache, const QString& key) {
  const LibraryItem* item = materialItem(cache, key);
  return item == nullptr ? QString()
                         : str(designFields(*item, cache.designMaterials, QJsonObject()), "appearance");
}

// The appearances a material offers (mitcad#1055), its default first.
QStringList materialLooks(const MaterialCache& cache, const QString& key) {
  QStringList looks;
  const QString first = materialLook(cache, key);
  if (!first.isEmpty()) {
    looks << first;
  }
  const LibraryItem* item = materialItem(cache, key);
  if (item != nullptr) {
    const QJsonObject fields = designFields(*item, cache.designMaterials, QJsonObject());
    for (const QJsonValue& value : fields.value(QStringLiteral("appearances")).toArray()) {
      if (!value.toString().isEmpty() && !looks.contains(value.toString())) {
        looks << value.toString();
      }
    }
  }
  return looks;
}

// The appearance chosen for the bodies when the material offers several
// (mitcad#1055: `appearance_id`); empty when it offers one or none.
QString chosenLook(const MaterialCache& cache, const CommandState& state) {
  const QString key = state.choice(QStringLiteral("material"));
  if (key == defaultKey()) {
    return QString(); // no material of their own: no appearance of one
  }
  const QStringList looks = materialLooks(cache, key);
  const QString chosen = state.choice(QStringLiteral("appearance"));
  return looks.size() > 1 && looks.contains(chosen) ? chosen : QString();
}

QString appearanceName(const MaterialCache& cache, const QString& id) {
  const Appearance* found = findAppearance(cache.appearances, id);
  return found != nullptr ? found->name : id;
}

// Where the picker lists a material: the design's first, then the
// libraries by place, named as the managers' groups (mitcad#1135).
QString materialSection(const LibraryItem& item) {
  return placeName(inDesign(item) ? QStringLiteral("design") : item.place);
}

// The materials in the picker's order: Default, the design's, then each
// library's.
QVector<const LibraryItem*> orderedMaterials(const MaterialCache& cache) {
  QVector<const LibraryItem*> ordered;
  for (const LibraryItem& item : cache.items) {
    if (inDesign(item)) {
      ordered.append(&item);
    }
  }
  QStringList places;
  for (const LibraryItem& item : cache.items) {
    if (!inDesign(item) && !places.contains(item.place)) {
      places << item.place;
    }
  }
  for (const QString& place : std::as_const(places)) {
    for (const LibraryItem& item : cache.items) {
      if (!inDesign(item) && item.place == place) {
        ordered.append(&item);
      }
    }
  }
  return ordered;
}

QVector<PickerEntry> materialEntries(const MaterialCache& cache) {
  QVector<PickerEntry> entries;
  const LibraryItem* fallback = materialItem(cache, defaultKey());
  entries.append(PickerEntry{defaultKey(), defaultMaterialName(cache), QString(),
                             colorSwatch(fallback != nullptr ? fallback->displayColor : QColor(), kSwatch * 2),
                             QObject::tr("No material of its own")});
  for (const LibraryItem* item : orderedMaterials(cache)) {
    PickerEntry entry{item->key(), item->shownName(), materialSection(*item),
                      colorSwatch(item->displayColor, kSwatch * 2), QString()};
    // A built-in material is also found by its stored English name
    // (mitcad#1689).
    if (entry.name != item->name) {
      entry.search = item->name;
    }
    entries.append(entry);
  }
  return entries;
}

// The drop-down of a row of values: As above (the picker's value), Default
// and the materials under their headings.
Choices materialRowChoices(const MaterialCache& cache) {
  Choices choices{{QString(), QObject::tr("As above")}, {defaultKey(), defaultMaterialName(cache)}};
  QString section;
  for (const LibraryItem* item : orderedMaterials(cache)) {
    const QString heading = materialSection(*item);
    if (heading != section) {
      section = heading;
      choices.append({choiceHeadingPrefix() + heading, heading});
    }
    choices.append({item->key(), item->shownName()});
  }
  return choices;
}

// A material key as a binding's result (an expression), and the library
// item to copy into the design first when it is not there yet.
QString materialResult(const MaterialCache& cache, const QString& key, const LibraryItem** copy) {
  if (key.isEmpty()) {
    return QString();
  }
  if (isExpressionKey(key)) {
    return key.mid(1);
  }
  if (key == defaultKey()) {
    return quotedText(cache.rootDefault);
  }
  const LibraryItem* item = findItem(cache.items, key);
  if (item == nullptr) {
    return QString();
  }
  if (!item->designId().isEmpty()) {
    return quotedText(item->designId());
  }
  // A library's material not in the design yet: copied in by the same undo
  // step and named by its name.
  if (copy != nullptr) {
    *copy = item;
  }
  return quotedText(item->name);
}

// A binding's result as a key: the design's material it names by id, else
// by name; else the expression as it is.
QString materialKeyOf(const MaterialCache& cache, const QString& result) {
  if (result.trimmed().isEmpty()) {
    return QString();
  }
  const QString text = unquotedText(result);
  if (const LibraryItem* item = findDesignCopy(cache.items, text)) {
    return item->key();
  }
  for (const LibraryItem& item : cache.items) {
    if (!item.designId().isEmpty() && item.name == text) {
      return item.key();
    }
  }
  return expressionKey(result.trimmed());
}

// ---------------------------------------------------------------------------
// Appearances

// The class of the targets' material (mitcad#1102): the first picked body's
// (its own, or its component's default); empty when it has none known.
QString targetsMaterialClass(const MaterialCache& cache, const CommandState& state, const CommandContext& context) {
  const QStringList inputs{QStringLiteral("bodies"), QStringLiteral("faces")};
  for (const QString& input : inputs) {
    for (const SelectionItem& item : state.items(input)) {
      QJsonObject query{{QStringLiteral("query"), QStringLiteral("body_material")},
                        {QStringLiteral("body"), item.owner}};
      if (!item.occurrence.isEmpty()) {
        query.insert(QStringLiteral("occurrence"), item.occurrence);
      }
      QString id;
      try {
        id = str(context.queryObject(query), "id");
      } catch (const std::exception&) {
        continue;
      }
      for (const QJsonValue& value : cache.designMaterials) {
        if (str(value.toObject(), "id") == id) {
          return str(value.toObject(), "class");
        }
      }
      return QString();
    }
  }
  return QString();
}

// The picker's entries: Default, then the appearances the tool gave last
// under Recent (mitcad#1685, the newest first; listed again under their
// own headings), then those of the targets' material's class (mitcad#1102,
// as a material's Add…), then This design and the library's by class
// (mitcad#1133).
QVector<PickerEntry> appearanceEntries(const MaterialCache& cache, bool withDefault, const QString& materialClass) {
  QVector<PickerEntry> entries;
  if (withDefault) {
    entries.append(PickerEntry{defaultKey(), QObject::tr("Default"), QString(), colorSwatch(QColor(), kSwatch * 2),
                               QObject::tr("No appearance of its own: its material's")});
  }
  QStringList known;
  for (const Appearance& appearance : cache.appearances) {
    known << appearance.id;
  }
  QStringList recentNames;
  for (const QString& id : recentAppearanceRows(known, recentAppearances())) {
    for (const Appearance& appearance : cache.appearances) {
      if (appearance.id == id) {
        entries.append(PickerEntry{appearance.id, appearance.name, QObject::tr("Recent"),
                                   appearanceSwatch(appearance, kSwatch * 2), QString()});
        recentNames << appearance.name;
        break;
      }
    }
  }
  qDebug().noquote() << QStringLiteral("Appearance picker recent: %1")
                            .arg(recentNames.isEmpty() ? QStringLiteral("none")
                                                       : recentNames.join(QStringLiteral(" | ")));
  const QVector<AppearanceSection> sections =
      appearanceSections(cache.appearances, materialClass, QObject::tr("This design"));
  for (const AppearanceSection& section : sections) {
    for (const Appearance* appearance : section.appearances) {
      entries.append(PickerEntry{appearance->id, appearance->name, section.title,
                                 appearanceSwatch(*appearance, kSwatch * 2), QString()});
    }
  }
  qDebug().noquote() << QStringLiteral("Appearance picker sections: %1").arg(appearanceSectionsText(sections));
  return entries;
}

// A row's choices (the fx): As above, then This design and the library's
// by class under their headings, as the picker (mitcad#1133).
Choices appearanceRowChoices(const MaterialCache& cache) {
  Choices choices{{QString(), QObject::tr("As above")}};
  const QVector<AppearanceSection> sections =
      appearanceSections(cache.appearances, QString(), QObject::tr("This design"));
  for (const AppearanceSection& section : sections) {
    choices.append({choiceHeadingPrefix() + section.title, section.title});
    for (const Appearance* appearance : section.appearances) {
      choices.append({appearance->id, appearance->name});
    }
  }
  return choices;
}

QString appearanceResult(const QString& key) {
  if (key.isEmpty() || key == defaultKey()) {
    return QString(); // an appearance's binding has no default of its own
  }
  return isExpressionKey(key) ? key.mid(1) : quotedText(key);
}

QString appearanceKeyOf(const MaterialCache& cache, const QString& result) {
  if (result.trimmed().isEmpty()) {
    return QString();
  }
  const QString text = unquotedText(result);
  for (const Appearance& appearance : cache.appearances) {
    if (appearance.id == text) {
      return appearance.id;
    }
  }
  for (const Appearance& appearance : cache.appearances) {
    if (appearance.name == text) {
      return appearance.id;
    }
  }
  return expressionKey(result.trimmed());
}

// ---------------------------------------------------------------------------
// The fx: a binding's rows in the state

// What a tool says of its value: the material or the appearance.
struct ToolKind {
  bool material = true;
  QString value; // the picker's input
  QString log;   // the log's and the picker's name
  QString targets; // the selection input of bodies
};

QString fxKey(const ToolKind& kind) { return kind.value + QStringLiteral("_fx"); }

bool varies(const ToolKind& kind, const CommandState& state) {
  return state.checked(fxKey(kind)) && !state.choice(QStringLiteral("varies")).isEmpty();
}

// The rows of values shown (without Other, whose result is the picker's).
BindingTable shownTable(const CommandState& state) {
  return bindingTableOf(state.object(QStringLiteral("binding")));
}

QString keyOfResult(const ToolKind& kind, const MaterialCache& cache, const QString& result) {
  return kind.material ? materialKeyOf(cache, result) : appearanceKeyOf(cache, result);
}

// Shows `mapping` (null: none yet, every value as the picker's) of option
// `name` in the rows: the values in the rows, their results in the rows'
// drop-downs, the default (Other's, a closed list's kept one) in the
// picker.
void showBinding(const ToolKind& kind, const MaterialCache& cache, const QString& name, const OptionMapping* mapping,
                 CommandState& state) {
  const BindingTable full = bindingTable(name, findBindingOption(cache.options, name), mapping, QString());
  BindingTable shown;
  shown.option = name;
  QString otherwise = full.keptOtherwise;
  for (const BindingRow& row : full.rows) {
    if (row.other) {
      otherwise = row.result;
    } else {
      shown.rows.append(row);
    }
  }
  state.setObject(QStringLiteral("binding"), bindingTableJson(shown));
  state.setChoice(QStringLiteral("varies"), name);
  for (int i = 0; i < kRows; ++i) {
    state.setChoice(rowKey(i), i < shown.rows.size() ? keyOfResult(kind, cache, shown.rows[i].result) : QString());
  }
  if (mapping != nullptr && !otherwise.isEmpty()) {
    state.setChoice(kind.value, keyOfResult(kind, cache, otherwise));
  }
}

// The expression of the rows and the picker; `copies` gets the library
// materials not in the design yet that it names.
QString bindingOf(const ToolKind& kind, const MaterialCache& cache, const CommandState& state,
                  QVector<const LibraryItem*>* copies) {
  BindingTable table = shownTable(state);
  const auto resultOf = [&kind, &cache, copies](const QString& key) {
    const LibraryItem* copy = nullptr;
    const QString result = kind.material ? materialResult(cache, key, &copy) : appearanceResult(key);
    if (copy != nullptr && copies != nullptr && !copies->contains(copy)) {
      copies->append(copy);
    }
    return result;
  };
  for (int i = 0; i < table.rows.size(); ++i) {
    // Rows beyond the panel's keep their results.
    if (i < kRows) {
      table.rows[i].result = resultOf(state.choice(rowKey(i)));
    }
  }
  table.keptOtherwise = resultOf(state.choice(kind.value));
  return bindingExpression(table);
}

// The expression a target follows: a body's material or appearance, a
// face's appearance (by the name it was given, or one of the faces that
// name finds now); empty when it has a value set by hand.
QString targetExpression(const ToolKind& kind, const MaterialCache& cache, const SelectionItem& item) {
  const QJsonObject body = bodyRow(cache, item.owner);
  if (kind.material) {
    return str(body, "material_expr");
  }
  if (item.kind != SelectKind::Face) {
    return str(body, "appearance_expr");
  }
  QString stored = item.name;
  for (const QJsonValue& entry : body.value(QStringLiteral("face_appearances")).toArray()) {
    const QJsonObject look = entry.toObject();
    bool named = str(look, "face") == item.name;
    for (const QJsonValue& face : look.value(QStringLiteral("faces")).toArray()) {
      named = named || face.toString() == item.name;
    }
    if (named) {
      stored = str(look, "face");
    }
  }
  return body.value(QStringLiteral("face_appearance_exprs")).toObject().value(stored).toString();
}

// The first target that can follow an expression (a body, a face).
const SelectionItem* firstBound(const ToolKind& kind, const CommandState& state) {
  QStringList inputs{kind.targets};
  if (!kind.material) {
    inputs << QStringLiteral("faces"); // the Material tool has no faces
  }
  for (const QString& input : std::as_const(inputs)) {
    for (const SelectionItem& item : state.items(input)) {
      if (item.kind == SelectKind::Body || item.kind == SelectKind::Face) {
        return &item;
      }
    }
  }
  return nullptr;
}

// After the targets changed, unless the user changed the fx's inputs: the
// binding the first target follows, shown in the rows (mitcad#1059: one
// made earlier, in the windows of mitcad#861, shows in the tool). When it
// follows none, a binding shown from an earlier target goes, the fx off.
void loadBinding(const ToolKind& kind, const MaterialCache& cache, CommandState& state) {
  const QJsonObject now = state.object(QStringLiteral("binding"));
  if (now.value(QStringLiteral("touched")).toBool()) {
    return;
  }
  const SelectionItem* first = firstBound(kind, state);
  const QString expression = first != nullptr ? targetExpression(kind, cache, *first) : QString();
  OptionMapping mapping;
  if (!expression.isEmpty() && OptionMapping::parse(expression, &mapping)) {
    state.setChecked(fxKey(kind), true);
    showBinding(kind, cache, mapping.option, &mapping, state);
    QJsonObject table = state.object(QStringLiteral("binding"));
    table.insert(QStringLiteral("loaded"), true);
    state.setObject(QStringLiteral("binding"), table);
    qDebug().noquote() << QStringLiteral("%1: the targets follow %2").arg(kind.log, expression);
    return;
  }
  if (now.value(QStringLiteral("loaded")).toBool()) {
    state.setChecked(fxKey(kind), false);
    state.setChoice(QStringLiteral("varies"), QString());
    state.setObject(QStringLiteral("binding"), QJsonObject());
  }
}

// The fx's inputs: the fx button beside the picker, Varies With and a row
// per value.
QVector<InputDef> bindingInputs(const ToolKind& kind, const std::shared_ptr<MaterialCache>& cache) {
  QVector<InputDef> inputs;
  const QString fx = fxKey(kind);
  inputs.append(checkInput(fx, QObject::tr("Varies with an option"), false)
                    .withFxToggle()
                    .withBesidePrevious()
                    .withTooltip(kind.material ? QObject::tr("A material for each value of an option")
                                               : QObject::tr("An appearance for each value of an option")));
  const auto shown = [fx](const CommandState& state) { return state.checked(fx); };
  inputs.append(
      choiceInput(QStringLiteral("varies"), QObject::tr("Varies With"), {}, QString())
          .withTooltip(QObject::tr("The option whose values choose it; None: one for all"))
          .withChoices([cache](const CommandState& state) {
            Choices choices{{QString(), QObject::tr("None")}};
            for (const BindingOption& option : cache->options) {
              choices.append({option.name, option.name});
            }
            const QString now = state.choice(QStringLiteral("varies"));
            if (!now.isEmpty() && findBindingOption(cache->options, now) == nullptr) {
              choices.append({now, now}); // an option the design no longer lists
            }
            return choices;
          })
          .withVisible(shown)
          .withRowNote([cache](const CommandState&, const CommandContext&) {
            return cache->options.isEmpty()
                       ? QObject::tr("The design has no options yet; Configurations… adds them.")
                       : QString();
          })
          .withOnChange([kind, cache](CommandState& state, const CommandContext&) {
            // The table of the option chosen: the first target's rows when it
            // follows that option, else every value as the picker's.
            const QString chosen = state.choice(QStringLiteral("varies"));
            const SelectionItem* first = firstBound(kind, state);
            const QString expression = first != nullptr ? targetExpression(kind, *cache, *first) : QString();
            OptionMapping mapping;
            const bool follows = !expression.isEmpty() && OptionMapping::parse(expression, &mapping) &&
                                 mapping.option == chosen;
            if (chosen.isEmpty()) {
              state.setObject(QStringLiteral("binding"), QJsonObject{{QStringLiteral("touched"), true}});
            } else {
              showBinding(kind, *cache, chosen, follows ? &mapping : nullptr, state);
              QJsonObject table = state.object(QStringLiteral("binding"));
              table.insert(QStringLiteral("touched"), true);
              state.setObject(QStringLiteral("binding"), table);
            }
            qDebug().noquote() << QStringLiteral("%1 varies with %2: %3 rows")
                                      .arg(kind.log, chosen.isEmpty() ? QStringLiteral("None") : chosen)
                                      .arg(shownTable(state).rows.size());
          }));
  for (int i = 0; i < kRows; ++i) {
    InputDef row = choiceInput(rowKey(i), QString(), {}, QString());
    row.choiceImageSize = kSwatch;
    row.withTooltip(kind.material ? QObject::tr("The material for this value; As above: the one chosen above")
                                  : QObject::tr("The appearance for this value; As above: the one chosen above"))
        .withLabel([i, cache](const CommandState& state) {
          const BindingTable table = shownTable(state);
          if (i >= table.rows.size()) {
            return QString();
          }
          const bool now = isCurrentRow(table, findBindingOption(cache->options, table.option), i);
          return now ? QObject::tr("%1 (now)").arg(table.rows[i].shown) : table.rows[i].shown;
        })
        .withChoices([kind, cache](const CommandState&) {
          return kind.material ? materialRowChoices(*cache) : appearanceRowChoices(*cache);
        })
        .withChoiceImages([kind, cache](const QString& key) {
          if (key.isEmpty() || key.startsWith(choiceHeadingPrefix()) || isExpressionKey(key)) {
            return QImage();
          }
          if (!kind.material) {
            const Appearance* found = findAppearance(cache->appearances, key);
            return found != nullptr ? appearanceSwatch(*found, kSwatch) : QImage();
          }
          const LibraryItem* item = materialItem(*cache, key);
          return colorSwatch(item != nullptr ? item->displayColor : QColor(), kSwatch);
        })
        .withVisible([fx, i](const CommandState& state) {
          return state.checked(fx) && !state.choice(QStringLiteral("varies")).isEmpty() &&
                 i < shownTable(state).rows.size();
        })
        .withOnChange([](CommandState& state, const CommandContext&) {
          QJsonObject table = state.object(QStringLiteral("binding"));
          table.insert(QStringLiteral("touched"), true);
          state.setObject(QStringLiteral("binding"), table);
        });
    inputs.append(row);
  }
  return inputs;
}

// The picker's tile: the swatch, the name and where it comes from; with the
// fx, what it is for.
ChoiceTile valueTile(const ToolKind& kind, const MaterialCache& cache, const CommandState& state) {
  const QString key = state.choice(kind.value);
  ChoiceTile tile;
  if (key.isEmpty()) {
    tile.title = kind.material ? QObject::tr("Choose a material…") : QObject::tr("Choose an appearance…");
    tile.image = colorSwatch(QColor(Qt::transparent), kTileSwatch);
  } else if (kind.material) {
    const LibraryItem* item = materialItem(cache, key);
    tile.title = materialName(cache, key);
    tile.image = colorSwatch(item != nullptr ? item->displayColor : QColor(), kTileSwatch);
    tile.detail = key == defaultKey() ? QObject::tr("No material of its own")
                  : item != nullptr   ? materialSection(*item)
                                      : QString();
  } else {
    const Appearance* found = key == defaultKey() ? nullptr : findAppearance(cache.appearances, key);
    tile.title = key == defaultKey() ? QObject::tr("Default") : (found != nullptr ? found->name : unquotedText(key));
    tile.image = found != nullptr ? appearanceSwatch(*found, kTileSwatch) : colorSwatch(QColor(), kTileSwatch);
    tile.detail = key == defaultKey() ? QObject::tr("No appearance of its own")
                  : found != nullptr  ? (found->library ? QObject::tr("Library") : QObject::tr("This design"))
                                      : QString();
  }
  if (varies(kind, state)) {
    tile.detail = QObject::tr("For the values set to As above");
  }
  return tile;
}

// The keys and names the tool's picker offers: the materials or the
// appearances.
QVector<std::pair<QString, QString>> offeredNow(const ToolKind& kind, const MaterialCache& cache) {
  QVector<std::pair<QString, QString>> offered;
  if (kind.material) {
    for (const LibraryItem& item : cache.items) {
      offered.append({item.key(), item.name});
    }
  } else {
    for (const Appearance& appearance : cache.appearances) {
      offered.append({appearance.id, appearance.name});
    }
  }
  return offered;
}

// What a window the tool opened does when it closes (Material Library…,
// mitcad#1250; Edit Materials… and Edit Appearances… of the picker,
// mitcad#1415): the tool reads its materials or appearances again, so that
// one made or changed there is offered, logged "Material tool: materials
// read again, 12 offered, new: Bench PETG" ("Appearance tool: appearances
// read again, …"); the panel shows them, its inputs (the picked bodies
// among them) as they were.
std::function<void()> readAgainWhenClosed(const ToolKind& kind, const std::shared_ptr<MaterialCache>& cache,
                                          const CommandContext& model) {
  DocumentHost* host = chooserOf(model);
  const QPointer<QWidget> guard(windowOf(model));
  const CommandContext* context = &model;
  return [kind, cache, host, guard, context] {
    if (guard.isNull() || host == nullptr) {
      return;
    }
    // Read when no job computes the model: a query refused then would
    // leave the picker empty.
    host->whenIdle(guard.data(), [kind, cache, host, context] {
      QStringList before;
      for (const auto& item : offeredNow(kind, *cache)) {
        before << item.first;
      }
      readCache(*cache, *context, kind.material);
      const QVector<std::pair<QString, QString>> after = offeredNow(kind, *cache);
      QStringList added;
      for (const auto& item : after) {
        if (!before.contains(item.first)) {
          added << item.second;
        }
      }
      const QString news =
          added.isEmpty() ? QString() : QStringLiteral(", new: %1").arg(added.join(QStringLiteral(", ")));
      qDebug().noquote() << QStringLiteral("%1: %2 read again, %3 offered%4")
                                .arg(kind.log, kind.material ? QStringLiteral("materials") : QStringLiteral("appearances"))
                                .arg(after.size())
                                .arg(news);
      // The panel shows them (CommandSession::refreshInputs); its inputs,
      // the picked bodies among them, stay as they were.
      host->commandInputsChanged();
    });
  };
}

// The picker's last entry (mitcad#1415): below a line, Edit Materials… or
// Edit Appearances…, as the material appearance picker and 3D Print's
// Material Picker end.
QString editKey() { return QStringLiteral("__edit"); }

PickerEntry editEntry(const ToolKind& kind) {
  PickerEntry entry{editKey(), kind.material ? QObject::tr("Edit Materials…") : QObject::tr("Edit Appearances…"),
                    QString(), QImage(),
                    kind.material ? QObject::tr("New materials and changes in the Materials window, offered here "
                                                "once it closes")
                                  : QObject::tr("New appearances and changes in the Appearances window, offered here "
                                                "once it closes")};
  entry.lineBefore = true;
  return entry;
}

// Edit Materials… or Edit Appearances… chosen: the window on the picker's
// current material or appearance while the panel stays open with its
// targets; the tool reads its lists again when the window closes.
void editInWindow(const ToolKind& kind, const std::shared_ptr<MaterialCache>& cache, const QString& current,
                  const CommandContext& model) {
  DocumentHost* host = chooserOf(model);
  if (host == nullptr || windowOf(model) == nullptr) {
    return;
  }
  // Default and an expression are none of the window's.
  const bool own = !current.isEmpty() && current != defaultKey() && !isExpressionKey(current);
  const QString key = own ? current : QString();
  const QString name = !own ? QStringLiteral("nothing")
                       : kind.material ? materialName(*cache, key)
                                       : appearanceName(*cache, key);
  qDebug().noquote() << QStringLiteral("%1: %2 on %3")
                            .arg(kind.log,
                                 kind.material ? QStringLiteral("Edit Materials…") : QStringLiteral("Edit Appearances…"),
                                 name);
  if (kind.material) {
    host->editMaterialsOn(key, readAgainWhenClosed(kind, cache, model));
  } else {
    host->editAppearancesOn(key, readAgainWhenClosed(kind, cache, model));
  }
}

// Opens the picker under the pointer (the AppKit frontend's popover under
// the tile, mitcad#1066); its choice goes to the panel's input, its last
// entry opens the window (mitcad#1415).
void openPicker(const ToolKind& kind, const QString& command, QVector<PickerEntry> entries,
                const std::shared_ptr<MaterialCache>& cache, const CommandState& state, const CommandContext& context) {
  QWidget* window = windowOf(context);
  DocumentHost* host = chooserOf(context);
  const QString input = kind.value;
  const QString current = state.choice(input);
  const CommandContext* model = &context;
  entries.append(editEntry(kind));
  LibraryPicker::open(
      window, kind.material ? QObject::tr("Material Picker") : QObject::tr("Appearance Picker"),
      kind.material ? QStringLiteral("Material picker") : QStringLiteral("Appearance picker"), entries, current,
      QCursor::pos(),
      [host, command, input, kind, cache, current, model](const QString& value) {
        if (value == editKey()) {
          editInWindow(kind, cache, current, *model);
          return;
        }
        if (host == nullptr || !host->chooseInCommand(command, input, value)) {
          qInfo().noquote() << QStringLiteral("%1: the pick was not taken").arg(command);
        }
      },
      QStringLiteral("input_") + input);
}

// What the targets have now, for the row under them: "3 bodies: PLA Basic,
// 1 body: PETG".
QString materialsNow(const MaterialCache& cache, const CommandState& state, const CommandContext& context) {
  QVector<TargetState> targets;
  QStringList seen;
  for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
    const QString key = (item.kind == SelectKind::Component ? QStringLiteral("c:") : QStringLiteral("b:")) + item.owner;
    if (seen.contains(key)) {
      continue;
    }
    seen << key;
    TargetState target;
    if (item.kind == SelectKind::Component) {
      target.component = true;
      target.what = QObject::tr("Default");
      for (const QJsonValue& value : cache.designMaterials) {
        if (value.toObject().value(QStringLiteral("components")).toArray().contains(item.owner)) {
          target.what = shownMaterialName(str(value.toObject(), "id"), str(value.toObject(), "name"));
        }
      }
    } else {
      const QString expression = str(bodyRow(cache, item.owner), "material_expr");
      OptionMapping mapping;
      if (!expression.isEmpty()) {
        target.what = OptionMapping::parse(expression, &mapping) ? QObject::tr("varies with %1").arg(mapping.option)
                                                                  : QObject::tr("follows an expression");
      } else {
        QJsonObject query{{QStringLiteral("query"), QStringLiteral("body_material")}, {QStringLiteral("body"), item.owner}};
        if (!item.occurrence.isEmpty()) {
          query.insert(QStringLiteral("occurrence"), item.occurrence);
        }
        try {
          const QJsonObject made = context.queryObject(query);
          target.what = shownMaterialName(str(made, "id"), str(made, "name"));
          if (target.what.isEmpty()) {
            target.what = str(made, "id");
          }
        } catch (const std::exception&) {
          target.what = QObject::tr("unknown");
        }
      }
    }
    targets.append(target);
  }
  return targetsSummary(targets);
}

// The bodies whose appearance was set by hand and is not one the material
// offers.
QStringList handSetLooks(const MaterialCache& cache, const CommandState& state) {
  // Only a hand-set appearance the material does not offer is replaced
  // (mitcad#1067, as the context menu's asksToReplaceAppearance).
  const QStringList looks = materialLooks(cache, state.choice(QStringLiteral("material")));
  QStringList names;
  for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
    if (item.kind != SelectKind::Body) {
      continue;
    }
    const QJsonObject body = bodyRow(cache, item.owner);
    if (body.value(QStringLiteral("appearance_set_by_hand")).toBool() && !looks.contains(str(body, "appearance")) &&
        !names.contains(str(body, "name"))) {
      names << str(body, "name");
    }
  }
  return names;
}

// The bodies and the components picked, each once.
QStringList ownersOf(const Selection& items, SelectKind kind) {
  QStringList owners;
  for (const SelectionItem& item : items) {
    if (item.kind == kind && !owners.contains(item.owner)) {
      owners << item.owner;
    }
  }
  return owners;
}

// The material the targets share: their own (a body's, a component's
// default); Default when none has one; empty when they differ.
QString sharedMaterial(const MaterialCache& cache, const CommandState& state) {
  QStringList own;
  for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
    QString id;
    if (item.kind == SelectKind::Component) {
      for (const QJsonValue& value : cache.designMaterials) {
        if (value.toObject().value(QStringLiteral("components")).toArray().contains(item.owner)) {
          id = str(value.toObject(), "id");
        }
      }
    } else {
      id = str(bodyRow(cache, item.owner), "material");
    }
    if (!own.contains(id)) {
      own << id;
    }
  }
  if (own.size() != 1) {
    return QString();
  }
  if (own.first().isEmpty()) {
    return defaultKey();
  }
  const LibraryItem* item = findDesignCopy(cache.items, own.first());
  return item != nullptr ? item->key() : QString();
}

// The appearance the targets share (a face's own, else its body's).
QString sharedAppearance(const MaterialCache& cache, const CommandState& state) {
  QStringList looks;
  const QStringList inputs{QStringLiteral("bodies"), QStringLiteral("faces")};
  for (const QString& input : inputs) {
    for (const SelectionItem& item : state.items(input)) {
      const QJsonObject body = bodyRow(cache, item.owner);
      QString look = str(body, "appearance");
      if (item.kind == SelectKind::Face) {
        for (const QJsonValue& entry : body.value(QStringLiteral("face_appearances")).toArray()) {
          const QJsonObject faceLook = entry.toObject();
          bool named = str(faceLook, "face") == item.name;
          for (const QJsonValue& face : faceLook.value(QStringLiteral("faces")).toArray()) {
            named = named || face.toString() == item.name;
          }
          if (named) {
            look = str(faceLook, "appearance");
          }
        }
      }
      if (!looks.contains(look)) {
        looks << look;
      }
    }
  }
  if (looks.size() != 1) {
    return QString();
  }
  return looks.first().isEmpty() ? defaultKey() : looks.first();
}

} // namespace

// ---------------------------------------------------------------------------
// The Material tool (mitcad#1059)

CommandDef materialCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.material");
  def.name = QObject::tr("Material");
  def.icon = QStringLiteral("material");
  def.tooltip = QObject::tr("Gives bodies, or a component's default, a material");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("material"), QStringLiteral("physical material"), QStringLiteral("density"),
                  QStringLiteral("mass"),     QStringLiteral("filament"),          QStringLiteral("assign")};
  def.modelCommand = true;
  const auto cache = std::make_shared<MaterialCache>();
  const ToolKind kind{true, QStringLiteral("material"), QStringLiteral("Material tool"), QStringLiteral("bodies")};

  InputDef material = choiceInput(QStringLiteral("material"), QObject::tr("Material"), {}, QString());
  material.withTooltip(QObject::tr("The material: the design's or a library's"))
      .withChoices([cache](const CommandState&) {
        Choices choices{{defaultKey(), defaultMaterialName(*cache)}};
        for (const LibraryItem& item : cache->items) {
          choices.append({item.key(), item.shownName()});
        }
        return choices;
      })
      .withChoiceTile([cache, kind](const CommandState& state) { return valueTile(kind, *cache, state); })
      .withButton(QObject::tr("Choose…"), QObject::tr("Choose the material"),
                  [cache, kind](const CommandState& state, const CommandContext& model) {
                    // Read again: a window may have added one (mitcad#1059).
                    cache->items = queryLibraryItems(model, QStringLiteral("material"), cache->project);
                    openPicker(kind, QStringLiteral("solid.material"), materialEntries(*cache), cache, state, model);
                  })
      .withOnChange([cache](CommandState& state, const CommandContext&) {
        // Its default appearance with it; Appearance offers its others.
        state.setChoice(QStringLiteral("appearance"), materialLook(*cache, state.choice(QStringLiteral("material"))));
      });

  InputDef appearance = choiceInput(QStringLiteral("appearance"), QObject::tr("Appearance"), {}, QString());
  appearance.choiceImageSize = kSwatch;
  // One of the material's appearances (mitcad#1055): its default first,
  // a choice when it offers several (as Material ▸ material ▸ appearance).
  appearance.withTooltip(QObject::tr("The material's appearance the bodies get"))
      .withChoices([cache](const CommandState& state) {
        const QStringList looks = materialLooks(*cache, state.choice(QStringLiteral("material")));
        if (looks.isEmpty()) {
          return Choices{{QString(), QObject::tr("Default")}};
        }
        Choices choices;
        for (const QString& look : looks) {
          choices.append({look, appearanceName(*cache, look)});
        }
        return choices;
      })
      .withChoiceImages([cache](const QString& id) {
        const Appearance* found = findAppearance(cache->appearances, id);
        return found != nullptr ? appearanceSwatch(*found, kSwatch) : QImage();
      })
      .withEnabled([cache](const CommandState& state) {
        const QString key = state.choice(QStringLiteral("material"));
        return key != defaultKey() && materialLooks(*cache, key).size() > 1;
      })
      .withVisible([kind](const CommandState& state) { return !varies(kind, state); });

  // Bodies and components (a component's default material), picked in the
  // view or the Browser; a face picked before stands for its body.
  InputDef bodies =
      selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies"), SelectKind::Body | SelectKind::Component, 1, 0);
  bodies.withTooltip(QObject::tr("Bodies, or components for their default material"))
      .withAlsoTakes(SelectKind::Face)
      .withConvert([](const SelectionItem& item) {
        if (item.kind != SelectKind::Face) {
          return item;
        }
        SelectionItem body{SelectKind::Body, item.owner, QString(), QString()};
        body.occurrence = item.occurrence;
        return body;
      })
      .withRowNote([cache](const CommandState& state, const CommandContext& model) {
        return materialsNow(*cache, state, model);
      })
      .withOnChange([cache, kind](CommandState& state, const CommandContext&) {
        loadBinding(kind, *cache, state);
        if (state.choice(QStringLiteral("material")).isEmpty()) {
          state.setChoice(QStringLiteral("material"), sharedMaterial(*cache, state));
          state.setChoice(QStringLiteral("appearance"),
                          materialLook(*cache, state.choice(QStringLiteral("material"))));
        }
      });

  // Material Library… under the picker (mitcad#1250, GitHub #132): the
  // Materials manager on My library while the panel stays open; when it
  // closes, the materials are read again so that one made there is
  // offered, the picked bodies kept.
  InputDef library = noteInput(
      QStringLiteral("library"),
      [](const CommandState&) { return InputNote{QString(), {QObject::tr("Material Library…")}, {}}; },
      [cache, kind](int, const CommandState&, const CommandContext& model) {
        DocumentHost* host = chooserOf(model);
        if (host == nullptr || windowOf(model) == nullptr) {
          return;
        }
        qDebug().noquote() << QStringLiteral("Material tool: Material Library opened");
        host->openMaterialLibrary(readAgainWhenClosed(kind, cache, model));
      });
  library.withFieldColumn();

  def.inputs.clear();
  def.inputs << material;
  // The fx stays beside the picker; Material Library… under them.
  const QVector<InputDef> binding = bindingInputs(kind, cache);
  def.inputs << binding.first() << library;
  def.inputs << binding.mid(1);
  def.inputs << appearance << bodies;
  // A body's appearance set by hand (a paint) stays when this is on;
  // offered only while a picked body has one the material would replace.
  def.inputs << checkInput(QStringLiteral("keep_appearance"), QObject::tr("Keep appearances set by hand"), false)
                    .withTooltip(QObject::tr("Bodies painted by hand keep their appearance"))
                    .withVisible([cache, kind](const CommandState& state) {
                      // Not with an appearance chosen among several: that
                      // one is what the bodies get.
                      return !varies(kind, state) && chosenLook(*cache, state).isEmpty() &&
                             !handSetLooks(*cache, state).isEmpty();
                    })
                    .withRowNote([cache](const CommandState& state, const CommandContext&) {
                      const QStringList names = handSetLooks(*cache, state);
                      return names.isEmpty() ? QString()
                                             : QObject::tr("Set by hand: %1").arg(names.join(QStringLiteral(", ")));
                    });
  def.enabled = [&context] { return hasBodies(context); };
  def.init = [cache, kind](CommandState& state, const CommandContext& model) {
    readCache(*cache, model, true);
    state.setObject(QStringLiteral("binding"), QJsonObject());
    loadBinding(kind, *cache, state);
    // What the targets share, so that nothing changes until it does.
    if (state.choice(QStringLiteral("material")).isEmpty()) {
      state.setChoice(QStringLiteral("material"), sharedMaterial(*cache, state));
    }
    state.setChoice(QStringLiteral("appearance"), materialLook(*cache, state.choice(QStringLiteral("material"))));
  };
  def.build = [cache, kind](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& picked = state.items(QStringLiteral("bodies"));
    const QStringList bodyUids = ownersOf(picked, SelectKind::Body);
    const QStringList components = ownersOf(picked, SelectKind::Component);
    if (bodyUids.isEmpty() && components.isEmpty()) {
      return Built::failure(QObject::tr("Pick bodies or components."), QStringLiteral("bodies"));
    }
    QJsonArray commands;
    if (varies(kind, state)) {
      if (!components.isEmpty()) {
        return Built::failure(QObject::tr("A component's default material cannot vary with an option: "
                                          "pick its bodies."),
                              QStringLiteral("bodies"));
      }
      QVector<const LibraryItem*> copies;
      const QString expression = bindingOf(kind, *cache, state, &copies);
      if (expression.isEmpty()) {
        return Built::failure(QObject::tr("Choose a material for a value, or above."), QStringLiteral("material"));
      }
      // A library's material is copied into the design first, in the same
      // undo step; the expression names it by its name.
      for (const LibraryItem* copy : std::as_const(copies)) {
        QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_body_material")},
                            {QStringLiteral("uid"), bodyUids.first()}};
        nameItem(command, *copy, cache->project, QStringLiteral("material"));
        commands.append(command);
      }
      for (const QString& uid : bodyUids) {
        commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_body_material")},
                                    {QStringLiteral("uid"), uid},
                                    {QStringLiteral("expression"), expression}});
      }
    } else {
      const QString key = state.choice(QStringLiteral("material"));
      if (key.isEmpty()) {
        return Built::failure(QObject::tr("Choose a material."), QStringLiteral("material"));
      }
      const LibraryItem* item = key == defaultKey() ? nullptr : findItem(cache->items, key);
      if (key != defaultKey() && item == nullptr) {
        return Built::failure(QObject::tr("Choose a material."), QStringLiteral("material"));
      }
      const auto named = [&item, &cache](QJsonObject command) {
        if (item == nullptr) {
          command.insert(QStringLiteral("material"), QJsonValue());
        } else {
          // A library's material is copied into the design in the same undo
          // step (the command's `library`).
          nameItem(command, *item, cache->project, QStringLiteral("material"));
        }
        return command;
      };
      // The material's appearance replaces the bodies'; with Keep
      // appearances set by hand, the model keeps those (commands.md,
      // Materials).
      // An appearance chosen among the material's (mitcad#1055) goes as
      // `appearance_id`, in the place of `appearance`.
      const QString look = chosenLook(*cache, state);
      const bool keep = look.isEmpty() && state.checked(QStringLiteral("keep_appearance")) &&
                        !handSetLooks(*cache, state).isEmpty();
      for (const QString& uid : bodyUids) {
        QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_body_material")}, {QStringLiteral("uid"), uid}};
        if (!look.isEmpty()) {
          command.insert(QStringLiteral("appearance_id"), look);
        } else if (!keep) {
          command.insert(QStringLiteral("appearance"), QStringLiteral("replace"));
        }
        commands.append(named(command));
      }
      for (const QString& component : components) {
        QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_component_material")},
                            {QStringLiteral("component"), component}};
        if (!look.isEmpty() && item != nullptr) {
          command.insert(QStringLiteral("appearance_id"), look);
        }
        commands.append(named(command));
      }
    }
    result.def = {{QStringLiteral("commands"), commands}, {QStringLiteral("label"), QObject::tr("Material")}};
    return result;
  };
  // After OK (once): the log lines for tests, and a library's material
  // remembered for the context menus' Material ▸ (mitcad#1046).
  def.describe = [cache, kind](const CommandState& state, const CommandContext& model) {
    const Selection& picked = state.items(QStringLiteral("bodies"));
    const QStringList bodyUids = ownersOf(picked, SelectKind::Body);
    const QStringList components = ownersOf(picked, SelectKind::Component);
    QStringList lines;
    if (varies(kind, state)) {
      lines << QStringLiteral("Material tool: varies with %1 to %2 bodies: %3")
                   .arg(state.choice(QStringLiteral("varies")))
                   .arg(bodyUids.size())
                   .arg(bindingOf(kind, *cache, state, nullptr));
      return lines.join(QLatin1Char('\n'));
    }
    const QString key = state.choice(QStringLiteral("material"));
    const LibraryItem* item = key == defaultKey() ? nullptr : findItem(cache->items, key);
    const QString chosen = chosenLook(*cache, state);
    const QString look = chosen.isEmpty() ? materialLook(*cache, key) : chosen;
    const bool keep = chosen.isEmpty() && state.checked(QStringLiteral("keep_appearance")) &&
                      !handSetLooks(*cache, state).isEmpty();
    lines << QStringLiteral("Material tool: %1/%2 to %3 bodies")
                 .arg(materialName(*cache, key), look.isEmpty() ? QObject::tr("Default") : appearanceName(*cache, look))
                 .arg(bodyUids.size());
    const QString id = item == nullptr ? QStringLiteral("default")
                                       : (item->designId().isEmpty() ? item->id : item->designId());
    if (!bodyUids.isEmpty()) {
      lines << QStringLiteral("Material %1 set for %2 body(ies), appearance %3")
                   .arg(id)
                   .arg(bodyUids.size())
                   .arg(keep ? QStringLiteral("keep") : QStringLiteral("replace"));
    }
    for (const QString& component : components) {
      // The component by its name, as the windows logged it (Lid, Root).
      lines << QStringLiteral("Default material of %1: %2")
                   .arg(componentName(component, model), item == nullptr ? QStringLiteral("none") : id);
    }
    if (item != nullptr) {
      rememberRecentMaterial(item->place, item->key());
    }
    return lines.join(QLatin1Char('\n'));
  };
  return def;
}

// ---------------------------------------------------------------------------
// The Appearance tool (mitcad#46, mitcad#53; the picker and the fx,
// mitcad#1059)

CommandDef appearanceCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.appearance");
  def.name = QObject::tr("Appearance");
  def.icon = QStringLiteral("appearance");
  def.tooltip = QObject::tr("How bodies look");
  def.group = QStringLiteral("MODIFY");
  def.keywords = QStringList{QStringLiteral("colour"), QStringLiteral("color"), QStringLiteral("paint")};
  def.modelCommand = true;
  const auto cache = std::make_shared<MaterialCache>();
  const ToolKind kind{false, QStringLiteral("value"), QStringLiteral("Appearance tool"), QStringLiteral("bodies")};

  InputDef value = choiceInput(QStringLiteral("value"), QObject::tr("Appearance"), {}, QString());
  value.withTooltip(QObject::tr("The appearance: the library's or the design's"))
      .withChoices([cache](const CommandState&) {
        // The library's appearances and the document's own (mitcad#46).
        Choices choices{{defaultKey(), QObject::tr("Default")}};
        for (const Appearance& look : cache->appearances) {
          choices.append({look.id, look.name});
        }
        return choices;
      })
      .withChoiceTile([cache, kind](const CommandState& state) { return valueTile(kind, *cache, state); })
      .withButton(QObject::tr("Choose…"), QObject::tr("Choose the appearance"),
                  [cache, kind](const CommandState& state, const CommandContext& model) {
                    const QString materialClass = targetsMaterialClass(*cache, state, model);
                    openPicker(kind, QStringLiteral("solid.appearance"), appearanceEntries(*cache, true, materialClass),
                               cache, state, model);
                  });

  // Appearances also go to single faces (mitcad#53), overriding their
  // body's; Default takes a face's own away.
  InputDef bodies = selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies"), SelectKind::Body, 0, 0);
  InputDef faces = selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 0, 0);
  const auto targetsChanged = [cache, kind](CommandState& state, const CommandContext&) {
    loadBinding(kind, *cache, state);
    if (state.choice(QStringLiteral("value")).isEmpty()) {
      state.setChoice(QStringLiteral("value"), sharedAppearance(*cache, state));
    }
  };
  bodies.withOnChange(targetsChanged);
  faces.withOnChange(targetsChanged);
  def.inputs.clear();
  def.inputs << value;
  def.inputs << bindingInputs(kind, cache);
  def.inputs << bodies << faces;
  def.enabled = [&context] { return hasBodies(context); };
  def.init = [cache, kind](CommandState& state, const CommandContext& model) {
    readCache(*cache, model, false);
    state.setObject(QStringLiteral("binding"), QJsonObject());
    loadBinding(kind, *cache, state);
    if (state.choice(QStringLiteral("value")).isEmpty()) {
      state.setChoice(QStringLiteral("value"), sharedAppearance(*cache, state));
    }
  };
  def.build = [name = def.name, cache, kind](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonArray commands;
    QStringList owners;
    QHash<QString, QJsonArray> faceNames;
    for (const SelectionItem& item : state.items(QStringLiteral("faces"))) {
      if (!owners.contains(item.owner)) {
        owners << item.owner;
      }
      faceNames[item.owner].append(item.name);
    }
    if (owners.isEmpty() && state.items(QStringLiteral("bodies")).isEmpty()) {
      return Built::failure(QObject::tr("Select bodies or faces"), QStringLiteral("bodies"));
    }
    // One appearance, or an expression of an option (the fx).
    QString field = QStringLiteral("appearance");
    QJsonValue given;
    if (varies(kind, state)) {
      const QString expression = bindingOf(kind, *cache, state, nullptr);
      if (expression.isEmpty()) {
        return Built::failure(QObject::tr("Choose an appearance for a value, or above."), QStringLiteral("value"));
      }
      field = QStringLiteral("expression");
      given = expression;
    } else {
      const QString key = state.choice(QStringLiteral("value"));
      if (key.isEmpty()) {
        return Built::failure(QObject::tr("Choose an appearance."), QStringLiteral("value"));
      }
      given = key == defaultKey() ? QJsonValue() : QJsonValue(key);
    }
    for (const QString& uid : ownersOf(state.items(QStringLiteral("bodies")), SelectKind::Body)) {
      commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_body_appearance")},
                                  {QStringLiteral("uid"), uid},
                                  {field, given}});
    }
    for (const QString& owner : std::as_const(owners)) {
      commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_face_appearance")},
                                  {QStringLiteral("uid"), owner},
                                  {QStringLiteral("faces"), faceNames.value(owner)},
                                  {field, given}});
    }
    result.def = {{QStringLiteral("commands"), commands}, {QStringLiteral("label"), name}};
    return result;
  };
  def.describe = [cache, kind](const CommandState& state, const CommandContext&) {
    const auto faceCount = state.items(QStringLiteral("faces")).size();
    const QString key = state.choice(QStringLiteral("value"));
    const QString shown = varies(kind, state) ? bindingOf(kind, *cache, state, nullptr)
                          : key == defaultKey() ? QString()
                                                : key;
    // After OK (the Material tool's recent materials likewise): one
    // appearance given, not Default nor an expression, is remembered for
    // the picker's Recent (mitcad#1685).
    if (!varies(kind, state) && !key.isEmpty() && appearanceKeyOf(*cache, key) == key) {
      rememberRecentAppearance(key);
      qDebug().noquote() << QStringLiteral("Appearance tool: recent %1")
                                .arg(recentAppearances().join(QStringLiteral(", ")));
    }
    return QStringLiteral("Appearance %1 for %2 body(ies)%3")
        .arg(shown)
        .arg(state.items(QStringLiteral("bodies")).size())
        .arg(faceCount > 0 ? QStringLiteral(", %1 face(s)").arg(faceCount) : QString());
  };
  return def;
}

} // namespace mitcad::cmd
