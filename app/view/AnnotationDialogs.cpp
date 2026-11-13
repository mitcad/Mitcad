// SPDX-License-Identifier: MIT
#include "AnnotationDialogs.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <initializer_list>
#include <optional>

#include <QCheckBox>
#include <QColor>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QEvent>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLabel>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QRegularExpression>
#include <QRegularExpressionValidator>
#include <QSignalBlocker>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/UserText.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// The kinds of dimension in words, as Measure names them.
const std::pair<const char*, const char*> kKinds[] = {
    {"distance", QT_TRANSLATE_NOOP("Annotations", "Distance")},
    {"angle", QT_TRANSLATE_NOOP("Annotations", "Angle")},
    {"offset", QT_TRANSLATE_NOOP("Annotations", "Offset")},
    {"axis_distance", QT_TRANSLATE_NOOP("Annotations", "Axis distance")},
    {"center_distance", QT_TRANSLATE_NOOP("Annotations", "Center distance")},
    {"radius", QT_TRANSLATE_NOOP("Annotations", "Radius")},
    {"diameter", QT_TRANSLATE_NOOP("Annotations", "Diameter")}};

QString kindName(const QString& key) {
  for (const auto& [kind, name] : kKinds) {
    if (key == QLatin1String(kind)) {
      return QCoreApplication::translate("Annotations", name);
    }
  }
  return key;
}

// ISO 286-2's common classes, offered first; another can be typed.
const char* const kFits[] = {"H7", "H8", "H9", "H11", "G7", "F8", "E9", "D10", "JS7", "K7", "M7",
                             "N7", "P7", "R7", "S7", "h6", "h7", "h9", "h11", "g6", "f7", "e8",
                             "d9", "c11", "js6", "k6", "m6", "n6", "p6", "r6", "s6"};

QString compact(const QJsonValue& value) {
  if (value.isObject()) {
    return QString::fromUtf8(QJsonDocument(value.toObject()).toJson(QJsonDocument::Compact));
  }
  if (value.isNull() || value.isUndefined()) {
    return QStringLiteral("null");
  }
  return QString::fromUtf8(QJsonDocument(QJsonArray{value}).toJson(QJsonDocument::Compact)).mid(1).chopped(1);
}

// The prefix a kind has unless its format says otherwise.
QString defaultPrefix(const QString& kind) {
  if (kind == QLatin1String("diameter")) {
    return QStringLiteral("Ø");
  }
  if (kind == QLatin1String("radius")) {
    return QStringLiteral("R");
  }
  return QString();
}

// A radius and a diameter lie in their circle's plane and an angle in its
// sides': the model takes no plane picked for them.
bool liesInOwnPlane(const QString& kind) {
  return kind == QLatin1String("radius") || kind == QLatin1String("diameter") || kind == QLatin1String("angle");
}

// A tolerance's fields in a form: its type, one or two values, a fit code
// and whether a fit's deviations are shown; the rows that do not apply
// hidden.
struct ToleranceFields {
  QFormLayout* form = nullptr;
  bool angle = false;
  // The value (mm or degrees) when one annotation is edited: limits are
  // typed as the largest and the smallest size then, not as deviations.
  std::optional<double> nominal;
  QComboBox* type = nullptr;
  QLabel* firstLabel = nullptr;
  QDoubleSpinBox* first = nullptr;
  QLabel* secondLabel = nullptr;
  QDoubleSpinBox* second = nullptr;
  QComboBox* fit = nullptr;
  QCheckBox* shown = nullptr;
  // Whether the user changed the fields since `set`, and whether another
  // kind dropped the tolerance (between an angle and a length): only then
  // is the tolerance sent back.
  bool userEdited = false;
  bool dropped = false;
  bool edited() const { return userEdited || dropped; }
  // The fields are being changed here, not by the user.
  bool quiet = false;

  // Marks the fields changed by the user from now on.
  void watch(QWidget* parent) {
    const auto mark = [this] {
      if (!quiet) {
        userEdited = true;
      }
    };
    QObject::connect(type, &QComboBox::currentIndexChanged, parent, mark);
    QObject::connect(first, &QDoubleSpinBox::valueChanged, parent, mark);
    QObject::connect(second, &QDoubleSpinBox::valueChanged, parent, mark);
    QObject::connect(fit, &QComboBox::currentTextChanged, parent, mark);
    QObject::connect(shown, &QCheckBox::toggled, parent, mark);
  }

  // Another kind chosen: its unit (degrees for an angle), Fit only for a
  // length, and its value for limits. Fields the user has not touched show
  // the `original` tolerance again while the kind is an angle as the
  // original kind was (or a length as it was), and none otherwise (it is
  // dropped); fields the user changed stay, unless the unit changed.
  void retarget(bool isAngle, std::optional<double> measured, const QJsonValue& original, bool originalAngle) {
    const bool crossed = isAngle != angle;
    angle = isAngle;
    nominal = measured;
    quiet = true;
    for (QDoubleSpinBox* box : {first, second}) {
      box->setSingleStep(angle ? 0.1 : 0.01);
      box->setSuffix(angle ? QStringLiteral("°") : QObject::tr(" mm"));
    }
    const int fitIndex = type->findData(QStringLiteral("fit"));
    if (angle && fitIndex >= 0) {
      type->removeItem(fitIndex);
    } else if (!angle && fitIndex < 0) {
      type->addItem(QObject::tr("Fit (ISO 286)"), QStringLiteral("fit"));
    }
    if (!userEdited) {
      if (isAngle == originalAngle) {
        set(original);
        dropped = false;
      } else {
        type->setCurrentIndex(0);
        dropped = true;
      }
    } else if (crossed) {
      type->setCurrentIndex(0);
    }
    quiet = false;
    follow();
  }

  void build(QFormLayout* into, bool isAngle, std::optional<double> measured, QWidget* parent) {
    form = into;
    angle = isAngle;
    nominal = measured;
    type = new QComboBox(parent);
    type->addItem(QObject::tr("None"), QStringLiteral("none"));
    type->addItem(QObject::tr("± symmetric"), QStringLiteral("symmetric"));
    type->addItem(QObject::tr("Deviations"), QStringLiteral("deviations"));
    type->addItem(QObject::tr("Limits"), QStringLiteral("limits"));
    if (!angle) {
      type->addItem(QObject::tr("Fit (ISO 286)"), QStringLiteral("fit"));
    }
    form->addRow(QObject::tr("&Tolerance:"), type);
    const auto spin = [&] {
      auto* box = new QDoubleSpinBox(parent);
      box->setRange(-1e7, 1e7);
      box->setDecimals(4);
      box->setSingleStep(angle ? 0.1 : 0.01);
      box->setSuffix(angle ? QStringLiteral("°") : QObject::tr(" mm"));
      return box;
    };
    first = spin();
    second = spin();
    firstLabel = new QLabel(QObject::tr("&Value:"), parent);
    firstLabel->setBuddy(first);
    secondLabel = new QLabel(QObject::tr("&Lower deviation:"), parent);
    secondLabel->setBuddy(second);
    form->addRow(firstLabel, first);
    form->addRow(secondLabel, second);
    fit = new QComboBox(parent);
    fit->setEditable(true);
    for (const char* code : kFits) {
      fit->addItem(QLatin1String(code));
    }
    form->addRow(QObject::tr("&Fit:"), fit);
    shown = new QCheckBox(QObject::tr("Show d&eviations"), parent);
    form->addRow(QString(), shown);
    QObject::connect(type, &QComboBox::currentIndexChanged, parent, [this] {
      // Values to start from for the type chosen.
      const QString key = type->currentData().toString();
      const double step = angle ? 0.5 : 0.1;
      if (key == QLatin1String("symmetric")) {
        first->setValue(step);
      } else if (key == QLatin1String("deviations") || key == QLatin1String("limits")) {
        const double base = key == QLatin1String("limits") ? nominal.value_or(0.0) : 0.0;
        first->setValue(base + step);
        second->setValue(base - step);
      }
      follow();
    });
  }

  bool sizes() const { return nominal && type->currentData().toString() == QLatin1String("limits"); }

  void follow() {
    const QString key = type->currentData().toString();
    const bool one = key == QLatin1String("symmetric");
    const bool two = key == QLatin1String("deviations") || key == QLatin1String("limits");
    firstLabel->setText(one ? QObject::tr("&Value:") : sizes() ? QObject::tr("&Upper limit:")
                                                               : QObject::tr("&Upper deviation:"));
    secondLabel->setText(sizes() ? QObject::tr("&Lower limit:") : QObject::tr("&Lower deviation:"));
    form->setRowVisible(first, one || two);
    form->setRowVisible(second, two);
    form->setRowVisible(fit, key == QLatin1String("fit"));
    form->setRowVisible(shown, key == QLatin1String("fit"));
  }

  void set(const QJsonValue& tolerance) {
    const QJsonObject t = tolerance.toObject();
    const QString key = t.value(QStringLiteral("type")).toString(QStringLiteral("none"));
    const QSignalBlocker block(type);
    type->setCurrentIndex(std::max(0, type->findData(key)));
    if (key == QLatin1String("symmetric")) {
      first->setValue(t.value(QStringLiteral("value")).toDouble());
    } else if (key == QLatin1String("deviations") || key == QLatin1String("limits")) {
      const double base = sizes() ? *nominal : 0.0;
      first->setValue(base + t.value(QStringLiteral("upper")).toDouble());
      second->setValue(base + t.value(QStringLiteral("lower")).toDouble());
    } else if (key == QLatin1String("fit")) {
      fit->setCurrentText(t.value(QStringLiteral("code")).toString());
      shown->setChecked(t.value(QStringLiteral("deviations")).toBool());
    }
    follow();
  }

  // The tolerance as the model takes it, null for none; an empty string
  // with the reason in `why` when it is no tolerance.
  QJsonValue value(QString* why) const {
    const QString key = type->currentData().toString();
    if (key == QLatin1String("symmetric")) {
      if (!(first->value() > 0.0)) {
        *why = QObject::tr("A symmetric tolerance is more than 0.");
        return QString();
      }
      return QJsonObject{{QStringLiteral("type"), key}, {QStringLiteral("value"), first->value()}};
    }
    if (key == QLatin1String("deviations") || key == QLatin1String("limits")) {
      if (!(first->value() > second->value())) {
        *why = sizes() ? QObject::tr("The upper limit is more than the lower.")
                       : QObject::tr("The upper deviation is more than the lower.");
        return QString();
      }
      // Limits typed as sizes are kept as deviations from the value, to
      // the fields' precision (20.1 - 20 is 0.1, not 0.10000000000000142).
      const double base = sizes() ? *nominal : 0.0;
      const double precision = std::pow(10.0, first->decimals());
      const auto deviation = [base, precision](double typed) {
        return std::round((typed - base) * precision) / precision + 0.0;
      };
      return QJsonObject{{QStringLiteral("type"), key},
                         {QStringLiteral("upper"), deviation(first->value())},
                         {QStringLiteral("lower"), deviation(second->value())}};
    }
    if (key == QLatin1String("fit")) {
      const QString code = fit->currentText().trimmed();
      if (code.isEmpty()) {
        *why = QObject::tr("Give a fit code, as H7 or g6.");
        return QString();
      }
      QJsonObject t{{QStringLiteral("type"), key}, {QStringLiteral("code"), code}};
      if (shown->isChecked()) {
        t.insert(QStringLiteral("deviations"), true);
      }
      return t;
    }
    return QJsonValue(QJsonValue::Null);
  }
};

// OK and Cancel under a form, with a message line for what is wrong.
struct Buttons {
  QLabel* message = nullptr;
  QDialogButtonBox* box = nullptr;
};

Buttons addButtons(QDialog& dialog, QVBoxLayout* layout) {
  Buttons buttons;
  buttons.message = new QLabel(&dialog);
  buttons.message->setWordWrap(true);
  setErrorStyleSheet(buttons.message);
  buttons.message->hide();
  layout->addWidget(buttons.message);
  buttons.box = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
  QObject::connect(buttons.box, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons.box, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons.box);
  return buttons;
}

void showMessage(const Buttons& buttons, const QString& text) {
  buttons.message->setText(text);
  buttons.message->setVisible(!text.isEmpty());
}

// A colour picker of the named colours, the annotation's own when it is
// another (kept, mitcad#902), and Custom… (the colour dialog).
QComboBox* colourBox(QWidget* parent, const QString& current) {
  auto* box = new QComboBox(parent);
  for (const auto& [name, hex] : annotationColours()) {
    box->addItem(name, hex);
  }
  if (box->findData(current) < 0 && !current.isEmpty()) {
    box->addItem(current, current);
  }
  box->addItem(QObject::tr("Custom…"), QString());
  box->setCurrentIndex(std::max(0, box->findData(current)));
  QObject::connect(box, &QComboBox::activated, parent, [box, parent, current](int index) {
    if (!box->itemData(index).toString().isEmpty()) {
      return;
    }
    // The colour dialog from the annotation's own colour (black for Auto).
    const QColor own(current);
    const QColor picked =
        dialogColor(own.isValid() ? own : QColor(0x21, 0x25, 0x29), parent, QObject::tr("Annotation Color"));
    if (!picked.isValid()) {
      box->setCurrentIndex(0);
      return;
    }
    const int existing = box->findData(picked.name());
    if (existing >= 0) {
      box->setCurrentIndex(existing);
    } else {
      box->insertItem(index, picked.name(), picked.name());
      box->setCurrentIndex(index);
    }
  });
  return box;
}

// ISO 1101's characteristics in words, by the model's keys.
const std::pair<const char*, const char*> kCharacteristics[] = {
    {"straightness", QT_TRANSLATE_NOOP("Annotations", "Straightness")},
    {"flatness", QT_TRANSLATE_NOOP("Annotations", "Flatness")},
    {"circularity", QT_TRANSLATE_NOOP("Annotations", "Circularity")},
    {"cylindricity", QT_TRANSLATE_NOOP("Annotations", "Cylindricity")},
    {"line_profile", QT_TRANSLATE_NOOP("Annotations", "Profile of a line")},
    {"surface_profile", QT_TRANSLATE_NOOP("Annotations", "Profile of a surface")},
    {"parallelism", QT_TRANSLATE_NOOP("Annotations", "Parallelism")},
    {"perpendicularity", QT_TRANSLATE_NOOP("Annotations", "Perpendicularity")},
    {"angularity", QT_TRANSLATE_NOOP("Annotations", "Angularity")},
    {"position", QT_TRANSLATE_NOOP("Annotations", "Position")},
    {"concentricity", QT_TRANSLATE_NOOP("Annotations", "Concentricity")},
    {"symmetry", QT_TRANSLATE_NOOP("Annotations", "Symmetry")},
    {"circular_runout", QT_TRANSLATE_NOOP("Annotations", "Circular runout")},
    {"total_runout", QT_TRANSLATE_NOOP("Annotations", "Total runout")}};

QString characteristicName(const QString& key) {
  for (const auto& [own, name] : kCharacteristics) {
    if (key == QLatin1String(own)) {
      return QCoreApplication::translate("Annotations", name);
    }
  }
  return key;
}

// A feature control frame's fields in a form: the characteristic (its
// symbol and name), the zone's width, Ø, the material condition and three
// datums; those its rules (the geometric_characteristics query) leave out
// disabled and emptied.
struct FrameFields {
  QJsonArray characteristics;
  QComboBox* characteristic = nullptr;
  QDoubleSpinBox* width = nullptr;
  QCheckBox* diameter = nullptr;
  QComboBox* modifier = nullptr;
  std::array<QComboBox*, 3> datums{};
  bool touched = false; // a field changed by the user (watch)

  QJsonObject rules() const {
    const int index = characteristic->currentIndex();
    return index >= 0 && index < characteristics.size() ? characteristics.at(index).toObject() : QJsonObject();
  }

  void build(QFormLayout* form, const QJsonArray& list, const QStringList& letters, QWidget* parent) {
    characteristics = list;
    characteristic = new QComboBox(parent);
    for (const QJsonValue& entry : list) {
      const QJsonObject c = entry.toObject();
      const QString key = c.value(QStringLiteral("key")).toString();
      characteristic->addItem(
          QStringLiteral("%1  %2").arg(c.value(QStringLiteral("symbol")).toString(), characteristicName(key)), key);
    }
    form->addRow(QObject::tr("C&haracteristic:"), characteristic);
    width = new QDoubleSpinBox(parent);
    width->setDecimals(4);
    width->setRange(0.0, 1000.0);
    width->setSingleStep(0.01);
    width->setSuffix(QObject::tr(" mm"));
    width->setValue(0.05);
    form->addRow(QObject::tr("&Tolerance:"), width);
    diameter = new QCheckBox(QObject::tr("C&ylindrical zone (Ø)"), parent);
    form->addRow(QString(), diameter);
    modifier = new QComboBox(parent);
    modifier->addItem(QObject::tr("None"), QString());
    modifier->addItem(QObject::tr("Ⓜ Maximum material"), QStringLiteral("M"));
    modifier->addItem(QObject::tr("Ⓛ Least material"), QStringLiteral("L"));
    form->addRow(QObject::tr("&Material condition:"), modifier);
    auto* row = new QHBoxLayout;
    for (QComboBox*& box : datums) {
      box = new QComboBox(parent);
      box->setEditable(true);
      box->addItem(QString());
      box->addItems(letters);
      box->setValidator(new QRegularExpressionValidator(QRegularExpression(QStringLiteral("[A-Za-z]{0,2}")), box));
      row->addWidget(box, 1);
    }
    auto* label = new QLabel(QObject::tr("&Datums:"), parent);
    label->setBuddy(datums[0]);
    form->addRow(label, row);
    QObject::connect(characteristic, &QComboBox::currentIndexChanged, parent, [this] { follow(); });
    follow();
  }

  void follow() {
    const QJsonObject own = rules();
    const bool cylinder = own.value(QStringLiteral("diameter")).toBool();
    diameter->setEnabled(cylinder);
    if (!cylinder) {
      diameter->setChecked(false);
    }
    const bool material = own.value(QStringLiteral("modifier")).toBool();
    modifier->setEnabled(material);
    if (!material) {
      modifier->setCurrentIndex(0);
    }
    const bool none = own.value(QStringLiteral("datums")).toString() == QLatin1String("none");
    for (QComboBox* box : datums) {
      box->setEnabled(!none);
      if (none) {
        box->setCurrentText(QString());
      }
    }
  }

  void set(const QJsonObject& own) {
    if (own.isEmpty()) {
      return;
    }
    characteristic->setCurrentIndex(
        std::max(0, characteristic->findData(own.value(QStringLiteral("characteristic")).toString())));
    width->setValue(own.value(QStringLiteral("value")).toDouble());
    follow();
    diameter->setChecked(own.value(QStringLiteral("diameter")).toBool());
    modifier->setCurrentIndex(std::max(0, modifier->findData(own.value(QStringLiteral("modifier")).toString())));
    const QJsonArray letters = own.value(QStringLiteral("datums")).toArray();
    for (std::size_t i = 0; i < datums.size(); ++i) {
      const auto at = static_cast<qsizetype>(i);
      datums[i]->setCurrentText(at < letters.size() ? letters.at(at).toString() : QString());
    }
  }

  // From now on a change of a field is the user's: the frame is sent
  // only then (shown again and read back it would carry the fields'
  // rounding).
  void watch(QWidget* parent) {
    const auto mark = [this] { touched = true; };
    QObject::connect(characteristic, &QComboBox::currentIndexChanged, parent, mark);
    QObject::connect(width, &QDoubleSpinBox::valueChanged, parent, mark);
    QObject::connect(diameter, &QCheckBox::toggled, parent, mark);
    QObject::connect(modifier, &QComboBox::currentIndexChanged, parent, mark);
    for (QComboBox* box : datums) {
      QObject::connect(box, &QComboBox::currentTextChanged, parent, mark);
    }
  }

  // The frame as the model takes it; empty with the reason in `why` when
  // the fields do not make one.
  QJsonObject frame(QString* why) const {
    const QString key = characteristic->currentData().toString();
    if (key.isEmpty()) {
      *why = QObject::tr("Choose a characteristic.");
      return {};
    }
    if (!(width->value() > 0.0)) {
      *why = QObject::tr("A tolerance zone is wider than 0.");
      return {};
    }
    QJsonObject result{{QStringLiteral("characteristic"), key}, {QStringLiteral("value"), width->value()}};
    if (diameter->isChecked()) {
      result.insert(QStringLiteral("diameter"), true);
    }
    if (const QString material = modifier->currentData().toString(); !material.isEmpty()) {
      result.insert(QStringLiteral("modifier"), material);
    }
    QJsonArray letters;
    QStringList seen;
    for (const QComboBox* box : datums) {
      const QString letter = box->currentText().trimmed().toUpper();
      if (letter.isEmpty()) {
        continue;
      }
      if (seen.contains(letter)) {
        *why = QObject::tr("Datum %1 is referred to twice.").arg(letter);
        return {};
      }
      seen << letter;
      letters.append(letter);
    }
    if (letters.isEmpty() && rules().value(QStringLiteral("datums")).toString() == QLatin1String("required")) {
      *why = QObject::tr("%1 refers to at least one datum.").arg(characteristicName(key));
      return {};
    }
    if (!letters.isEmpty()) {
      result.insert(QStringLiteral("datums"), letters);
    }
    return result;
  }
};

} // namespace

const std::vector<std::pair<QString, QString>>& annotationColours() {
  // A technical drawing's colours (mitcad#902): Auto first, then black and a
  // dark grey; any other through Custom… (the bright presets are gone; a
  // colour an annotation has is kept and offered as it is).
  static const std::vector<std::pair<QString, QString>> colours = {{QObject::tr("Auto"), QStringLiteral("auto")},
                                                                   {QObject::tr("Black"), QStringLiteral("#212529")},
                                                                   {QObject::tr("Dark Gray"), QStringLiteral("#495057")}};
  return colours;
}

std::optional<QString> askAnnotationColour(QWidget* parent, const QString& current, const QString& what) {
  QStringList names;
  QStringList hexes;
  int at = -1;
  for (const auto& [name, hex] : annotationColours()) {
    if (hex == current) {
      at = static_cast<int>(names.size());
    }
    names << name;
    hexes << hex;
  }
  // A colour of its own (an older preset, a custom one): offered as it is.
  if (at < 0 && QColor(current).isValid()) {
    at = static_cast<int>(names.size());
    names << current;
    hexes << current;
  }
  names << QObject::tr("Custom…");
  qDebug().noquote() << QStringLiteral("Annotation Colour dialog opened: %1 %2 (%3)")
                            .arg(what, current, names.join(QStringLiteral(" | ")));
  bool ok = false;
  const QString chosen = dialogItem(parent, QObject::tr("Annotation Color"), QObject::tr("Color of %1:").arg(what),
                                    names, at >= 0 ? at : 0, &ok);
  if (!ok) {
    return std::nullopt;
  }
  const qsizetype index = names.indexOf(chosen);
  QString hex;
  if (index >= 0 && index < hexes.size()) {
    hex = hexes.at(index);
  } else {
    const QColor own(current);
    const QColor picked =
        dialogColor(own.isValid() ? own : QColor(0x21, 0x25, 0x29), parent, QObject::tr("Color of %1").arg(what));
    if (!picked.isValid()) {
      return std::nullopt;
    }
    hex = picked.name();
  }
  qDebug().noquote() << QStringLiteral("Annotation Colour dialog: %1").arg(hex);
  return hex;
}

std::optional<QString> askAnnotationGroup(QWidget* parent, const QStringList& groups, const QString& current,
                                          const QString& what) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Annotation Group"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  auto* group = new QComboBox(&dialog);
  group->setEditable(true);
  group->addItem(QString());
  group->addItems(groups);
  group->setCurrentText(current);
  group->lineEdit()->setPlaceholderText(QObject::tr("No group"));
  form->addRow(QObject::tr("&Group of %1:").arg(what), group);
  const Buttons buttons = addButtons(dialog, layout);
  (void)buttons;
  group->setFocus();
  group->lineEdit()->selectAll();
  qDebug().noquote() << QStringLiteral("Annotation Group dialog opened: %1 '%2' (%3)")
                            .arg(what, current, groups.join(QStringLiteral(" | ")));
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  const QString chosen = group->currentText().trimmed();
  qDebug().noquote() << QStringLiteral("Annotation Group dialog: '%1'").arg(chosen);
  return chosen;
}

std::optional<QJsonValue> askAnnotationDecimals(QWidget* parent, const QJsonValue& current, const QString& what) {
  QStringList items{QObject::tr("Automatic")};
  for (int d = 0; d <= 6; ++d) {
    items << QString::number(d);
  }
  const int at = current.isDouble() ? current.toInt() + 1 : 0;
  qDebug().noquote() << QStringLiteral("Annotation Decimals dialog opened: %1 %2").arg(what, compact(current));
  bool ok = false;
  const QString chosen = dialogItem(parent, QObject::tr("Annotation Decimals"), QObject::tr("Decimals of %1:").arg(what),
                                    items, std::clamp(at, 0, 7), &ok);
  if (!ok) {
    return std::nullopt;
  }
  const qsizetype index = items.indexOf(chosen);
  const QJsonValue decimals = index > 0 ? QJsonValue(static_cast<int>(index - 1)) : QJsonValue(QJsonValue::Null);
  qDebug().noquote() << QStringLiteral("Annotation Decimals dialog: %1").arg(compact(decimals));
  return decimals;
}

std::optional<QJsonValue> askAnnotationTextHeight(QWidget* parent, const QJsonObject& current) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Annotation Text Height"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  const double automatic = current.value(QStringLiteral("automatic")).toDouble();
  const QString autoText = QObject::tr("Auto (%1 mm)").arg(QString::number(automatic));
  auto* height = new QComboBox(&dialog);
  height->setEditable(true);
  height->addItem(autoText);
  for (const QJsonValue& value : current.value(QStringLiteral("series")).toArray()) {
    height->addItem(QStringLiteral("%1 mm").arg(QString::number(value.toDouble())));
  }
  const QJsonValue set = current.value(QStringLiteral("text_height"));
  if (set.isDouble()) {
    const QString own = QStringLiteral("%1 mm").arg(QString::number(set.toDouble()));
    if (height->findText(own) < 0) {
      height->addItem(own);
    }
    height->setCurrentIndex(height->findText(own));
  } else {
    height->setCurrentIndex(0);
  }
  form->addRow(QObject::tr("&Height:"), height);
  auto* note = new QLabel(QObject::tr("Every annotation's text in the design, in the design's millimeters; "
                                      "Auto takes one of 2.5, 3.5, 5, 7 and 10 mm by the model's size."),
                          &dialog);
  note->setWordWrap(true);
  layout->addWidget(note);
  const Buttons buttons = addButtons(dialog, layout);
  height->setFocus();
  qDebug().noquote() << QStringLiteral("Annotation Text Height dialog opened: %1 (%2)")
                            .arg(compact(set), height->currentText());
  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    QString text = height->currentText().trimmed();
    QJsonValue chosen(QJsonValue::Null);
    if (text != autoText && !text.startsWith(QObject::tr("Auto"), Qt::CaseInsensitive)) {
      if (text.endsWith(QStringLiteral("mm"), Qt::CaseInsensitive)) {
        text.chop(2);
      }
      bool number = false;
      const double mm = text.trimmed().replace(QLatin1Char(','), QLatin1Char('.')).toDouble(&number);
      if (!number || !(mm >= 0.1 && mm <= 1000.0)) {
        showMessage(buttons, QObject::tr("A text height is Auto or 0.1 to 1000 mm."));
        continue;
      }
      chosen = mm;
    }
    qDebug().noquote() << QStringLiteral("Annotation Text Height dialog: %1").arg(compact(chosen));
    return chosen;
  }
  return std::nullopt;
}

std::optional<QJsonValue> askAnnotationTolerance(QWidget* parent, const QJsonValue& current, bool angle,
                                                 const QString& what) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Annotation Tolerance"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(new QLabel(what, &dialog));
  auto* form = new QFormLayout;
  layout->addLayout(form);
  ToleranceFields fields;
  fields.build(form, angle, std::nullopt, &dialog);
  fields.set(current);
  const Buttons buttons = addButtons(dialog, layout);
  fields.type->setFocus();
  qDebug().noquote() << QStringLiteral("Annotation Tolerance dialog opened: %1 %2").arg(what, compact(current));
  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    QString why;
    const QJsonValue tolerance = fields.value(&why);
    if (!why.isEmpty()) {
      showMessage(buttons, why);
      continue;
    }
    qDebug().noquote() << QStringLiteral("Annotation Tolerance dialog: %1").arg(compact(tolerance));
    return tolerance;
  }
  return std::nullopt;
}

std::optional<QString> askNoteText(QWidget* parent, const QString& what, const QString& current) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Note"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  form->addRow(QObject::tr("On:"), new QLabel(what, &dialog));
  auto* text = new QPlainTextEdit(current, &dialog);
  text->setTabChangesFocus(true);
  text->setFixedHeight(text->fontMetrics().lineSpacing() * 4 + 12);
  form->addRow(QObject::tr("&Text:"), text);
  const Buttons buttons = addButtons(dialog, layout);
  text->setFocus();
  qDebug().noquote() << QStringLiteral("Note dialog opened: %1").arg(what);
  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    const QString own = text->toPlainText();
    if (own.trimmed().isEmpty()) {
      showMessage(buttons, QObject::tr("A note needs a text."));
      continue;
    }
    qDebug().noquote() << QStringLiteral("Note dialog: %1").arg(QString(own).replace(QLatin1Char('\n'), QStringLiteral(" / ")));
    return own;
  }
  return std::nullopt;
}

std::optional<QString> askDatumLetter(QWidget* parent, const QString& suggested, const QString& what) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Datum Label"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  form->addRow(QObject::tr("On:"), new QLabel(what, &dialog));
  auto* letter = new QLineEdit(suggested, &dialog);
  letter->setValidator(new QRegularExpressionValidator(QRegularExpression(QStringLiteral("[A-Za-z]{0,2}")), letter));
  form->addRow(QObject::tr("&Letter:"), letter);
  const Buttons buttons = addButtons(dialog, layout);
  letter->setFocus();
  letter->selectAll();
  qDebug().noquote() << QStringLiteral("Datum Label dialog opened: %1 %2").arg(what, suggested);
  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    const QString chosen = letter->text().trimmed().toUpper();
    if (chosen.isEmpty()) {
      showMessage(buttons, QObject::tr("A datum label needs a letter."));
      continue;
    }
    qDebug().noquote() << QStringLiteral("Datum Label dialog: %1").arg(chosen);
    return chosen;
  }
  return std::nullopt;
}

std::optional<QJsonObject> askFeatureControlFrame(QWidget* parent, const QJsonArray& characteristics,
                                                  const QJsonObject& current, const QStringList& datums,
                                                  const QString& what) {
  FrameFields fields; // outlives the dialog, whose fields call it
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Feature Control Frame"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  form->addRow(QObject::tr("On:"), new QLabel(what, &dialog));
  fields.build(form, characteristics, datums, &dialog);
  fields.set(current);
  const Buttons buttons = addButtons(dialog, layout);
  fields.characteristic->setFocus();
  qDebug().noquote() << QStringLiteral("Feature Control Frame dialog opened: %1 %2").arg(what, compact(current));
  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    QString why;
    const QJsonObject frame = fields.frame(&why);
    if (!why.isEmpty()) {
      showMessage(buttons, why);
      continue;
    }
    qDebug().noquote() << QStringLiteral("Feature Control Frame dialog: %1").arg(compact(frame));
    return frame;
  }
  return std::nullopt;
}

std::optional<AnnotationEdit> editAnnotation(QWidget* parent, const QJsonObject& annotation,
                                             const AnnotationEditContext& context) {
  const QString name = annotation.value(QStringLiteral("name")).toString();
  const QString kind = annotation.value(QStringLiteral("kind")).toString();
  const bool note = kind == QLatin1String("note");
  const bool datum = kind == QLatin1String("datum");
  const bool isFrame = kind == QLatin1String("frame");
  // A hole callout's text is its hole's (mitcad#902), a material or
  // process note's its body's (mitcad#1049): shown, not edited.
  const bool bodyNote = kind == QLatin1String("material") || kind == QLatin1String("process_note");
  const bool callout = kind == QLatin1String("hole_callout") || bodyNote;
  const bool leader = note || datum || isFrame || callout;
  const QJsonObject format = annotation.value(QStringLiteral("format")).toObject();
  // Outlive the dialog, whose fields call them.
  ToleranceFields tolerance;
  FrameFields frameFields;
  QString requested; // what Pick Again… or Pick… asks to pick in the view
  bool planeCleared = false;
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Edit Annotation"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  const QString error = annotation.value(QStringLiteral("error")).toString();
  if (!error.isEmpty()) {
    // In words (mitcad#1741).
    auto* broken = new QLabel(QObject::tr("Broken: %1").arg(shownError(annotation)), &dialog);
    broken->setWordWrap(true);
    setErrorStyleSheet(broken);
    layout->insertWidget(0, broken);
  }

  auto* nameField = new QLineEdit(name, &dialog);
  form->addRow(QObject::tr("&Name:"), nameField);
  QComboBox* kindBox = nullptr;
  QLineEdit* prefix = nullptr;
  QComboBox* decimals = nullptr;
  QLineEdit* before = nullptr;
  QLineEdit* after = nullptr;
  QLineEdit* replace = nullptr;
  QPlainTextEdit* text = nullptr;
  QLineEdit* letter = nullptr;
  QComboBox* placed = nullptr;
  const QString ownPrefix = format.contains(QStringLiteral("prefix")) ? format.value(QStringLiteral("prefix")).toString()
                                                                      : defaultPrefix(kind);
  const QJsonObject ownFrame = annotation.value(QStringLiteral("frame")).toObject();
  const QString ownOn = annotation.value(QStringLiteral("on")).toString();
  if (note) {
    text = new QPlainTextEdit(annotation.value(QStringLiteral("text")).toString(), &dialog);
    text->setTabChangesFocus(true);
    text->setFixedHeight(text->fontMetrics().lineSpacing() * 4 + 12);
    form->addRow(QObject::tr("&Text:"), text);
  } else if (datum) {
    letter = new QLineEdit(annotation.value(QStringLiteral("letter")).toString(), &dialog);
    letter->setValidator(new QRegularExpressionValidator(QRegularExpression(QStringLiteral("[A-Za-z]{0,2}")), letter));
    form->addRow(QObject::tr("&Letter:"), letter);
  } else if (isFrame) {
    frameFields.build(form, context.characteristics, context.datums, &dialog);
    frameFields.set(ownFrame);
    frameFields.watch(&dialog);
    // On its leader, or in a dimension's place under its line.
    placed = new QComboBox(&dialog);
    placed->addItem(QObject::tr("On its leader"), QString());
    for (const QString& dimension : context.dimensions) {
      placed->addItem(QObject::tr("Under %1").arg(dimension), dimension);
    }
    if (!ownOn.isEmpty() && placed->findData(ownOn) < 0) {
      placed->addItem(QObject::tr("Under %1").arg(ownOn), ownOn);
    }
    placed->setCurrentIndex(std::max(0, placed->findData(ownOn)));
    form->addRow(QObject::tr("&Placed:"), placed);
  } else if (callout) {
    auto* written = new QLabel(annotation.value(QStringLiteral("label")).toObject().value(QStringLiteral("text")).toString(),
                               &dialog);
    written->setToolTip(bodyNote ? QObject::tr("Written from the body: it follows its material and print settings")
                                 : QObject::tr("Written from the hole's feature: it follows the hole's size, depth and thread"));
    form->addRow(bodyNote ? QObject::tr("Text:") : QObject::tr("Callout:"), written);
  } else {
    // The kinds its picks have now.
    const QJsonObject values = annotation.value(QStringLiteral("values")).toObject();
    if (values.size() > 1) {
      kindBox = new QComboBox(&dialog);
      for (const auto& [key, label] : kKinds) {
        if (values.contains(QLatin1String(key))) {
          kindBox->addItem(QCoreApplication::translate("Annotations", label), QString::fromLatin1(key));
        }
      }
      kindBox->setCurrentIndex(std::max(0, kindBox->findData(kind)));
      form->addRow(QObject::tr("&Kind:"), kindBox);
    } else {
      form->addRow(QObject::tr("Kind:"), new QLabel(kindName(kind), &dialog));
    }
    prefix = new QLineEdit(ownPrefix, &dialog);
    form->addRow(QObject::tr("&Prefix:"), prefix);
    decimals = new QComboBox(&dialog);
    decimals->addItem(QObject::tr("Automatic"));
    for (int d = 0; d <= 6; ++d) {
      decimals->addItem(QString::number(d), d);
    }
    const QJsonValue ownDecimals = format.value(QStringLiteral("decimals"));
    decimals->setCurrentIndex(ownDecimals.isDouble() ? ownDecimals.toInt() + 1 : 0);
    form->addRow(QObject::tr("&Decimals:"), decimals);
    before = new QLineEdit(format.value(QStringLiteral("before")).toString(), &dialog);
    form->addRow(QObject::tr("Text &before:"), before);
    after = new QLineEdit(format.value(QStringLiteral("after")).toString(), &dialog);
    form->addRow(QObject::tr("Text &after:"), after);
    replace = new QLineEdit(format.value(QStringLiteral("replace")).toString(), &dialog);
    replace->setPlaceholderText(QObject::tr("The value"));
    form->addRow(QObject::tr("&Replace value:"), replace);
    // Limits typed as the sizes they allow, from the value of the kind
    // shown (mm, or degrees for an angle).
    const auto nominalOf = [values](const QString& shownKind) -> std::optional<double> {
      const QJsonValue measured = values.value(shownKind);
      if (!measured.isDouble()) {
        return std::nullopt;
      }
      return shownKind == QLatin1String("angle") ? measured.toDouble() * 180.0 / 3.14159265358979323846
                                                 : measured.toDouble();
    };
    const QJsonValue ownTolerance = annotation.value(QStringLiteral("tolerance"));
    tolerance.build(form, kind == QLatin1String("angle"), nominalOf(kind), &dialog);
    tolerance.set(ownTolerance);
    tolerance.watch(&dialog);
    if (kindBox != nullptr) {
      QObject::connect(kindBox, &QComboBox::currentIndexChanged, &dialog,
                       [&tolerance, kindBox, nominalOf, ownTolerance, originalAngle = kind == QLatin1String("angle")] {
                         const QString chosen = kindBox->currentData().toString();
                         tolerance.retarget(chosen == QLatin1String("angle"), nominalOf(chosen), ownTolerance,
                                            originalAngle);
                       });
    }
  }

  // The picks and the plane, each picked again in the view on a click of
  // its button: the dialog closes for the pick and opens again after it.
  const auto described = [&context](const QJsonValue& pick) {
    return context.describe ? context.describe(pick) : compact(pick);
  };
  std::vector<std::pair<QString, QPushButton*>> pickButtons;
  // Under a dimension a frame stands in the dimension's place: it shows no
  // pick or plane of its own (Placed says where it stands).
  const bool underDimension = isFrame && !ownOn.isEmpty();
  const auto addPick = [&](const QString& label, const QString& shown, const QString& button, const QString& field) {
    auto* row = new QHBoxLayout;
    auto* what = new QLabel(shown, &dialog);
    what->setWordWrap(true);
    row->addWidget(what, 1);
    auto* again = new QPushButton(button, &dialog);
    again->setAutoDefault(false);
    again->setEnabled(context.canPick);
    QObject::connect(again, &QPushButton::clicked, &dialog, [&dialog, &requested, field] {
      requested = field;
      dialog.accept();
    });
    row->addWidget(again);
    form->addRow(label, row);
    pickButtons.emplace_back(field, again);
    return row;
  };
  QHBoxLayout* firstRow = addPick(leader ? QObject::tr("Points at:") : QObject::tr("First pick:"),
                                  described(annotation.value(QStringLiteral("a"))), QObject::tr("Pick Again…"),
                                  QStringLiteral("a"));
  if (annotation.contains(QStringLiteral("b"))) {
    addPick(QObject::tr("Second pick:"), described(annotation.value(QStringLiteral("b"))), QObject::tr("Pick Again…"),
            QStringLiteral("b"));
  }
  // A plane picked (a planar face or a datum plane), else the one the view
  // gave when it was kept, else the model's own; no row when every kind
  // offered lies in a plane of its own (a radius, a diameter, an angle).
  bool planeOffered = !liesInOwnPlane(kind);
  if (kindBox != nullptr) {
    for (int i = 0; i < kindBox->count(); ++i) {
      planeOffered = planeOffered || !liesInOwnPlane(kindBox->itemData(i).toString());
    }
  }
  const QJsonObject plane = annotation.value(QStringLiteral("plane")).toObject();
  const QString ownPlaneText = plane.contains(QStringLiteral("pick")) ? described(plane.value(QStringLiteral("pick")))
                               : plane.contains(QStringLiteral("normal"))
                                   ? QObject::tr("Square to the view it was kept in")
                                   : QObject::tr("Automatic");
  QHBoxLayout* planeRow = nullptr;
  QLabel* shownPlane = nullptr;
  QPushButton* planePick = nullptr;
  QPushButton* planeAutomatic = nullptr;
  if (planeOffered) {
    planeRow = addPick(QObject::tr("Plane:"), ownPlaneText, QObject::tr("Pick…"), QStringLiteral("plane"));
    shownPlane = static_cast<QLabel*>(planeRow->itemAt(0)->widget());
    planePick = pickButtons.back().second;
    if (!plane.isEmpty()) {
      planeAutomatic = new QPushButton(QObject::tr("Automatic"), &dialog);
      planeAutomatic->setAutoDefault(false);
      QObject::connect(planeAutomatic, &QPushButton::clicked, &dialog, [&planeCleared, planeAutomatic, shownPlane] {
        planeCleared = true;
        shownPlane->setText(QObject::tr("Automatic"));
        planeAutomatic->setEnabled(false);
      });
      planeRow->addWidget(planeAutomatic);
      pickButtons.emplace_back(QStringLiteral("automatic plane"), planeAutomatic);
    }
  }
  // Another kind chosen: the plane row disabled for one in its own plane
  // (the model drops the plane then), enabled again for one that takes it.
  const auto followKind = [&context, &planeCleared, kindBox, shownPlane, planePick, planeAutomatic, ownPlaneText] {
    if (kindBox == nullptr || planePick == nullptr) {
      return;
    }
    const QString chosen = kindBox->currentData().toString();
    const bool takes = !liesInOwnPlane(chosen);
    planePick->setEnabled(context.canPick && takes);
    if (planeAutomatic != nullptr) {
      planeAutomatic->setEnabled(takes && !planeCleared);
    }
    shownPlane->setEnabled(takes);
    if (takes) {
      shownPlane->setText(planeCleared ? QObject::tr("Automatic") : ownPlaneText);
    } else {
      shownPlane->setText(chosen == QLatin1String("angle") ? QObject::tr("The plane of its sides")
                                                           : QObject::tr("The plane of its circle"));
    }
  };
  followKind();
  // A frame's rows by where Placed puts it: its pick on its leader, none
  // under a dimension. One that stood under a dimension as the editor
  // opened shows its plane only once it stands on its leader (the editor
  // opens again after the pick that puts it there).
  const auto followPlaced = [form, placed, firstRow, planeRow, underDimension] {
    if (placed == nullptr) {
      return;
    }
    const bool onLeader = placed->currentData().toString().isEmpty();
    form->setRowVisible(firstRow, onLeader);
    if (planeRow != nullptr) {
      form->setRowVisible(planeRow, onLeader && !underDimension);
    }
  };
  followPlaced();

  const QString colour = annotation.value(QStringLiteral("color")).toString();
  QComboBox* colourField = colourBox(&dialog, colour);
  form->addRow(QObject::tr("&Color:"), colourField);
  auto* group = new QComboBox(&dialog);
  group->setEditable(true);
  group->addItem(QString());
  group->addItems(context.groups);
  const QString ownGroup = annotation.value(QStringLiteral("group")).toString();
  group->setCurrentText(ownGroup);
  group->lineEdit()->setPlaceholderText(QObject::tr("No group"));
  form->addRow(QObject::tr("&Group:"), group);
  const bool visible = annotation.value(QStringLiteral("visible")).toBool(true);
  auto* shownBox = new QCheckBox(QObject::tr("&Shown"), &dialog);
  shownBox->setChecked(visible);
  form->addRow(QString(), shownBox);
  const Buttons buttons = addButtons(dialog, layout);
  nameField->setFocus();
  nameField->selectAll();
  dialog.resize(460, dialog.sizeHint().height());
  qDebug().noquote() << QStringLiteral("Edit Annotation dialog opened: %1 %2").arg(name, kind);
  // For UI tests: where the pick buttons shown are, in the main window's
  // coordinates (" (disabled)" after one disabled), once the dialog shows
  // and again once it has settled (a window manager may still place it) or
  // its rows change; logged when they change, by watched timers (TestSync)
  // so that a test's sync waits for them.
  QString loggedPicks;
  const auto logPicks = [&dialog, &pickButtons, &loggedPicks, parent] {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    // The main window: the parent's, or the Annotations window's parent's.
    const QWidget* own = parent != nullptr ? parent->window() : nullptr;
    const QWidget* top = own != nullptr && own->parentWidget() != nullptr ? own->parentWidget()->window() : own;
    const QPoint origin = top != nullptr ? top->mapToGlobal(QPoint(0, 0)) : QPoint();
    QStringList lines;
    for (const auto& [field, button] : pickButtons) {
      if (!button->isVisibleTo(&dialog)) {
        continue; // a row left out (a frame under a dimension)
      }
      const QPoint at = button->mapToGlobal(button->rect().center()) - origin;
      lines << QStringLiteral("Edit Annotation pick %1 at %2,%3%4")
                   .arg(field)
                   .arg(at.x())
                   .arg(at.y())
                   .arg(button->isEnabled() ? QString() : QStringLiteral(" (disabled)"));
    }
    const QString joined = lines.join(QLatin1Char('\n'));
    if (joined != loggedPicks) {
      loggedPicks = joined;
      for (const QString& line : std::as_const(lines)) {
        qDebug().noquote() << line;
      }
    }
  };
  for (const int delay : {0, 150, 400}) {
    TestSync::singleShot(delay, &dialog, logPicks);
  }
  if (kindBox != nullptr) {
    QObject::connect(kindBox, &QComboBox::currentIndexChanged, &dialog, [&dialog, followKind, logPicks] {
      followKind();
      TestSync::singleShot(0, &dialog, logPicks);
    });
  }
  if (placed != nullptr) {
    QObject::connect(placed, &QComboBox::currentIndexChanged, &dialog, [&dialog, followPlaced, logPicks] {
      followPlaced();
      TestSync::singleShot(0, &dialog, logPicks);
    });
  }

  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    QString pick = requested;
    requested.clear();
    QJsonObject change;
    QJsonObject withPick;
    if (nameField->text().trimmed() != name) {
      change.insert(QStringLiteral("new_name"), nameField->text().trimmed());
    }
    if (kindBox != nullptr && kindBox->currentData().toString() != kind) {
      change.insert(QStringLiteral("kind"), kindBox->currentData().toString());
    }
    if (note) {
      const QString own = text->toPlainText();
      if (own.trimmed().isEmpty()) {
        showMessage(buttons, QObject::tr("A note needs a text."));
        continue;
      }
      if (own != annotation.value(QStringLiteral("text")).toString()) {
        change.insert(QStringLiteral("text"), own);
      }
    } else if (datum) {
      const QString own = letter->text().trimmed().toUpper();
      if (own.isEmpty()) {
        showMessage(buttons, QObject::tr("A datum label needs a letter."));
        continue;
      }
      if (own != annotation.value(QStringLiteral("letter")).toString()) {
        change.insert(QStringLiteral("letter"), own);
      }
    } else if (isFrame) {
      // The frame only when its fields were changed.
      if (frameFields.touched) {
        QString why;
        const QJsonObject chosen = frameFields.frame(&why);
        if (!why.isEmpty()) {
          showMessage(buttons, why);
          continue;
        }
        if (chosen != ownFrame) {
          change.insert(QStringLiteral("frame"), chosen);
        }
      }
      const QString on = placed->currentData().toString();
      if (on != ownOn && on.isEmpty() && (pick == QLatin1String("a") || context.leaderNeedsPick)) {
        // Taken from under its dimension onto a leader with a new pick:
        // `on: null` goes with that pick in one edit, which the model
        // checks with the new pick. One whose pick is no feature (a
        // vertex, a point, a body) is asked for the pick first.
        if (!context.canPick) {
          showMessage(buttons, QObject::tr("On its leader %1 needs a face, an edge or an axis to point at: pick it "
                                           "once no command or sketch is open.")
                                   .arg(name));
          continue;
        }
        withPick.insert(QStringLiteral("on"), QJsonValue(QJsonValue::Null));
        pick = QStringLiteral("a");
      } else if (on != ownOn) {
        change.insert(QStringLiteral("on"), on.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(on));
      }
    } else if (callout) {
      // Nothing of its own to change: its text is its hole's.
    } else {
      QJsonObject formatChange;
      if (prefix->text() != ownPrefix) {
        formatChange.insert(QStringLiteral("prefix"), prefix->text() == defaultPrefix(kind)
                                                          ? QJsonValue(QJsonValue::Null)
                                                          : QJsonValue(prefix->text()));
      }
      const QJsonValue chosenDecimals =
          decimals->currentIndex() > 0 ? QJsonValue(decimals->currentIndex() - 1) : QJsonValue(QJsonValue::Null);
      if (chosenDecimals != format.value(QStringLiteral("decimals")) &&
          !(chosenDecimals.isNull() && !format.contains(QStringLiteral("decimals")))) {
        formatChange.insert(QStringLiteral("decimals"), chosenDecimals);
      }
      const std::pair<const char*, QLineEdit*> texts[] = {{"before", before}, {"after", after}};
      for (const auto& [key, field] : texts) {
        if (field->text().trimmed() != format.value(QLatin1String(key)).toString()) {
          formatChange.insert(QLatin1String(key), field->text().trimmed());
        }
      }
      if (replace->text().trimmed() != format.value(QStringLiteral("replace")).toString()) {
        formatChange.insert(QStringLiteral("replace"), replace->text().trimmed().isEmpty()
                                                           ? QJsonValue(QJsonValue::Null)
                                                           : QJsonValue(replace->text().trimmed()));
      }
      if (!formatChange.isEmpty()) {
        change.insert(QStringLiteral("format"), formatChange);
      }
      // The tolerance only when its fields were changed: shown again and
      // read back it would come back with the fields' rounding.
      if (tolerance.edited()) {
        QString why;
        const QJsonValue chosenTolerance = tolerance.value(&why);
        if (!why.isEmpty()) {
          showMessage(buttons, why);
          continue;
        }
        const QJsonValue ownTolerance = annotation.value(QStringLiteral("tolerance"));
        if (chosenTolerance != ownTolerance && !(chosenTolerance.isNull() && ownTolerance.isUndefined())) {
          change.insert(QStringLiteral("tolerance"), chosenTolerance);
        }
      }
    }
    if (planeCleared && pick != QLatin1String("plane")) {
      change.insert(QStringLiteral("plane"), QJsonValue(QJsonValue::Null));
      // Automatic (mitcad#894): a length's dimension line placed by the
      // model again too, outside its bodies, as its plane is chosen.
      if (annotation.contains(QStringLiteral("line_offset"))) {
        change.insert(QStringLiteral("line_offset"), QJsonValue(QJsonValue::Null));
      }
    }
    const QString chosenColour = colourField->currentData().toString();
    if (!chosenColour.isEmpty() && chosenColour != colour) {
      change.insert(QStringLiteral("color"), chosenColour);
    }
    const QString chosenGroup = group->currentText().trimmed();
    if (chosenGroup != ownGroup) {
      change.insert(QStringLiteral("group"),
                    chosenGroup.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(chosenGroup));
    }
    if (shownBox->isChecked() != visible) {
      change.insert(QStringLiteral("visible"), shownBox->isChecked());
    }
    QString picking;
    if (!pick.isEmpty()) {
      picking = withPick.isEmpty() ? QStringLiteral(" (pick %1)").arg(pick)
                                   : QStringLiteral(" (pick %1 with %2)").arg(pick, compact(withPick));
    }
    qDebug().noquote() << QStringLiteral("Edit Annotation dialog: %1%2").arg(compact(change), picking);
    if (change.isEmpty() && pick.isEmpty()) {
      return std::nullopt;
    }
    return AnnotationEdit{change, pick, withPick};
  }
  return std::nullopt;
}

} // namespace mitcad
