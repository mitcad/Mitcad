// SPDX-License-Identifier: MIT
#include "SettingField.hpp"

#include <algorithm>
#include <cmath>
#include <initializer_list>
#include <optional>
#include <utility>

#include <QBrush>
#include <QColor>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFontMetrics>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QIcon>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QPainter>
#include <QPalette>
#include <QPen>
#include <QPixmap>
#include <QPushButton>
#include <QRectF>
#include <QSignalBlocker>
#include <QStringList>
#include <QTableWidget>
#include <QTableWidgetItem>
#include <QToolButton>
#include <QVBoxLayout>
#include <QWheelEvent>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/Theme.hpp"
#include "../platform/NativeDialogs.hpp"

namespace mitcad {
namespace {

// The fields' widths (mitcad#655): every field of a form takes the same
// width, so that the ↺ after them make one column; a number's unit, a
// label or a [mm | %] box, the same width.
constexpr int kEditorWidth = 240;
constexpr int kNumberWidth = 150;
constexpr int kUnitWidth = 80;

// A drop-down that leaves the wheel to the form that scrolls unless it has
// the keyboard (mitcad#655).
class WheelSafeComboBox : public QComboBox {
public:
  WheelSafeComboBox() { setFocusPolicy(Qt::StrongFocus); }

protected:
  void wheelEvent(QWheelEvent* event) override {
    if (hasFocus()) {
      QComboBox::wheelEvent(event);
    } else {
      event->ignore();
    }
  }
};

// One slicer's preset field (mitcad#1062): an editable drop-down whose
// list is asked for each time it opens (the installed slicer's presets),
// the typed text kept.
class PresetComboBox : public WheelSafeComboBox {
public:
  std::function<QStringList()> names;

  void showPopup() override {
    if (names) {
      const QString typed = currentText();
      const QSignalBlocker block(this);
      clear();
      addItems(names());
      setEditText(typed);
    }
    QComboBox::showPopup();
  }
};

std::optional<double> numberOf(const QJsonValue& value) {
  if (value.isDouble()) {
    return value.toDouble();
  }
  QString text = value.toVariant().toString().trimmed();
  if (text.endsWith(QLatin1Char('%'))) {
    text.chop(1);
  }
  return parsePlainNumber(text);
}

// The colour field (mitcad#655), drawn as the other fields (a white fill,
// their border, the focus colour): a swatch of its height at the left,
// solid with a neutral border (a checker only under transparency), and
// after it the words, grey as a placeholder while unset. A click opens the
// colour picker.
class ColorFieldButton : public QPushButton {
public:
  explicit ColorFieldButton(const QString& role) : m_role(role) {
    setAutoDefault(false);
    setFocusPolicy(Qt::StrongFocus);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  }

  void setColor(const QColor& color, const QString& words, bool placeholder) {
    m_color = color;
    m_words = words;
    m_placeholder = placeholder;
    setProperty(NativeColorFieldProperty, color);
    setProperty(NativeColorFieldTextProperty, words);
    setProperty(NativeColorFieldPlaceholderProperty, placeholder);
    // A generic button value does not describe this custom painting. Its
    // name also identifies the field and the visible colour's source.
    // Keep the custom-painted swatch and words unchanged.
    const QString value = placeholder && color.isValid() ? tr("%1, inherited from %2").arg(color.name(), words)
                          : color.isValid() ? color.name()
                          : words.isEmpty() ? tr("Unset") : words;
    setAccessibleName(tr("%1: %2").arg(m_role, value));
    update();
  }

  QSize sizeHint() const override { return QSize(150, std::max(fontMetrics().height() + 8, 22)); }
  QSize minimumSizeHint() const override { return QSize(60, sizeHint().height()); }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QRectF frame = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
    painter.setPen(QPen(palette().color(hasFocus() ? QPalette::Highlight : QPalette::Mid), 1));
    painter.setBrush(palette().color(isEnabled() ? QPalette::Base : QPalette::Window));
    painter.drawRoundedRect(frame, 3.0, 3.0);
    const int side = std::max(8, height() - 8);
    const QRect swatch(4, 4, side + 8, side);
    if (!m_color.isValid() || m_color.alpha() < 255) {
      const int cell = 3;
      for (int y = 0; y * cell < swatch.height(); ++y) {
        for (int x = 0; x * cell < swatch.width(); ++x) {
          const QRect square(swatch.left() + x * cell, swatch.top() + y * cell, cell, cell);
          painter.fillRect(square.intersected(swatch), (x + y) % 2 == 0 ? QColor(200, 200, 200) : QColor(255, 255, 255));
        }
      }
    }
    painter.setBrush(m_color.isValid() ? QBrush(m_color) : QBrush(Qt::NoBrush));
    painter.setPen(QPen(QColor(140, 140, 140), 1));
    painter.drawRoundedRect(QRectF(swatch).adjusted(0.5, 0.5, -0.5, -0.5), 2.0, 2.0);
    const QRect words = rect().adjusted(swatch.right() + 7, 0, -4, 0);
    painter.setPen(palette().color(m_placeholder || !isEnabled() ? QPalette::PlaceholderText : QPalette::Text));
    painter.drawText(words, Qt::AlignLeft | Qt::AlignVCenter,
                     fontMetrics().elidedText(m_words, Qt::ElideRight, std::max(0, words.width())));
  }

private:
  QString m_role;
  QColor m_color;
  QString m_words;
  bool m_placeholder = false;
};

// Whether an outline's points are an axis-aligned rectangle (four corners
// of their bounds, in any order).
bool isRectangle(const QJsonArray& points, double minX, double minY, double maxX, double maxY) {
  if (points.size() != 4 || maxX <= minX || maxY <= minY) {
    return false;
  }
  for (const QJsonValue& value : points) {
    const double x = value.toArray().at(0).toDouble();
    const double y = value.toArray().at(1).toDouble();
    const bool cornerX = std::abs(x - minX) < 1e-9 || std::abs(x - maxX) < 1e-9;
    const bool cornerY = std::abs(y - minY) < 1e-9 || std::abs(y - maxY) < 1e-9;
    if (!cornerX || !cornerY) {
      return false;
    }
  }
  return true;
}

// Edit… of an outline (the bed's shape, excluded areas; mitcad#655): a
// rectangle as its width, depth and origin, any other shape as a table of
// points (X, Y), never as raw text. Shape switches between the two.
class OutlineDialog : public QDialog {
public:
  OutlineDialog(const PrintKey& key, const QJsonValue& points, QWidget* parent) : QDialog(parent) {
    setWindowTitle(key.name);
    auto* layout = new QVBoxLayout(this);
    const QJsonArray given = points.toArray();
    double minX = 0.0;
    double minY = 0.0;
    double maxX = 0.0;
    double maxY = 0.0;
    bool first = true;
    for (const QJsonValue& value : given) {
      const double x = value.toArray().at(0).toDouble();
      const double y = value.toArray().at(1).toDouble();
      minX = first ? x : std::min(minX, x);
      minY = first ? y : std::min(minY, y);
      maxX = first ? x : std::max(maxX, x);
      maxY = first ? y : std::max(maxY, y);
      first = false;
    }
    auto* shapeRow = new QFormLayout;
    m_shape = new QComboBox;
    m_shape->addItem(tr("Rectangle"));
    m_shape->addItem(tr("Points"));
    shapeRow->addRow(tr("Shape"), m_shape);
    layout->addLayout(shapeRow);

    m_rectangle = new QWidget;
    auto* form = new QFormLayout(m_rectangle);
    form->setContentsMargins(0, 0, 0, 0);
    const auto box = [](double value) {
      auto* spin = new DecimalSpinBox;
      spin->setRange(-100000.0, 100000.0);
      spin->setDecimals(3);
      spin->setSuffix(QStringLiteral(" mm"));
      spin->setValue(value);
      return spin;
    };
    m_width = box(maxX - minX);
    m_depth = box(maxY - minY);
    m_x = box(minX);
    m_y = box(minY);
    form->addRow(tr("Width"), m_width);
    form->addRow(tr("Depth"), m_depth);
    form->addRow(tr("Origin X"), m_x);
    form->addRow(tr("Origin Y"), m_y);
    layout->addWidget(m_rectangle);

    m_pointsPage = new QWidget;
    auto* pointsBox = new QVBoxLayout(m_pointsPage);
    pointsBox->setContentsMargins(0, 0, 0, 0);
    m_table = new QTableWidget(0, 2);
    m_table->setHorizontalHeaderLabels({tr("X (mm)"), tr("Y (mm)")});
    m_table->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    m_table->verticalHeader()->setVisible(false);
    for (const QJsonValue& value : given) {
      addPoint(value.toArray().at(0).toDouble(), value.toArray().at(1).toDouble());
    }
    pointsBox->addWidget(m_table);
    auto* rowButtons = new QHBoxLayout;
    auto* add = new QPushButton(tr("Add Point"));
    auto* remove = new QPushButton(tr("Remove Point"));
    for (QPushButton* button : {add, remove}) {
      button->setAutoDefault(false);
      rowButtons->addWidget(button);
    }
    rowButtons->addStretch(1);
    pointsBox->addLayout(rowButtons);
    layout->addWidget(m_pointsPage);

    m_error = new QLabel;
    m_error->setWordWrap(true);
    setErrorStyleSheet(m_error);
    m_error->hide();
    layout->addWidget(m_error);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    layout->addWidget(buttons);

    const bool rectangle = given.isEmpty() || isRectangle(given, minX, minY, maxX, maxY);
    m_shape->setCurrentIndex(rectangle ? 0 : 1);
    showShape();
    connect(m_shape, &QComboBox::currentIndexChanged, this, [this] {
      if (m_shape->currentIndex() == 1 && m_table->rowCount() == 0) {
        // The rectangle's corners to start from.
        const double x = m_x->value();
        const double y = m_y->value();
        const double w = m_width->value();
        const double d = m_depth->value();
        addPoint(x, y);
        addPoint(x + w, y);
        addPoint(x + w, y + d);
        addPoint(x, y + d);
      }
      showShape();
    });
    connect(add, &QPushButton::clicked, this, [this] { addPoint(0.0, 0.0); });
    connect(remove, &QPushButton::clicked, this, [this] {
      const int row = m_table->currentRow() >= 0 ? m_table->currentRow() : m_table->rowCount() - 1;
      if (row >= 0) {
        m_table->removeRow(row);
      }
    });
    connect(buttons, &QDialogButtonBox::accepted, this, &OutlineDialog::take);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  }

  QJsonValue points() const { return m_value; }

private:
  void addPoint(double x, double y) {
    const int row = m_table->rowCount();
    m_table->insertRow(row);
    m_table->setItem(row, 0, new QTableWidgetItem(printNumberText(x)));
    m_table->setItem(row, 1, new QTableWidgetItem(printNumberText(y)));
  }

  void showShape() {
    m_rectangle->setVisible(m_shape->currentIndex() == 0);
    m_pointsPage->setVisible(m_shape->currentIndex() == 1);
  }

  void fail(const QString& message) {
    m_error->setText(message);
    m_error->show();
  }

  void take() {
    QJsonArray outline;
    if (m_shape->currentIndex() == 0) {
      const double x = m_x->value();
      const double y = m_y->value();
      const double w = m_width->value();
      const double d = m_depth->value();
      if (w <= 0.0 || d <= 0.0) {
        fail(tr("Use a width and a depth above 0 mm."));
        return;
      }
      outline = QJsonArray{QJsonArray{x, y}, QJsonArray{x + w, y}, QJsonArray{x + w, y + d}, QJsonArray{x, y + d}};
    } else {
      for (int row = 0; row < m_table->rowCount(); ++row) {
        const QTableWidgetItem* xItem = m_table->item(row, 0);
        const QTableWidgetItem* yItem = m_table->item(row, 1);
        const std::optional<double> x = parsePlainNumber(xItem != nullptr ? xItem->text() : QString());
        const std::optional<double> y = parsePlainNumber(yItem != nullptr ? yItem->text() : QString());
        if (!x || !y) {
          fail(tr("Use a number for X and Y of point %1.").arg(row + 1));
          return;
        }
        outline.append(QJsonArray{*x, *y});
      }
      if (outline.size() < 3) {
        fail(tr("Use at least three points."));
        return;
      }
    }
    m_value = outline;
    accept();
  }

  QComboBox* m_shape = nullptr;
  QWidget* m_rectangle = nullptr;
  QWidget* m_pointsPage = nullptr;
  QDoubleSpinBox* m_width = nullptr;
  QDoubleSpinBox* m_depth = nullptr;
  QDoubleSpinBox* m_x = nullptr;
  QDoubleSpinBox* m_y = nullptr;
  QTableWidget* m_table = nullptr;
  QLabel* m_error = nullptr;
  QJsonValue m_value;
};

} // namespace

SettingField::SettingField(const PrintKey& key, QWidget* parent, bool takesPercent, const QString& slicer)
    : QWidget(parent), m_key(key), m_slicer(key.perDialect ? slicer : QString()), m_takesPercent(takesPercent) {
  // A value per slicer is never typed as a whole: the slicers' presets
  // name it, one slicer's field each (mitcad#1062).
  m_readOnly = key.perDialect && m_slicer.isEmpty();
  build();
  showValue();
  updateState();
}

void SettingField::build() {
  // The field and ↺ on a line, a refusal under them.
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(1);
  auto* box = new QHBoxLayout;
  box->setContentsMargins(0, 0, 0, 0);
  box->setSpacing(4);
  column->addLayout(box);
  m_message = new QLabel;
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  m_message->hide();
  column->addWidget(m_message);
  m_editor = new QWidget;
  m_editor->setFixedWidth(kEditorWidth);
  // One white fill for every field, set or empty (mitcad#655: an empty
  // one must not read as disabled; only its placeholder is grey); the
  // grey fill of a disabled field is for read only (a built-in item).
  m_editor->setStyleSheet(QStringLiteral("QLineEdit { background: palette(base); border: 1px solid palette(mid);"
                                         " border-radius: 3px; padding: 1px 4px; }"
                                         "QLineEdit:focus { border: 1px solid palette(highlight); }"
                                         "QLineEdit:disabled { background: palette(window); }"));
  auto* editor = new QHBoxLayout(m_editor);
  editor->setContentsMargins(0, 0, 0, 0);
  editor->setSpacing(4);
  const auto unitLabel = [](const QString& unit) {
    auto* label = new QLabel(unit);
    label->setFixedWidth(kUnitWidth);
    // The unit where a [mm | %] box shows its text, so a column of units
    // reads the same (mitcad#655).
    label->setContentsMargins(6, 0, 0, 0);
    return label;
  };
  // Every unset field is an empty input with the placeholder "Slicer
  // default" (mitcad#655): it opens empty, never with a guessed value.
  const auto lineField = [this](int width) {
    m_line = new QLineEdit;
    m_line->setPlaceholderText(tr("Slicer default"));
    if (width > 0) {
      m_line->setFixedWidth(width);
    }
    m_line->installEventFilter(this);
    connect(m_line, &QLineEdit::textEdited, this, [this] { m_dirty = true; });
    connect(m_line, &QLineEdit::editingFinished, this, [this] {
      if (m_dirty) {
        commitTyped();
      }
    });
    return m_line;
  };
  const auto comboField = [this]() {
    auto* combo = new WheelSafeComboBox;
    m_combo = combo;
    m_combo->setPlaceholderText(tr("Slicer default"));
    connect(m_combo, &QComboBox::activated, this, [this](int index) {
      const QVariant data = m_combo->itemData(index);
      commit(data.typeId() == QMetaType::Bool ? QJsonValue(data.toBool()) : QJsonValue(data.toString()));
    });
    return m_combo;
  };
  if (!m_slicer.isEmpty()) {
    // One slicer's preset (mitcad#1062): a name typed or chosen from the
    // installed slicer's presets; Enter, leaving the field or a choice
    // commits it.
    auto* combo = new PresetComboBox;
    combo->names = [this] { return m_choices ? m_choices() : QStringList(); };
    combo->setEditable(true);
    combo->setInsertPolicy(QComboBox::NoInsert);
    m_combo = combo;
    m_line = combo->lineEdit();
    m_line->installEventFilter(this);
    connect(m_line, &QLineEdit::textEdited, this, [this] { m_dirty = true; });
    connect(m_line, &QLineEdit::editingFinished, this, [this] {
      if (m_dirty) {
        commitTyped();
      }
    });
    connect(m_combo, &QComboBox::activated, this,
            [this](int index) { commitPreset(m_combo->itemText(index).trimmed()); });
    editor->addWidget(m_combo, 1);
  } else if (m_key.perDialect) {
    m_text = new QLabel;
    m_text->setTextInteractionFlags(Qt::TextSelectableByMouse);
    editor->addWidget(m_text, 1);
  } else {
    switch (m_key.kind) {
    case PrintKey::Kind::Boolean:
      editor->addWidget(comboField(), 1);
      m_combo->addItem(tr("Yes"), true);
      m_combo->addItem(tr("No"), false);
      break;
    case PrintKey::Kind::Integer:
    case PrintKey::Kind::Number:
      editor->addWidget(lineField(kNumberWidth));
      editor->addWidget(unitLabel(m_key.unit));
      break;
    case PrintKey::Kind::Percent:
      editor->addWidget(lineField(kNumberWidth));
      editor->addWidget(unitLabel(QStringLiteral("%")));
      break;
    case PrintKey::Kind::NumberOrPercent: {
      editor->addWidget(lineField(kNumberWidth));
      if (!m_takesPercent) {
        // The design's slicer takes no percentage here (mitcad#655): the
        // unit as a label, "%" only for a percentage already set.
        m_unitLabel = unitLabel(m_key.unit.isEmpty() ? QStringLiteral("mm") : m_key.unit);
        editor->addWidget(m_unitLabel);
        break;
      }
      auto* units = new WheelSafeComboBox;
      m_unitBox = units;
      m_unitBox->addItem(m_key.unit.isEmpty() ? QStringLiteral("mm") : m_key.unit);
      m_unitBox->addItem(QStringLiteral("%"));
      m_unitBox->setFixedWidth(kUnitWidth); // "mm" and "%" whole beside the arrow
      connect(m_unitBox, &QComboBox::activated, this, [this] {
        if (!m_line->text().trimmed().isEmpty()) {
          m_dirty = true;
          commitTyped();
        }
      });
      editor->addWidget(m_unitBox);
      // Unset, the unit as plain text until a value is typed (mitcad#655).
      m_unitPlain = unitLabel(m_key.unit.isEmpty() ? QStringLiteral("mm") : m_key.unit);
      editor->addWidget(m_unitPlain);
      connect(m_line, &QLineEdit::textEdited, this, &SettingField::showUnit);
      break;
    }
    case PrintKey::Kind::Choice:
      editor->addWidget(comboField(), 1);
      for (const auto& [value, plain] : m_key.choices) {
        m_combo->addItem(plain, value);
      }
      break;
    case PrintKey::Kind::Color:
      // A field as wide as the others (mitcad#655): the colour's swatch and,
      // unset, the grey word for what is used instead (ColorFieldButton); a
      // click opens the colour picker. No code to type.
      m_swatch = new ColorFieldButton(m_key.name);
      m_swatch->setToolTip(tr("Choose a color"));
      connect(m_swatch, &QPushButton::clicked, this, &SettingField::pickColor);
      editor->addWidget(m_swatch, 1);
      break;
    case PrintKey::Kind::Points:
      // The outline in a read-only field of the standard width, Edit… after
      // it (mitcad#655); unset, the same empty field with Slicer default.
      m_outline = new QLineEdit;
      m_outline->setReadOnly(true);
      m_outline->setFocusPolicy(Qt::NoFocus);
      m_outline->setPlaceholderText(tr("Slicer default"));
      m_outline->setFixedWidth(kNumberWidth);
      m_edit = new QPushButton(tr("Edit…"));
      m_edit->setAutoDefault(false);
      connect(m_edit, &QPushButton::clicked, this, &SettingField::editOutline);
      editor->addWidget(m_outline);
      editor->addWidget(m_edit);
      break;
    case PrintKey::Kind::Text:
      editor->addWidget(lineField(0), 1);
      break;
    }
  }
  editor->addStretch(0);
  // ↺ right after the field; every field has the same width, so the ↺ of
  // a form make one column.
  m_clear = new QToolButton;
  m_clear->setText(QStringLiteral("↺"));
  m_clear->setAutoRaise(true);
  // A name, not a rule (mitcad#655).
  m_clear->setAccessibleName(tr("Reset"));
  m_clear->setToolTip(tr("Reset"));
  m_clear->setFocusPolicy(Qt::NoFocus);
  m_clear->setFixedWidth(24);
  connect(m_clear, &QToolButton::clicked, this, [this] {
    if (m_slicer.isEmpty()) {
      commit(QJsonValue());
    } else {
      commitPreset(QString()); // this slicer's entry only (mitcad#1062)
    }
  });
  box->addWidget(m_editor);
  box->addWidget(m_clear);
  // The own values bodies, occurrences or components set of the key, as
  // a grey hint like Newer: (mitcad#655): "4 on Body1, 3 on Arm".
  m_overrideMark = new QLabel;
  setRoleStyleSheet(m_overrideMark, ColorRole::TextSecondary);
  m_overrideMark->hide();
  box->addWidget(m_overrideMark);
  // What Update would make the value (mitcad#655), small and grey.
  m_hint = new QLabel;
  setRoleStyleSheet(m_hint, ColorRole::TextSecondary);
  m_hint->hide();
  box->addWidget(m_hint);
  box->addStretch(1);
}

void SettingField::setError(const QString& message) {
  m_message->setText(message);
  m_message->setVisible(!message.isEmpty());
}

void SettingField::setValue(const QJsonValue& value) {
  if (m_dirty) {
    return; // the user's typing stays until it is committed or left
  }
  setError(QString());
  m_value = value;
  // One slicer's field is set when that slicer has its own entry.
  m_set = m_slicer.isEmpty() ? !(value.isNull() || value.isUndefined()) : !ownPreset().isEmpty();
  showValue();
  updateState();
}

void SettingField::setPlaceholderValue(const QJsonValue& value, const QString& words) {
  m_placeholder = value;
  m_placeholderText = words;
  if (m_line != nullptr && m_slicer.isEmpty()) {
    // As the field would show the value: a number without its unit, which
    // is beside the field (or in its unit box).
    QString text;
    if (!(value.isNull() || value.isUndefined())) {
      const bool numeric = m_key.kind == PrintKey::Kind::Integer || m_key.kind == PrintKey::Kind::Number ||
                           m_key.kind == PrintKey::Kind::Percent || m_key.kind == PrintKey::Kind::NumberOrPercent;
      const std::optional<double> number = numeric ? numberOf(value) : std::nullopt;
      text = number ? printNumberText(*number)
             : m_key.kind == PrintKey::Kind::Color ? value.toVariant().toString()
                                                   : printValueText(m_key, value);
    }
    m_line->setPlaceholderText(text.isEmpty() ? tr("Slicer default") : text);
  }
  showValue();
}

void SettingField::setOwnValues(const QStringList& values) {
  m_overrideMark->setText(values.join(QStringLiteral(", ")));
  m_overrideMark->setToolTip(values.join(QStringLiteral(", ")));
  m_overrideMark->setVisible(!values.isEmpty());
}

void SettingField::setHint(const QString& hint, const QString& tooltip) {
  m_hint->setText(hint);
  m_hint->setToolTip(tooltip);
  m_hint->setVisible(!hint.isEmpty());
}

void SettingField::showUnit() {
  if (m_unitBox == nullptr || m_unitPlain == nullptr) {
    return;
  }
  const bool active = m_set || !m_line->text().trimmed().isEmpty();
  m_unitBox->setVisible(active);
  m_unitPlain->setVisible(!active);
}

void SettingField::setReadOnly(bool readOnly) {
  m_readOnly = readOnly || (m_key.perDialect && m_slicer.isEmpty());
  updateState();
}

void SettingField::setClearable(bool clearable) {
  m_clearable = clearable;
  updateState();
}

void SettingField::showValue() {
  const QJsonValue value = m_value;
  const bool none = !m_set;
  if (!m_slicer.isEmpty()) {
    // Its own entry; unset, what the export uses then: another Bambu-family
    // slicer's preset, else none (the material's own name, no preset).
    QString from;
    const QString used = slicerPresetOf(value, m_slicer, &from);
    const QSignalBlocker block(m_combo);
    m_line->setText(none ? QString() : used);
    m_line->setPlaceholderText(none && !used.isEmpty() ? tr("%1 (as %2)").arg(used, slicerTitle(from)) : tr("None"));
    return;
  }
  if (m_key.perDialect) {
    QStringList lines;
    const QJsonObject slicers = value.toObject();
    for (auto it = slicers.begin(); it != slicers.end(); ++it) {
      lines << QStringLiteral("%1: %2").arg(slicerTitle(it.key()), it.value().toVariant().toString());
    }
    m_text->setText(none ? tr("Slicer default") : lines.join(QLatin1Char('\n')));
    m_text->setEnabled(!none);
    return;
  }
  switch (m_key.kind) {
  case PrintKey::Kind::Boolean: {
    const QSignalBlocker block(m_combo);
    const bool on = value.isBool() ? value.toBool() : printValueOf(m_key, value.toVariant().toString()).toBool();
    m_combo->setCurrentIndex(none ? -1 : (on ? 0 : 1));
    break;
  }
  case PrintKey::Kind::Integer:
  case PrintKey::Kind::Number:
  case PrintKey::Kind::Percent:
  case PrintKey::Kind::NumberOrPercent: {
    const std::optional<double> number = none ? std::nullopt : numberOf(value);
    m_line->setText(number ? printNumberText(*number) : QString());
    if (m_unitLabel != nullptr) {
      const bool percent = value.isString() && value.toString().trimmed().endsWith(QLatin1Char('%'));
      m_unitLabel->setText(percent ? QStringLiteral("%") : (m_key.unit.isEmpty() ? QStringLiteral("mm") : m_key.unit));
    }
    showUnit();
    if (m_unitBox != nullptr) {
      const QSignalBlocker block(m_unitBox);
      const bool percent = value.isString() && value.toString().trimmed().endsWith(QLatin1Char('%'));
      m_unitBox->setCurrentIndex(percent ? 1 : 0);
    }
    break;
  }
  case PrintKey::Kind::Choice: {
    const QSignalBlocker block(m_combo);
    const QString chosen = value.toVariant().toString();
    int index = none ? -1 : m_combo->findData(chosen);
    if (index < 0 && !none) {
      m_combo->addItem(chosen, chosen); // a value the choices lack stays shown
      index = m_combo->count() - 1;
    }
    m_combo->setCurrentIndex(index);
    break;
  }
  case PrintKey::Kind::Color: {
    // An unset colour shows what is used instead (the appearance's swatch
    // and its word, grey as a placeholder) or nothing; never a code.
    static_cast<ColorFieldButton*>(m_swatch)->setColor(
        QColor(none ? m_placeholder.toString() : value.toVariant().toString()),
        none ? (m_placeholderText.isEmpty() ? tr("Slicer default") : m_placeholderText) : QString(), none);
    break;
  }
  case PrintKey::Kind::Points:
    m_outline->setText(none ? QString() : outlineText(value));
    break;
  case PrintKey::Kind::Text:
    m_line->setText(none ? QString() : printValueText(m_key, value));
    break;
  }
}

void SettingField::updateState() {
  // Read only: the field disabled, so it looks read only (mitcad#655).
  m_editor->setEnabled(!m_readOnly || (m_key.perDialect && m_slicer.isEmpty()));
  if (m_line != nullptr) {
    m_line->setReadOnly(m_readOnly);
  }
  if (m_edit != nullptr) {
    m_edit->setEnabled(!m_readOnly);
  }
  // ↺ keeps its place while hidden, so that the column stays.
  QSizePolicy policy = m_clear->sizePolicy();
  policy.setRetainSizeWhenHidden(true);
  m_clear->setSizePolicy(policy);
  m_clear->setVisible(m_set && m_clearable && !m_readOnly);
}

void SettingField::startEditing() {
  if (m_readOnly) {
    return;
  }
  if (m_line != nullptr) {
    m_line->setFocus(Qt::OtherFocusReason);
    m_line->selectAll();
  } else if (m_combo != nullptr) {
    m_combo->setFocus(Qt::OtherFocusReason);
    if (!m_set) {
      m_combo->showPopup();
    }
  } else if (m_edit != nullptr) {
    m_edit->setFocus(Qt::OtherFocusReason);
  } else if (m_swatch != nullptr) {
    m_swatch->setFocus(Qt::OtherFocusReason);
  }
}

QWidget* SettingField::placeWidget() const {
  if (m_line != nullptr) {
    return m_line;
  }
  if (m_combo != nullptr) {
    return m_combo;
  }
  if (m_edit != nullptr) {
    return m_edit;
  }
  if (m_swatch != nullptr) {
    return m_swatch;
  }
  return m_editor;
}

QWidget* SettingField::clearWidget() const { return m_clear->isVisibleTo(this) ? m_clear : nullptr; }

QString SettingField::typedText() const {
  if (m_line == nullptr) {
    return QString();
  }
  QString text = m_line->text().trimmed();
  const bool percent = (m_unitBox != nullptr && m_unitBox->currentIndex() == 1) ||
                       (m_unitLabel != nullptr && m_unitLabel->text() == QStringLiteral("%"));
  if (percent && !text.isEmpty() && !text.endsWith(QLatin1Char('%'))) {
    text += QLatin1Char('%');
  }
  return text;
}

void SettingField::commitTyped() {
  if (!m_slicer.isEmpty()) {
    commitPreset(m_line->text().trimmed());
    return;
  }
  QString error;
  const QJsonValue typed = printValueOf(m_key, typedText(), &error, m_dialect);
  if (!error.isEmpty()) {
    setError(error); // at the field (mitcad#655)
    qDebug().noquote() << QStringLiteral("Setting %1 refused: %2").arg(m_key.key, error);
    emit refused(m_key.key, error);
    return;
  }
  setError(QString());
  m_dirty = false;
  if (typed.isNull()) {
    // An empty field sets nothing; emptying a set one clears it.
    if (m_set) {
      commit(QJsonValue());
    }
    return;
  }
  if (m_set && typed == m_value) {
    return;
  }
  commit(typed);
}

void SettingField::commit(const QJsonValue& value) {
  m_dirty = false;
  emit committed(m_key.key, value);
}

QString SettingField::ownPreset() const { return m_value.toObject().value(m_slicer).toString().trimmed(); }

void SettingField::commitPreset(const QString& name) {
  m_dirty = false;
  if (name == ownPreset()) {
    return; // also Enter's second signal (the list's activated)
  }
  // The other slicers' entries stay; a plain text (an older form) is the
  // name of every slicer, so it is kept for the others.
  QJsonObject slicers = m_value.toObject();
  if (m_value.isString()) {
    for (const QString& dialect : std::as_const(m_key.dialects)) {
      slicers.insert(dialect, m_value.toString());
    }
  }
  if (name.isEmpty()) {
    slicers.remove(m_slicer);
  } else {
    slicers.insert(m_slicer, name);
  }
  // Shown at once, so that a second signal of the same edit commits
  // nothing; the model's answer shows it again.
  m_value = slicers.isEmpty() ? QJsonValue() : QJsonValue(slicers);
  m_set = !name.isEmpty();
  emit committed(m_key.key, m_value);
}

void SettingField::editOutline() {
  OutlineDialog dialog(m_key, m_set ? m_value : QJsonValue(QJsonArray()), this);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  commit(dialog.points());
}

void SettingField::pickColor() {
  QColor initial(m_set ? m_value.toVariant().toString() : QString());
  if (!initial.isValid()) {
    initial = QColor(m_placeholder.toString()); // the colour used instead
  }
  const QColor chosen = dialogColor(initial.isValid() ? initial : QColor(Qt::white), this, m_key.name);
  if (chosen.isValid()) {
    commit(QJsonValue(chosen.name().toUpper()));
  }
}

bool SettingField::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_line && event->type() == QEvent::KeyPress) {
    const auto* press = static_cast<QKeyEvent*>(event);
    if ((press->key() == Qt::Key_Return || press->key() == Qt::Key_Enter) && !m_line->text().trimmed().isEmpty()) {
      m_dirty = true; // Enter commits what the field shows; an empty one sets nothing
    }
  }
  return QWidget::eventFilter(watched, event);
}

} // namespace mitcad
