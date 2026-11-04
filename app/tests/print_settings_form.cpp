// SPDX-License-Identifier: MIT
// The print key table's layout and values and the shared settings form
// (mitcad#655): the categories and sub-groups come from the `print_keys`
// query's layout (or the keys' own without one); values read and typed in
// the key's own unit; Advanced off hides an advanced key unless it is set,
// a tab with nothing to show is hidden, the search finds the plain names
// only; a field commits what is typed, in its range.
#include <cmath>
#include <cstdio>
#include <limits>

#include <QComboBox>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QObject>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QToolButton>
#include <QWidget>

#include "print/PrintKeys.hpp"
#include "print/SettingField.hpp"
#include "print/SettingsForm.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "print_settings_form.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonObject key(const char* name, const char* plain, const char* kind, const char* unit, const char* category,
                const char* group, bool advanced) {
  return QJsonObject{{QStringLiteral("key"), QString::fromLatin1(name)},
                     {QStringLiteral("set"), QStringLiteral("process")},
                     {QStringLiteral("name"), QString::fromLatin1(plain)},
                     {QStringLiteral("kind"), QString::fromLatin1(kind)},
                     {QStringLiteral("unit"), QString::fromLatin1(unit)},
                     {QStringLiteral("category"), QString::fromLatin1(category)},
                     {QStringLiteral("group"), QString::fromLatin1(group)},
                     {QStringLiteral("advanced"), advanced}};
}

// A key or a dialect entry with a range: `min`, and `max` unless it is
// unbounded (as the `print_keys` query leaves it out).
QJsonObject limited(QJsonObject object, double low, double high = std::numeric_limits<double>::infinity()) {
  object.insert(QStringLiteral("min"), low);
  if (std::isfinite(high)) {
    object.insert(QStringLiteral("max"), high);
  }
  return object;
}

// Keys whose ranges go beyond the usual (mitcad#746): a purge line spacing
// and a tower infill gap above 100 %, and Bambu Studio's -1 ("automatic")
// that OrcaSlicer does not take.
QJsonObject rangedAnswer() {
  QJsonArray keys;
  keys.append(limited(key("wipe_tower_extra_spacing", "Prime tower extra spacing", "percent", "", "others",
                          "prime_tower", true),
                      100.0, 300.0));
  keys.append(limited(
      key("prime_tower_infill_gap", "Prime tower infill gap", "percent", "", "others", "prime_tower", true), 100.0));
  keys.append(limited(key("fill_density", "Sparse infill density", "percent", "", "strength", "sparse_infill", false),
                      0.0, 100.0));
  QJsonObject walls =
      limited(key("tree_support_wall_count", "Tree support walls", "integer", "", "support", "tree_supports", true),
              -1.0);
  QJsonArray dialects;
  dialects.append(QJsonObject{{QStringLiteral("dialect"), QStringLiteral("bambu_studio")},
                              {QStringLiteral("key"), QStringLiteral("tree_support_wall_count")},
                              {QStringLiteral("list"), false}});
  dialects.append(limited(QJsonObject{{QStringLiteral("dialect"), QStringLiteral("orcaslicer")},
                                      {QStringLiteral("key"), QStringLiteral("tree_support_wall_count")},
                                      {QStringLiteral("list"), false}},
                          0.0, 2.0));
  walls.insert(QStringLiteral("dialects"), dialects);
  keys.append(walls);
  keys.append(key("xy_size_compensation", "X-Y outer contour compensation", "number", "mm", "quality", "precision",
                  true));
  return QJsonObject{{QStringLiteral("keys"), keys}};
}

QJsonObject group(const char* id, const char* title) {
  return QJsonObject{{QStringLiteral("id"), QString::fromLatin1(id)}, {QStringLiteral("title"), QString::fromLatin1(title)}};
}

// A process set as the model gives it: three categories, an advanced key
// in each of two of them.
QJsonObject answer(bool withLayout) {
  QJsonObject seam = key("seam_position", "Seam position", "choice", "", "quality", "seam", false);
  QJsonArray seamChoices;
  seamChoices.append(QJsonObject{{QStringLiteral("value"), QStringLiteral("aligned")},
                                 {QStringLiteral("name"), QStringLiteral("Aligned")}});
  seamChoices.append(QJsonObject{{QStringLiteral("value"), QStringLiteral("nearest")},
                                 {QStringLiteral("name"), QStringLiteral("Nearest")}});
  seam.insert(QStringLiteral("choices"), seamChoices);
  QJsonArray keys;
  // Numbers with their ranges as the model gives them (mitcad#746).
  keys.append(limited(key("layer_height", "Layer height", "number", "mm", "quality", "layer_height", false), 0.0));
  keys.append(limited(key("first_layer_extrusion_width", "First layer line width", "number_or_percent", "mm",
                          "quality", "layer_height", false),
                      0.0));
  keys.append(seam);
  keys.append(key("wall_sequence", "Walls printing order", "text", "", "quality", "order_and_flow", true));
  keys.append(limited(key("perimeters", "Walls", "integer", "", "strength", "walls", false), 0.0));
  keys.append(
      limited(key("fill_density", "Sparse infill density", "percent", "", "strength", "sparse_infill", false), 0.0, 100.0));
  keys.append(limited(key("default_jerk", "Default", "number", "mm/s", "speed", "jerk", true), 0.0));
  QJsonObject result{{QStringLiteral("keys"), keys}};
  if (withLayout) {
    QJsonArray quality;
    quality.append(group("layer_height", "Layer height"));
    quality.append(group("seam", "Seam"));
    quality.append(group("order_and_flow", "Order and flow"));
    QJsonArray strength;
    strength.append(group("walls", "Walls"));
    strength.append(group("sparse_infill", "Sparse infill"));
    QJsonArray speed;
    speed.append(group("jerk", "Jerk"));
    QJsonArray categories;
    categories.append(QJsonObject{{QStringLiteral("id"), QStringLiteral("quality")},
                                  {QStringLiteral("title"), QStringLiteral("Quality")},
                                  {QStringLiteral("groups"), quality}});
    categories.append(QJsonObject{{QStringLiteral("id"), QStringLiteral("strength")},
                                  {QStringLiteral("title"), QStringLiteral("Strength")},
                                  {QStringLiteral("groups"), strength}});
    categories.append(QJsonObject{{QStringLiteral("id"), QStringLiteral("speed")},
                                  {QStringLiteral("title"), QStringLiteral("Speed")},
                                  {QStringLiteral("groups"), speed}});
    QJsonArray layout;
    layout.append(QJsonObject{{QStringLiteral("set"), QStringLiteral("process")},
                              {QStringLiteral("categories"), categories}});
    result.insert(QStringLiteral("layout"), layout);
  }
  return result;
}

} // namespace

int printSettingsFormTests() {
  using namespace mitcad;

  // The layout: categories and groups in order, with their titles.
  const PrintKeyTable table(answer(true));
  const QVector<PrintCategory> layout = table.layout(QStringLiteral("process"));
  CHECK(layout.size() == 3);
  CHECK(layout.value(0).title == QStringLiteral("Quality"));
  CHECK(layout.value(0).groups.size() == 3);
  CHECK(layout.value(2).groups.value(0).title == QStringLiteral("Jerk"));
  CHECK(table.keys(QStringLiteral("process")).size() == 7);
  const PrintKey* height = table.find(QStringLiteral("layer_height"));
  CHECK(height != nullptr && table.categoryOf(*height) == QStringLiteral("quality") &&
        table.groupOf(*height) == QStringLiteral("layer_height"));
  // Without a layout: the keys' own categories, in their order.
  const PrintKeyTable bare(answer(false));
  CHECK(bare.layout(QStringLiteral("process")).size() == 3);
  CHECK(bare.layout(QStringLiteral("process")).value(1).id == QStringLiteral("strength"));
  CHECK(bare.layout(QStringLiteral("process")).value(1).title == QStringLiteral("Strength"));

  // Values in the key's own unit.
  const PrintKey* seam = table.find(QStringLiteral("seam_position"));
  const PrintKey* width = table.find(QStringLiteral("first_layer_extrusion_width"));
  const PrintKey* walls = table.find(QStringLiteral("perimeters"));
  const PrintKey* density = table.find(QStringLiteral("fill_density"));
  CHECK(seam != nullptr && width != nullptr && walls != nullptr && density != nullptr);
  if (seam == nullptr || width == nullptr || walls == nullptr || density == nullptr || height == nullptr) {
    return failures;
  }
  CHECK(printValueText(*height, 0.2) == QStringLiteral("0.2 mm"));
  CHECK(printValueText(*density, 15) == QStringLiteral("15 %"));
  CHECK(printValueText(*seam, QStringLiteral("aligned")) == QStringLiteral("Aligned"));
  CHECK(printValueText(*width, QStringLiteral("110%")) == QStringLiteral("110 %"));
  CHECK(printValueText(*width, 0.45) == QStringLiteral("0.45 mm"));
  CHECK(printValueText(*walls, 3) == QStringLiteral("3"));
  QJsonArray bed;
  bed.append(QJsonArray{0.0, 0.0});
  bed.append(QJsonArray{256.0, 0.0});
  bed.append(QJsonArray{256.0, 256.0});
  bed.append(QJsonArray{0.0, 256.0});
  CHECK(outlineText(bed) == QStringLiteral("256 × 256 mm"));
  CHECK(slicerTitle(QStringLiteral("bambu_studio")) == QStringLiteral("Bambu Studio"));
  // Typed values as the model takes them.
  QString error;
  CHECK(printValueOf(*height, QStringLiteral("0,25"), &error) == QJsonValue(0.25) && error.isEmpty());
  CHECK(printValueOf(*density, QStringLiteral("25%"), &error) == QJsonValue(25.0));
  CHECK(printValueOf(*seam, QStringLiteral("Nearest"), &error) == QJsonValue(QStringLiteral("nearest")));
  CHECK(printValueOf(*width, QStringLiteral("50%"), &error) == QJsonValue(QStringLiteral("50%")));
  CHECK(printValueOf(*walls, QStringLiteral("4"), &error) == QJsonValue(4.0));
  CHECK(printValueOf(*walls, QStringLiteral("3.5"), &error).isNull() && !error.isEmpty());
  CHECK(printValueOf(*height, QString(), &error).isNull() && error.isEmpty());
  // Real ranges (mitcad#655): no negative lengths or counts, a
  // percentage up to 100.
  CHECK(printValueOf(*height, QStringLiteral("-0.2"), &error).isNull() && !error.isEmpty());
  CHECK(printValueOf(*walls, QStringLiteral("-1"), &error).isNull() && !error.isEmpty());
  CHECK(printValueOf(*density, QStringLiteral("150"), &error).isNull() && !error.isEmpty());
  CHECK(printValueOf(*width, QStringLiteral("-0.4"), &error).isNull() && !error.isEmpty());

  // mitcad#746: the model's ranges, not guesses from the key's name: a
  // purge line spacing and a tower infill gap of 150 %, Bambu Studio's -1
  // ("automatic") with a Bambu Studio printer but not with an OrcaSlicer
  // one; a compensation either way; still no fill density of 150 %.
  const PrintKeyTable ranged(rangedAnswer());
  const PrintKey* spacing = ranged.find(QStringLiteral("wipe_tower_extra_spacing"));
  const PrintKey* gap = ranged.find(QStringLiteral("prime_tower_infill_gap"));
  const PrintKey* treeWalls = ranged.find(QStringLiteral("tree_support_wall_count"));
  const PrintKey* infill = ranged.find(QStringLiteral("fill_density"));
  const PrintKey* contour = ranged.find(QStringLiteral("xy_size_compensation"));
  CHECK(spacing != nullptr && gap != nullptr && treeWalls != nullptr && infill != nullptr && contour != nullptr);
  if (spacing != nullptr && gap != nullptr && treeWalls != nullptr && infill != nullptr && contour != nullptr) {
    CHECK(printValueOf(*spacing, QStringLiteral("150"), &error) == QJsonValue(150.0) && error.isEmpty());
    CHECK(printValueOf(*spacing, QStringLiteral("150%"), &error) == QJsonValue(150.0) && error.isEmpty());
    CHECK(printValueOf(*spacing, QStringLiteral("50"), &error).isNull() &&
          error == QStringLiteral("Use a value from 100 to 300"));
    CHECK(printValueOf(*gap, QStringLiteral("150"), &error) == QJsonValue(150.0) && error.isEmpty());
    CHECK(printValueOf(*gap, QStringLiteral("150"), &error, QStringLiteral("bambu_studio")) == QJsonValue(150.0));
    CHECK(printValueOf(*treeWalls, QStringLiteral("-1"), &error, QStringLiteral("bambu_studio")) == QJsonValue(-1.0) &&
          error.isEmpty());
    CHECK(printValueOf(*treeWalls, QStringLiteral("-1"), &error) == QJsonValue(-1.0) && error.isEmpty()); // no slicer
    CHECK(printValueOf(*treeWalls, QStringLiteral("-1"), &error, QStringLiteral("orcaslicer")).isNull() &&
          error == QStringLiteral("Use a value from 0 to 2"));
    CHECK(printValueOf(*treeWalls, QStringLiteral("-2"), &error, QStringLiteral("bambu_studio")).isNull() &&
          error == QStringLiteral("Use a value of -1 or more"));
    CHECK(printRange(*treeWalls, QStringLiteral("orcaslicer")).high == 2.0);
    CHECK(printValueOf(*infill, QStringLiteral("150"), &error, QStringLiteral("bambu_studio")).isNull() &&
          error == QStringLiteral("Use a value from 0 to 100"));
    CHECK(printValueOf(*contour, QStringLiteral("-0.05"), &error) == QJsonValue(-0.05) && error.isEmpty());
  }

  // The form: Advanced off hides an advanced key unless it is set; a tab
  // with nothing to show is hidden; the search finds every key.
  QSettings().setValue(QStringLiteral("print/advancedSettings"), false);
  SettingsForm form;
  form.setKeys(table, QStringLiteral("process"));
  form.setValues(QJsonObject{{QStringLiteral("layer_height"), 0.2}});
  CHECK(form.keyShown(QStringLiteral("layer_height")));
  CHECK(!form.keyShown(QStringLiteral("wall_sequence")));
  CHECK(!form.keyShown(QStringLiteral("perimeters"))); // another tab
  CHECK(form.shownTabs() == QStringList({QStringLiteral("quality"), QStringLiteral("strength")}));
  form.setValues(QJsonObject{{QStringLiteral("layer_height"), 0.2}, {QStringLiteral("default_jerk"), 8},
                             {QStringLiteral("wall_sequence"), QStringLiteral("inner_outer")}});
  CHECK(form.keyShown(QStringLiteral("wall_sequence"))); // set: shown
  CHECK(form.shownTabs().contains(QStringLiteral("speed")));
  CHECK(form.rows().contains(QStringLiteral("default_jerk = 8 mm/s")));
  form.setHiddenKeys({QStringLiteral("layer_height")});
  CHECK(!form.keyShown(QStringLiteral("layer_height")));
  form.setHiddenKeys({});
  for (QLineEdit* search : form.findChildren<QLineEdit*>(QStringLiteral("settingsSearch"))) {
    search->setText(QStringLiteral("infill"));
  }
  CHECK(form.keyShown(QStringLiteral("fill_density")));
  CHECK(!form.keyShown(QStringLiteral("layer_height")));
  for (QLineEdit* search : form.findChildren<QLineEdit*>(QStringLiteral("settingsSearch"))) {
    search->setText(QStringLiteral("fill_density")); // a raw key finds nothing
  }
  CHECK(!form.keyShown(QStringLiteral("fill_density")));

  // A field commits what is typed, in the key's unit; an unset one is an
  // empty input with the placeholder Slicer default, and leaving it empty
  // sets nothing.
  SettingField field(*height);
  QJsonValue committed;
  int commits = 0;
  QObject::connect(&field, &SettingField::committed,
                   [&committed, &commits](const QString&, const QJsonValue& value) {
                     committed = value;
                     ++commits;
                   });
  CHECK(!field.isSet());
  QLineEdit* line = field.findChild<QLineEdit*>();
  CHECK(line != nullptr && line->text().isEmpty() && line->placeholderText() == QStringLiteral("Slicer default"));
  if (line != nullptr) {
    emit line->editingFinished();
  }
  CHECK(commits == 0);
  field.setValue(0.2);
  CHECK(field.isSet());
  CHECK(line != nullptr && line->text() == QStringLiteral("0.2"));
  if (line != nullptr) {
    line->setText(QStringLiteral("0.28"));
    emit line->textEdited(line->text());
    emit line->editingFinished();
  }
  CHECK(committed == QJsonValue(0.28));

  // Round 4 (mitcad#655): refusals as instructions; what an unset field
  // stands for as its placeholder; Update's value as a hint; the slicers
  // by their titles in a message; a window's own page as a first tab, and
  // alone without print settings.
  CHECK(printValueOf(*density, QStringLiteral("150"), &error).isNull() &&
        error == QStringLiteral("Use a value from 0 to 100"));
  CHECK(printValueOf(*walls, QStringLiteral("-1"), &error).isNull() && error.startsWith(QStringLiteral("Use ")));
  SettingField inherited(*height);
  inherited.setPlaceholderValue(0.3);
  QLineEdit* inheritedLine = inherited.findChild<QLineEdit*>();
  CHECK(inheritedLine != nullptr && inheritedLine->placeholderText() == QStringLiteral("0.3"));
  inherited.setPlaceholderValue(QJsonValue());
  CHECK(inheritedLine != nullptr && inheritedLine->placeholderText() == QStringLiteral("Slicer default"));
  inherited.setHint(QStringLiteral("Newer: 0.28"), QStringLiteral("My library has 0.28"));
  bool hinted = false;
  for (const QLabel* label : inherited.findChildren<QLabel*>()) {
    hinted = hinted || label->text() == QStringLiteral("Newer: 0.28");
  }
  CHECK(hinted);
  CHECK(table.plainText(QStringLiteral("fill_density for bambu_studio: too high")) ==
        QStringLiteral("Sparse infill density for Bambu Studio: too high"));
  SettingsForm withGeneral;
  auto* general = new QWidget;
  withGeneral.setGeneralPage(general, QStringLiteral("General"));
  withGeneral.setKeys(table, QStringLiteral("process"));
  withGeneral.setValues(QJsonObject{{QStringLiteral("layer_height"), 0.2}});
  CHECK(withGeneral.shownTabs().value(0) == QStringLiteral("general"));
  withGeneral.setPrintShown(false);
  CHECK(withGeneral.shownTabs() == QStringList({QStringLiteral("general")}));
  CHECK(!withGeneral.keyShown(QStringLiteral("layer_height")));
  withGeneral.setPrintShown(true);
  CHECK(withGeneral.shownTabs().contains(QStringLiteral("quality")));
  // Search finds the general page's fields: the window is told the text,
  // to show only those it finds there (mitcad#1228); otherwise nothing.
  QString filtered = QStringLiteral("not called");
  withGeneral.filterGeneral = [&filtered](const QString& search) { filtered = search; };
  withGeneral.setGeneralNames({QStringLiteral("Bed temperature")});
  CHECK(filtered.isEmpty());
  QLineEdit* generalSearch = withGeneral.findChild<QLineEdit*>(QStringLiteral("settingsSearch"));
  CHECK(generalSearch != nullptr);
  if (generalSearch != nullptr) {
    generalSearch->setText(QStringLiteral("temperature"));
    CHECK(filtered == QStringLiteral("temperature"));
    generalSearch->setText(QStringLiteral("layer"));
    CHECK(filtered.isEmpty());
    generalSearch->clear();
    CHECK(filtered.isEmpty());
  }

  // Round 6 (mitcad#655): [mm | %] only where the design's slicer takes a
  // percentage; else the unit as a label, "%" for a percentage set.
  CHECK(width->takesPercent(QString()));
  CHECK(!width->takesPercent(QStringLiteral("bambu_studio")));
  SettingField withBox(*width);
  CHECK(withBox.findChild<QComboBox*>() != nullptr);
  SettingField plainUnit(*width, nullptr, false);
  CHECK(plainUnit.findChild<QComboBox*>() == nullptr);
  plainUnit.setValue(QStringLiteral("110%"));
  bool percentShown = false;
  for (const QLabel* label : plainUnit.findChildren<QLabel*>()) {
    percentShown = percentShown || label->text() == QStringLiteral("%");
  }
  CHECK(percentShown);
  // A body's own value of the key shows beside the field, as a hint.
  plainUnit.setOwnValues({QStringLiteral("4 on Body1"), QStringLiteral("3 on Arm")});
  bool marked = false;
  for (const QLabel* label : plainUnit.findChildren<QLabel*>()) {
    marked = marked || label->text() == QStringLiteral("4 on Body1, 3 on Arm");
  }
  CHECK(marked);

  // A key kept per slicer that the form edits (mitcad#1062: a material's
  // filament preset): a row per slicer, shown with Advanced off, each
  // field its slicer's entry; unset, a Bambu-family slicer shows the name
  // the export takes from another member; ↺ takes out its slicer's entry
  // only. Without setSlicerKeys such a key is not shown.
  const QJsonObject named{{QStringLiteral("bambu_studio"), QStringLiteral("Generic PETG HF")},
                          {QStringLiteral("prusaslicer"), QStringLiteral("Prusament PETG")}};
  CHECK(slicerPresetOf(named, QStringLiteral("bambu_studio")) == QStringLiteral("Generic PETG HF"));
  QString from;
  CHECK(slicerPresetOf(named, QStringLiteral("qidi_studio"), &from) == QStringLiteral("Generic PETG HF"));
  CHECK(from == QStringLiteral("bambu_studio"));
  CHECK(slicerPresetOf(named, QStringLiteral("cura")).isEmpty());
  CHECK(slicerPresetOf(QJsonObject{{QStringLiteral("prusaslicer"), QStringLiteral("Prusament PETG")}},
                       QStringLiteral("orcaslicer"))
            .isEmpty());
  CHECK(slicerPresetOf(QJsonValue(QStringLiteral("Any")), QStringLiteral("cura")) == QStringLiteral("Any"));
  QJsonObject preset = key("filament_settings_id", "Filament preset", "text", "", "quality", "seam", true);
  preset.insert(QStringLiteral("per_dialect"), true);
  QJsonArray presetDialects;
  for (const QString& id : {QStringLiteral("bambu_studio"), QStringLiteral("orcaslicer"), QStringLiteral("prusaslicer")}) {
    presetDialects.append(QJsonObject{{QStringLiteral("dialect"), id},
                                      {QStringLiteral("key"), QStringLiteral("filament_settings_id")},
                                      {QStringLiteral("list"), true}});
  }
  preset.insert(QStringLiteral("dialects"), presetDialects);
  QJsonArray presetKeys;
  presetKeys.append(preset);
  const PrintKeyTable presetTable(QJsonObject{{QStringLiteral("keys"), presetKeys}});
  CHECK(presetTable.find(QStringLiteral("filament_settings_id")) != nullptr &&
        presetTable.find(QStringLiteral("filament_settings_id"))->dialects.size() == 3);
  SettingsForm presets;
  presets.setSlicerKeys({QStringLiteral("filament_settings_id")});
  presets.setKeys(presetTable, QStringLiteral("process"));
  presets.setValues(QJsonObject{{QStringLiteral("filament_settings_id"), named}});
  CHECK(presets.keyShown(QStringLiteral("filament_settings_id")));
  CHECK(presets.rows() == QStringList({QStringLiteral("filament_settings_id.bambu_studio = Generic PETG HF"),
                                       QStringLiteral("filament_settings_id.prusaslicer = Prusament PETG")}));
  SettingField* orca = nullptr;
  SettingField* bambu = nullptr;
  for (SettingField* presetField : presets.findChildren<SettingField*>()) {
    if (presetField->slicer() == QStringLiteral("orcaslicer")) {
      orca = presetField;
    } else if (presetField->slicer() == QStringLiteral("bambu_studio")) {
      bambu = presetField;
    }
  }
  CHECK(orca != nullptr && bambu != nullptr);
  if (orca != nullptr && bambu != nullptr) {
    const QLineEdit* orcaLine = orca->findChild<QLineEdit*>();
    CHECK(orcaLine != nullptr && orcaLine->text().isEmpty() &&
          orcaLine->placeholderText() == QStringLiteral("Generic PETG HF (as Bambu Studio)"));
    const QLineEdit* bambuLine = bambu->findChild<QLineEdit*>();
    CHECK(bambuLine != nullptr && bambuLine->text() == QStringLiteral("Generic PETG HF"));
    QJsonValue presetCommitted(QStringLiteral("nothing"));
    QObject::connect(bambu, &SettingField::committed,
                     [&presetCommitted](const QString&, const QJsonValue& value) { presetCommitted = value; });
    auto* reset = bambu->findChild<QToolButton*>();
    CHECK(reset != nullptr);
    if (reset != nullptr) {
      reset->click();
    }
    CHECK(presetCommitted ==
          QJsonValue(QJsonObject{{QStringLiteral("prusaslicer"), QStringLiteral("Prusament PETG")}}));
  }
  SettingsForm withoutSlicers;
  withoutSlicers.setKeys(presetTable, QStringLiteral("process"));
  withoutSlicers.setValues(QJsonObject{{QStringLiteral("filament_settings_id"), named}});
  CHECK(!withoutSlicers.keyShown(QStringLiteral("filament_settings_id")));
  return failures;
}
