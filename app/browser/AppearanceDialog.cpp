// SPDX-License-Identifier: MIT
#include "AppearanceDialog.hpp"

#include <algorithm>
#include <initializer_list>
#include <utility>

#include <QCheckBox>
#include <QColorDialog>
#include <QComboBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QFont>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QPixmap>
#include <QPushButton>
#include <QSignalBlocker>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "DocumentHost.hpp"

namespace mitcad {
namespace {

constexpr int kIdRole = Qt::UserRole;
constexpr int kBodyRole = Qt::UserRole + 1;
constexpr int kFaceRole = Qt::UserRole + 2;
constexpr int kPreviewSize = 120;
constexpr double kDegree = 3.14159265358979323846 / 180.0;
// The largest image the project file embeds (core/model's
// MAX_EMBEDDED_IMAGE).
constexpr qint64 kMaxEmbedded = qint64(32) << 20;
constexpr int kIconSize = 22;

QString hexOf(const QColor& color) { return color.name(QColor::HexRgb); }

// A small square of the colour, for the colour buttons.
QIcon colorIcon(const QColor& color) {
  QPixmap pixmap(16, 16);
  pixmap.fill(color.isValid() ? color : QColor(Qt::transparent));
  return QIcon(pixmap);
}

QString number(double value) { return QString::number(value, 'g', 6); }

} // namespace

AppearanceDialog::AppearanceDialog(DocumentHost& host, QWidget* parent) : QDialog(parent), m_host(host) {
  setWindowTitle(tr("Appearances"));
  setWindowIcon(themeIcon(QStringLiteral("appearance")));
  resize(700, 760);
  auto* layout = new QVBoxLayout(this);
  auto* columns = new QHBoxLayout;
  layout->addLayout(columns, 1);

  m_list = new QListWidget;
  m_list->setIconSize(QSize(kIconSize, kIconSize));
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  m_list->setMinimumWidth(220);
  columns->addWidget(m_list, 1);

  auto* right = new QVBoxLayout;
  columns->addLayout(right, 1);
  m_preview = new QLabel;
  m_preview->setFixedSize(kPreviewSize, kPreviewSize);
  m_preview->setToolTip(tr("Preview: the rendered view shows the appearance lit by the sky and the sun"));
  right->addWidget(m_preview, 0, Qt::AlignHCenter);
  auto* form = new QFormLayout;
  right->addLayout(form);
  m_name = new QLineEdit;
  form->addRow(tr("Name"), m_name);
  addColor(form, "base_color", tr("Base colour"));
  addNumber(form, "metalness", tr("Metalness"), 0.0, 1.0, 0.05, tr("0 for plastics, paint and glass, 1 for metals"));
  addNumber(form, "roughness", tr("Roughness"), 0.0, 1.0, 0.05, tr("0 for a mirror finish, 1 for a fully matt one"));
  addNumber(form, "specular", tr("Specular"), 0.0, 1.0, 0.05, tr("The strength of a non-metal's reflection"));
  addNumber(form, "transmission", tr("Transmission"), 0.0, 1.0, 0.05,
            tr("Light going through: 1 for glass and clear plastic"));
  addNumber(form, "ior", tr("Index of refraction"), 1.0, 5.0, 0.05, tr("1.5 for glass and most plastics"));
  addNumber(form, "coat", tr("Coat"), 0.0, 1.0, 0.05, tr("A clear lacquer over the surface"));
  addNumber(form, "coat_roughness", tr("Coat roughness"), 0.0, 1.0, 0.05, QString());
  addNumber(form, "emission", tr("Emission"), 0.0, 1000.0, 0.5, tr("Light the surface gives off; 0 for none"));
  addColor(form, "emission_color", tr("Emission colour"));
  addNumber(form, "opacity", tr("Opacity"), 0.0, 1.0, 0.05, tr("Less than 1 makes the surface a see-through cut-out"));

  // The texture (mitcad#53).
  auto* textureTitle = new QLabel(tr("Texture"));
  QFont titleFont = textureTitle->font();
  titleFont.setBold(true);
  textureTitle->setFont(titleFont);
  form->addRow(textureTitle);
  auto* pathRow = new QHBoxLayout;
  m_texturePath = new QLineEdit;
  m_texturePath->setPlaceholderText(tr("No image"));
  m_texturePath->setToolTip(
      tr("A PNG or JPEG image for the base colour, relative to the design's folder or absolute"));
  m_textureChoose = new QPushButton(tr("Choose..."));
  m_textureRemove = new QPushButton(tr("Remove"));
  m_textureRemove->setToolTip(tr("No texture: the base colour only"));
  // Enter in a field changes it, it does not press these.
  m_textureChoose->setAutoDefault(false);
  m_textureRemove->setAutoDefault(false);
  pathRow->addWidget(m_texturePath, 1);
  pathRow->addWidget(m_textureChoose);
  pathRow->addWidget(m_textureRemove);
  form->addRow(tr("Image"), pathRow);
  const auto sizeBox = [] {
    auto* box = new QDoubleSpinBox;
    box->setRange(0.01, 100000.0);
    box->setDecimals(2);
    box->setSuffix(QStringLiteral(" mm"));
    box->setKeyboardTracking(false);
    return box;
  };
  m_textureWidth = sizeBox();
  m_textureWidth->setToolTip(tr("How wide one repeat of the image is on the body"));
  m_textureHeight = sizeBox();
  m_textureHeight->setToolTip(tr("How high one repeat of the image is on the body"));
  auto* sizeRow = new QHBoxLayout;
  sizeRow->addWidget(m_textureWidth);
  sizeRow->addWidget(new QLabel(QStringLiteral("×")));
  sizeRow->addWidget(m_textureHeight);
  sizeRow->addStretch(1);
  form->addRow(tr("Size"), sizeRow);
  m_textureRotation = new QDoubleSpinBox;
  m_textureRotation->setRange(-360.0, 360.0);
  m_textureRotation->setDecimals(1);
  m_textureRotation->setSuffix(QStringLiteral("°"));
  m_textureRotation->setKeyboardTracking(false);
  m_textureRotation->setToolTip(tr("The image turned about the projection's axis"));
  form->addRow(tr("Rotation"), m_textureRotation);
  m_textureProjection = new QComboBox;
  m_textureProjection->addItem(tr("Box"), QStringLiteral("box"));
  m_textureProjection->addItem(tr("Planar"), QStringLiteral("planar"));
  m_textureProjection->setToolTip(tr("Box: from the three axes, each surface from the one it faces most. "
                                     "Planar: along the body's Z axis"));
  form->addRow(tr("Projection"), m_textureProjection);
  m_textureEmbed = new QCheckBox(tr("Embed the image in the design"));
  m_textureEmbed->setToolTip(tr("The project file keeps a copy of the image, so it goes with the design"));
  form->addRow(QString(), m_textureEmbed);
  m_textureInfo = new QLabel;
  m_textureInfo->setWordWrap(true);
  right->addWidget(m_textureInfo);
  m_info = new QLabel;
  m_info->setWordWrap(true);
  right->addWidget(m_info);
  right->addStretch(1);

  // Faces with appearances of their own (mitcad#53).
  auto* facesRow = new QHBoxLayout;
  auto* facesTitle = new QLabel(tr("Faces with their own appearance"));
  facesRow->addWidget(facesTitle, 1);
  m_clearFaces = new QPushButton(tr("Clear"));
  m_clearFaces->setAutoDefault(false);
  m_clearFaces->setToolTip(tr("The selected faces show their body's appearance again"));
  facesRow->addWidget(m_clearFaces);
  layout->addLayout(facesRow);
  m_faces = new QListWidget;
  m_faces->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_faces->setMaximumHeight(110);
  layout->addWidget(m_faces);

  m_message = new QLabel;
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  layout->addWidget(m_message);

  auto* buttons = new QHBoxLayout;
  m_new = new QPushButton(themeIcon(QStringLiteral("add")), tr("New"));
  m_new->setToolTip(tr("A copy of the selected appearance in this design, to edit"));
  m_delete = new QPushButton(themeIcon(QStringLiteral("delete")), tr("Delete"));
  m_delete->setToolTip(tr("Delete the selected appearance of this design; its bodies get the default look"));
  m_assign = new QPushButton(themeIcon(QStringLiteral("appearance")), tr("Assign"));
  m_close = new QPushButton(tr("Close"));
  for (QPushButton* button : {m_new, m_delete, m_assign, m_close}) {
    button->setAutoDefault(false);
  }
  buttons->addWidget(m_new);
  buttons->addWidget(m_delete);
  buttons->addStretch(1);
  buttons->addWidget(m_assign);
  buttons->addWidget(m_close);
  layout->addLayout(buttons);

  connect(m_close, &QPushButton::clicked, this, &QDialog::accept);
  connect(m_new, &QPushButton::clicked, this, &AppearanceDialog::newAppearance);
  connect(m_delete, &QPushButton::clicked, this, &AppearanceDialog::deleteAppearance);
  connect(m_assign, &QPushButton::clicked, this, &AppearanceDialog::assign);
  connect(m_list, &QListWidget::currentItemChanged, this, [this](QListWidgetItem* item) {
    if (m_building || item == nullptr || item->data(kIdRole).toString().isEmpty()) {
      return;
    }
    const QString id = item->data(kIdRole).toString();
    if (id != m_current) {
      m_current = id;
      qDebug().noquote() << QStringLiteral("Appearance selected: %1").arg(id);
    }
    showCurrent();
  });
  connect(m_texturePath, &QLineEdit::editingFinished, this, [this] {
    const Appearance* a = current();
    const QString typed = m_texturePath->text().trimmed();
    if (!m_building && a != nullptr && !a->library && !typed.isEmpty() &&
        (!a->hasTexture || typed != a->texture.path)) {
      setTexturePath(typed);
    }
  });
  connect(m_textureChoose, &QPushButton::clicked, this, &AppearanceDialog::chooseTexture);
  connect(m_textureRemove, &QPushButton::clicked, this, [this] { sendTexture(QJsonValue(), tr("removed")); });
  connect(m_textureWidth, &QDoubleSpinBox::valueChanged, this, [this](double value) {
    const Appearance* a = current();
    if (!m_building && a != nullptr) {
      changeTexture(QStringLiteral("size"), QJsonArray{value, a->texture.height});
    }
  });
  connect(m_textureHeight, &QDoubleSpinBox::valueChanged, this, [this](double value) {
    const Appearance* a = current();
    if (!m_building && a != nullptr) {
      changeTexture(QStringLiteral("size"), QJsonArray{a->texture.width, value});
    }
  });
  connect(m_textureRotation, &QDoubleSpinBox::valueChanged, this, [this](double degrees) {
    if (!m_building) {
      changeTexture(QStringLiteral("rotation"), degrees * kDegree);
    }
  });
  connect(m_textureProjection, &QComboBox::currentIndexChanged, this, [this] {
    if (!m_building) {
      changeTexture(QStringLiteral("projection"), m_textureProjection->currentData().toString());
    }
  });
  connect(m_textureEmbed, &QCheckBox::toggled, this, [this](bool embed) {
    if (!m_building) {
      embedTexture(embed);
    }
  });
  connect(m_faces, &QListWidget::itemSelectionChanged, this,
          [this] { m_clearFaces->setEnabled(!m_faces->selectedItems().isEmpty()); });
  connect(m_clearFaces, &QPushButton::clicked, this, &AppearanceDialog::clearFaces);
  connect(m_name, &QLineEdit::editingFinished, this, [this] {
    const Appearance* a = current();
    if (!m_building && a != nullptr && !a->library && m_name->text().trimmed() != a->name) {
      change(QStringLiteral("name"), m_name->text().trimmed());
    }
  });
  setTargets({});
  refresh();
}

void AppearanceDialog::addNumber(QFormLayout* form, const char* key, const QString& label, double low, double high,
                                 double step, const QString& tip) {
  auto* box = new QDoubleSpinBox;
  box->setRange(low, high);
  box->setSingleStep(step);
  box->setDecimals(3);
  // A typed value counts on Enter or when the field is left, not per key.
  box->setKeyboardTracking(false);
  box->setToolTip(tip);
  form->addRow(label, box);
  m_numbers.append({key, box});
  connect(box, &QDoubleSpinBox::valueChanged, this, [this, key](double value) {
    if (!m_building) {
      change(QString::fromLatin1(key), value);
    }
  });
}

void AppearanceDialog::addColor(QFormLayout* form, const char* key, const QString& label) {
  auto* row = new QHBoxLayout;
  auto* button = new QToolButton;
  button->setToolTip(tr("Choose a colour"));
  auto* hex = new QLineEdit;
  hex->setToolTip(tr("sRGB as #rrggbb"));
  hex->setMaximumWidth(110);
  row->addWidget(button);
  row->addWidget(hex);
  row->addStretch(1);
  form->addRow(label, row);
  const ColorField field{key, button, hex};
  m_colors.append(field);
  connect(button, &QToolButton::clicked, this, [this, field] { pickColor(field); });
  connect(hex, &QLineEdit::editingFinished, this, [this, field] {
    const Appearance* a = current();
    if (m_building || a == nullptr || a->library) {
      return;
    }
    const QColor color(field.hex->text().trimmed());
    if (!color.isValid()) {
      m_message->setText(tr("'%1' is no colour: write it as #rrggbb.").arg(field.hex->text()));
      showCurrent();
      return;
    }
    const QColor old = qstrcmp(field.key, "base_color") == 0 ? a->baseColor : a->emissionColor;
    if (hexOf(color) != hexOf(old)) {
      change(QString::fromLatin1(field.key), colorJson(color));
    }
  });
}

void AppearanceDialog::setTargets(const Selection& targets) {
  m_targets = targets;
  QStringList bodies;
  QVector<QPair<QString, QString>> faces;
  for (const SelectionItem& item : targets) {
    if (item.kind == SelectKind::Face) {
      faces.append({item.owner, item.name});
    } else if (!bodies.contains(item.owner)) {
      bodies << item.owner;
    }
  }
  if (targets.isEmpty()) {
    m_assign->setText(tr("Assign"));
    m_assign->setEnabled(false);
    m_assign->setToolTip(tr("Select bodies or faces first, or use a body's context menu"));
  } else {
    if (faces.isEmpty()) {
      m_assign->setText(bodies.size() == 1 ? tr("Assign to %1").arg(m_host.model().bodyName(bodies.first()))
                                           : tr("Assign to %1 Bodies").arg(bodies.size()));
    } else if (bodies.isEmpty()) {
      m_assign->setText(faces.size() == 1 ? tr("Assign to Face") : tr("Assign to %1 Faces").arg(faces.size()));
    } else {
      m_assign->setText(tr("Assign to %1 Items").arg(targets.size()));
    }
    m_assign->setEnabled(true);
    m_assign->setToolTip(faces.isEmpty() ? tr("Give the bodies the selected appearance")
                                         : tr("Give the bodies and faces the selected appearance; a face's "
                                              "appearance overrides its body's"));
  }
  // The targets' own appearance, when they share one: a face's own, else
  // its body's.
  QString shared;
  bool same = !targets.isEmpty();
  bool first = true;
  try {
    for (const QJsonValue& value : m_host.model().queryArray(QStringLiteral("bodies"))) {
      const QJsonObject body = value.toObject();
      const QString uid = body.value(QStringLiteral("uid")).toString();
      const QString own = body.value(QStringLiteral("appearance")).toString();
      QStringList looks;
      if (bodies.contains(uid)) {
        looks << own;
      }
      for (const auto& [owner, face] : std::as_const(faces)) {
        if (owner != uid) {
          continue;
        }
        QString look = own;
        for (const QJsonValue& entry : body.value(QStringLiteral("face_appearances")).toArray()) {
          const QJsonObject faceLook = entry.toObject();
          bool named = faceLook.value(QStringLiteral("face")).toString() == face;
          for (const QJsonValue& name : faceLook.value(QStringLiteral("faces")).toArray()) {
            named = named || name.toString() == face;
          }
          if (named) {
            look = faceLook.value(QStringLiteral("appearance")).toString();
          }
        }
        looks << look;
      }
      for (const QString& look : std::as_const(looks)) {
        if (first) {
          shared = look;
          first = false;
        } else if (look != shared) {
          same = false;
        }
      }
    }
  } catch (const std::exception&) {
    same = false;
  }
  if (same && findAppearance(m_appearances, shared) != nullptr) {
    select(shared);
  } else {
    shared.clear();
  }
  qDebug().noquote() << QStringLiteral("Appearances for bodies: %1%2")
                            .arg(bodies.isEmpty() ? QStringLiteral("none") : bodies.join(QStringLiteral(", ")),
                                 shared.isEmpty() || !faces.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(shared));
  if (!faces.isEmpty()) {
    QStringList named;
    for (const auto& [owner, face] : std::as_const(faces)) {
      named << QStringLiteral("%1 %2").arg(owner, face);
    }
    qDebug().noquote() << QStringLiteral("Appearances for faces: %1%2")
                              .arg(named.join(QStringLiteral(", ")),
                                   shared.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(shared));
  }
  showFaces();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

const Appearance* AppearanceDialog::current() const { return findAppearance(m_appearances, m_current); }

bool AppearanceDialog::canChange() {
  // Not in the middle of a command or a sketch, whose undo steps the
  // change would come between.
  if (m_host.canChangeModel()) {
    return true;
  }
  m_message->setText(tr("Finish the command or the sketch first."));
  qDebug().noquote() << "Appearances: finish the command or the sketch first";
  return false;
}

void AppearanceDialog::select(const QString& id) {
  for (int i = 0; i < m_list->count(); ++i) {
    if (m_list->item(i)->data(kIdRole).toString() == id) {
      m_list->setCurrentRow(i);
      m_list->scrollToItem(m_list->item(i));
      return;
    }
  }
}

void AppearanceDialog::refresh() {
  try {
    m_appearances = appearancesOf(m_host.model().queryArray(QStringLiteral("appearances")));
  } catch (const std::exception& e) {
    m_message->setText(QString::fromUtf8(e.what()));
    return;
  }
  // Whether each texture's image is there (mitcad#53).
  const QString folder = m_host.documentFolder();
  for (Appearance& a : m_appearances) {
    if (a.hasTexture) {
      const QString id = a.id;
      resolveTexture(a.texture, folder, [this, id] {
        try {
          return QByteArray::fromBase64(m_host.model()
                                            .queryObject({{QStringLiteral("query"), QStringLiteral("appearance_image")},
                                                          {QStringLiteral("id"), id}})
                                            .value(QStringLiteral("data"))
                                            .toString()
                                            .toLatin1());
        } catch (const std::exception&) {
          return QByteArray();
        }
      });
    }
  }
  if (current() == nullptr) {
    m_current = m_appearances.isEmpty() ? QString() : m_appearances.first().id;
  }
  m_building = true;
  m_list->clear();
  const auto header = [this](const QString& text) {
    auto* item = new QListWidgetItem(text, m_list);
    item->setFlags(Qt::NoItemFlags);
    QFont font = item->font();
    font.setBold(true);
    item->setFont(font);
  };
  const qreal ratio = devicePixelRatioF();
  bool custom = false;
  header(tr("Library"));
  for (const Appearance& a : std::as_const(m_appearances)) {
    if (!a.library && !custom) {
      custom = true;
      header(tr("This Design"));
    }
    auto* item = new QListWidgetItem(QIcon(QPixmap::fromImage(appearanceSwatch(a, kIconSize, ratio))), a.name, m_list);
    item->setData(kIdRole, a.id);
    if (!a.bodies.isEmpty() || !a.faces.isEmpty()) {
      item->setToolTip(tr("Used by %n body(ies)", nullptr, static_cast<int>(a.bodies.size())) +
                       (a.faces.isEmpty() ? QString()
                                          : tr(" and %n face(s)", nullptr, static_cast<int>(a.faces.size()))));
    }
  }
  m_building = false;
  select(m_current);
  showCurrent();
  showFaces();
}

void AppearanceDialog::showCurrent() {
  const Appearance* a = current();
  m_building = true;
  const bool editable = a != nullptr && !a->library;
  m_name->setEnabled(editable);
  m_name->setText(a != nullptr ? a->name : QString());
  for (const NumberField& field : std::as_const(m_numbers)) {
    field.box->setEnabled(editable);
    if (a == nullptr) {
      continue;
    }
    const QString key = QString::fromLatin1(field.key);
    const double value = key == QLatin1String("metalness")        ? a->metalness
                         : key == QLatin1String("roughness")      ? a->roughness
                         : key == QLatin1String("specular")       ? a->specular
                         : key == QLatin1String("transmission")   ? a->transmission
                         : key == QLatin1String("ior")            ? a->ior
                         : key == QLatin1String("coat")           ? a->coat
                         : key == QLatin1String("coat_roughness") ? a->coatRoughness
                         : key == QLatin1String("emission")       ? a->emission
                                                                  : a->opacity;
    field.box->setValue(value);
  }
  for (const ColorField& field : std::as_const(m_colors)) {
    field.button->setEnabled(editable);
    field.hex->setEnabled(editable);
    const QColor color = a == nullptr                              ? QColor()
                         : qstrcmp(field.key, "base_color") == 0 ? a->baseColor
                                                                   : a->emissionColor;
    field.button->setIcon(colorIcon(color));
    field.hex->setText(color.isValid() ? hexOf(color) : QString());
  }
  m_building = false;
  if (a == nullptr) {
    m_preview->clear();
    m_info->clear();
  } else {
    m_preview->setPixmap(QPixmap::fromImage(appearanceSwatch(*a, kPreviewSize, devicePixelRatioF())));
    QStringList lines;
    lines << (a->library ? tr("Mitcad's library: read-only. New makes a copy in this design to edit.")
                         : tr("An appearance of this design."));
    if (!a->bodies.isEmpty()) {
      QStringList names;
      for (const QString& uid : a->bodies) {
        names << m_host.model().bodyName(uid);
      }
      lines << tr("Used by %1.").arg(names.join(QStringLiteral(", ")));
    }
    if (!a->faces.isEmpty()) {
      lines << tr("Used by %n face(s) with an appearance of their own.", nullptr, static_cast<int>(a->faces.size()));
    }
    m_info->setText(lines.join(QLatin1Char('\n')));
  }
  showTexture(a, editable);
  m_delete->setEnabled(editable);
  m_new->setEnabled(a != nullptr);
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void AppearanceDialog::change(const QString& key, const QJsonValue& value) {
  const Appearance* a = current();
  if (a == nullptr || a->library) {
    return;
  }
  if (!canChange()) {
    showCurrent();
    return;
  }
  const QString id = a->id;
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_appearance")}, {QStringLiteral("id"), id}};
  command.insert(key, value);
  const QString text = value.isArray() ? hexOf(colorOf(value))
                       : value.isDouble() ? number(value.toDouble())
                                          : value.toString();
  if (m_host.runModelCommand(command)) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Appearance %1 %2 = %3").arg(id, key, text);
  } else {
    m_message->setText(m_host.lastError());
    qDebug().noquote() << QStringLiteral("Appearance %1 %2 = %3 refused: %4").arg(id, key, text, m_host.lastError());
  }
  refresh();
}

void AppearanceDialog::pickColor(const ColorField& field) {
  const Appearance* a = current();
  if (a == nullptr || a->library) {
    return;
  }
  const bool base = qstrcmp(field.key, "base_color") == 0;
  const QColor color = QColorDialog::getColor(base ? a->baseColor : a->emissionColor, this,
                                              base ? tr("Base Colour") : tr("Emission Colour"));
  if (color.isValid()) {
    change(QString::fromLatin1(field.key), colorJson(color));
  }
}

void AppearanceDialog::newAppearance() {
  const Appearance* a = current();
  if (a == nullptr || !canChange()) {
    return;
  }
  // "Chrome Copy", "Chrome Copy 2", ...: names are unique.
  QString name = tr("%1 Copy").arg(a->name);
  for (int n = 2; std::any_of(m_appearances.begin(), m_appearances.end(),
                              [&name](const Appearance& other) { return other.name == name; });
       ++n) {
    name = tr("%1 Copy %2").arg(a->name).arg(n);
  }
  const QString base = a->id;
  QJsonObject result;
  if (!m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("create_appearance")},
                               {QStringLiteral("based_on"), base},
                               {QStringLiteral("name"), name}},
                              &result)) {
    m_message->setText(m_host.lastError());
    return;
  }
  m_message->clear();
  m_current = result.value(QStringLiteral("id")).toString();
  qDebug().noquote() << QStringLiteral("Created appearance %1 (%2) from %3").arg(m_current, name, base);
  refresh();
  m_name->setFocus();
  m_name->selectAll();
}

void AppearanceDialog::deleteAppearance() {
  const Appearance* a = current();
  if (a == nullptr || a->library || !canChange()) {
    return;
  }
  const QString id = a->id;
  if (!m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("delete_appearance")}, {QStringLiteral("id"), id}})) {
    m_message->setText(m_host.lastError());
    return;
  }
  m_message->clear();
  qDebug().noquote() << QStringLiteral("Deleted appearance %1").arg(id);
  m_current.clear();
  refresh();
}

void AppearanceDialog::assign() {
  const Appearance* a = current();
  if (a == nullptr || m_targets.isEmpty() || !canChange()) {
    return;
  }
  const QString id = a->id;
  QJsonArray commands;
  QStringList described;
  // Faces by body: one command each (mitcad#53).
  QStringList owners;
  QHash<QString, QJsonArray> faces;
  for (const SelectionItem& item : std::as_const(m_targets)) {
    if (item.kind == SelectKind::Face) {
      if (!owners.contains(item.owner)) {
        owners << item.owner;
      }
      faces[item.owner].append(item.name);
      described << QStringLiteral("%1 %2").arg(item.owner, item.name);
    } else {
      commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_body_appearance")},
                                  {QStringLiteral("uid"), item.owner},
                                  {QStringLiteral("appearance"), id}});
      described << item.owner;
    }
  }
  for (const QString& owner : std::as_const(owners)) {
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_face_appearance")},
                                {QStringLiteral("uid"), owner},
                                {QStringLiteral("faces"), faces.value(owner)},
                                {QStringLiteral("appearance"), id}});
  }
  if (!m_host.runModelCommands(commands, tr("Appearance"))) {
    m_message->setText(m_host.lastError());
    return;
  }
  m_message->clear();
  qDebug().noquote() << QStringLiteral("Assigned appearance %1 to %2").arg(id, described.join(QStringLiteral(", ")));
  refresh();
}

QStringList AppearanceDialog::targetBodies() const {
  QStringList bodies;
  for (const SelectionItem& item : m_targets) {
    if (!bodies.contains(item.owner)) {
      bodies << item.owner;
    }
  }
  return bodies;
}

void AppearanceDialog::showFaces() {
  const QStringList bodies = targetBodies();
  m_faces->clear();
  try {
    for (const QJsonValue& value : m_host.model().queryArray(QStringLiteral("bodies"))) {
      const QJsonObject body = value.toObject();
      const QString uid = body.value(QStringLiteral("uid")).toString();
      if (!bodies.contains(uid)) {
        continue;
      }
      for (const QJsonValue& entry : body.value(QStringLiteral("face_appearances")).toArray()) {
        const QJsonObject face = entry.toObject();
        const QString name = face.value(QStringLiteral("face")).toString();
        const QString id = face.value(QStringLiteral("appearance")).toString();
        const Appearance* look = findAppearance(m_appearances, id);
        const bool found = !face.value(QStringLiteral("faces")).toArray().isEmpty();
        auto* item = new QListWidgetItem(QStringLiteral("%1: %2 — %3%4")
                                             .arg(body.value(QStringLiteral("name")).toString(), name,
                                                  look != nullptr ? look->name : id,
                                                  found ? QString() : tr(" (the face is not found)")),
                                         m_faces);
        if (look != nullptr) {
          item->setIcon(QIcon(QPixmap::fromImage(appearanceSwatch(*look, kIconSize, devicePixelRatioF()))));
        }
        item->setData(kBodyRole, uid);
        item->setData(kFaceRole, name);
        item->setData(kIdRole, id);
      }
    }
  } catch (const std::exception& e) {
    m_message->setText(QString::fromUtf8(e.what()));
  }
  m_faces->setToolTip(bodies.isEmpty() ? tr("Open the dialog on bodies or faces to see their faces' appearances")
                                       : QString());
  m_clearFaces->setEnabled(!m_faces->selectedItems().isEmpty());
}

void AppearanceDialog::clearFaces() {
  const QList<QListWidgetItem*> selected = m_faces->selectedItems();
  if (selected.isEmpty() || !canChange()) {
    return;
  }
  QStringList owners;
  QHash<QString, QJsonArray> faces;
  QStringList described;
  for (const QListWidgetItem* item : selected) {
    const QString owner = item->data(kBodyRole).toString();
    if (!owners.contains(owner)) {
      owners << owner;
    }
    faces[owner].append(item->data(kFaceRole).toString());
    described << QStringLiteral("%1 %2").arg(owner, item->data(kFaceRole).toString());
  }
  QJsonArray commands;
  for (const QString& owner : std::as_const(owners)) {
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_face_appearance")},
                                {QStringLiteral("uid"), owner},
                                {QStringLiteral("faces"), faces.value(owner)},
                                {QStringLiteral("appearance"), QJsonValue()}});
  }
  if (!m_host.runModelCommands(commands, tr("Clear Face Appearances"))) {
    m_message->setText(m_host.lastError());
    return;
  }
  m_message->clear();
  qDebug().noquote() << QStringLiteral("Cleared face appearances of %1").arg(described.join(QStringLiteral(", ")));
  refresh();
}

void AppearanceDialog::showTexture(const Appearance* a, bool editable) {
  m_building = true;
  const bool has = a != nullptr && a->hasTexture;
  m_texturePath->setEnabled(editable);
  m_texturePath->setText(has ? a->texture.path : QString());
  m_textureChoose->setEnabled(editable);
  m_textureRemove->setEnabled(editable && has);
  for (QWidget* field : std::initializer_list<QWidget*>{m_textureWidth, m_textureHeight, m_textureRotation,
                                                        m_textureProjection, m_textureEmbed}) {
    field->setEnabled(editable && has);
  }
  const AppearanceTexture texture = has ? a->texture : AppearanceTexture();
  m_textureWidth->setValue(texture.width);
  m_textureHeight->setValue(texture.height);
  m_textureRotation->setValue(texture.rotation / kDegree);
  m_textureProjection->setCurrentIndex(texture.planar ? 1 : 0);
  m_textureEmbed->setChecked(texture.embedded);
  m_building = false;
  if (!has) {
    m_textureInfo->setText(editable ? tr("No texture: choose a PNG or JPEG image for the base colour.") : QString());
  } else if (!texture.missing.isEmpty()) {
    m_textureInfo->setText(tr("The texture is not drawn: %1. The rendered view shows the base colour.")
                               .arg(texture.missing));
    qDebug().noquote() << QStringLiteral("Appearance %1 texture missing: %2").arg(a->id, texture.missing);
  } else {
    m_textureInfo->setText((texture.embedded ? tr("The image is embedded in the design. ") : QString()) +
                           tr("The rendered view draws the texture; the shaded view shows the base colour."));
  }
}

void AppearanceDialog::setTexturePath(const QString& typed) {
  const Appearance* a = current();
  if (a == nullptr || a->library) {
    return;
  }
  // Relative to the design's folder when it is saved (and the image is on
  // the same drive).
  QString path = QDir::fromNativeSeparators(typed);
  const QString folder = m_host.documentFolder();
  QString absolute = path;
  if (QFileInfo(path).isRelative()) {
    absolute = folder.isEmpty() ? QFileInfo(path).absoluteFilePath() : QDir(folder).filePath(path);
  } else if (!folder.isEmpty()) {
    const QString relative = QDir(folder).relativeFilePath(path);
    if (QFileInfo(relative).isRelative()) {
      path = relative;
    }
  }
  QJsonObject texture{{QStringLiteral("path"), path},
                      {QStringLiteral("size"), QJsonArray{m_textureWidth->value(), m_textureHeight->value()}},
                      {QStringLiteral("rotation"), m_textureRotation->value() * kDegree},
                      {QStringLiteral("projection"), m_textureProjection->currentData().toString()}};
  if (a->hasTexture && a->texture.embedded) {
    // A new image is embedded as the old one was.
    QFile file(absolute);
    if (!file.open(QIODevice::ReadOnly) || file.size() > kMaxEmbedded) {
      m_message->setText(tr("The image %1 cannot be read to embed it.").arg(QDir::toNativeSeparators(absolute)));
      showCurrent();
      return;
    }
    texture.insert(QStringLiteral("data"), QString::fromLatin1(file.readAll().toBase64()));
  }
  sendTexture(texture, QStringLiteral("path %1").arg(path));
}

void AppearanceDialog::chooseTexture() {
  const Appearance* a = current();
  if (a == nullptr || a->library) {
    return;
  }
  const QString start = a->hasTexture && !a->texture.file.isEmpty() ? QFileInfo(a->texture.file).absolutePath()
                                                                     : m_host.documentFolder();
  const QString path = QFileDialog::getOpenFileName(this, tr("Texture Image"), start,
                                                    tr("Images (*.png *.jpg *.jpeg);;All files (*)"));
  if (!path.isEmpty()) {
    setTexturePath(path);
  }
}

void AppearanceDialog::changeTexture(const QString& key, const QJsonValue& value) {
  const Appearance* a = current();
  if (a == nullptr || a->library || !a->hasTexture) {
    return;
  }
  // The texture as the model lists it, so an embedded image stays.
  QJsonObject texture = a->texture.json;
  texture.insert(key, value);
  sendTexture(texture, QStringLiteral("%1 %2").arg(key, QString::fromUtf8(QJsonDocument(QJsonArray{value})
                                                                              .toJson(QJsonDocument::Compact))));
}

void AppearanceDialog::embedTexture(bool embed) {
  const Appearance* a = current();
  if (a == nullptr || a->library || !a->hasTexture || embed == a->texture.embedded) {
    return;
  }
  QJsonObject texture = a->texture.json;
  texture.remove(QStringLiteral("embedded"));
  texture.remove(QStringLiteral("image_sha256"));
  if (embed) {
    QFile file(a->texture.file);
    if (a->texture.file.isEmpty() || !file.open(QIODevice::ReadOnly) || file.size() > kMaxEmbedded) {
      m_message->setText(tr("The image cannot be embedded: %1.")
                             .arg(a->texture.missing.isEmpty() ? tr("it cannot be read or is larger than 32 MB")
                                                               : a->texture.missing));
      showCurrent();
      return;
    }
    texture.insert(QStringLiteral("data"), QString::fromLatin1(file.readAll().toBase64()));
  } else {
    // The image file at the path again: it must be there.
    AppearanceTexture external = a->texture;
    external.embedded = false;
    resolveTexture(external, m_host.documentFolder(), {});
    if (external.file.isEmpty()) {
      m_message->setText(tr("The image stays embedded: %1. Choose an image file first.").arg(external.missing));
      showCurrent();
      return;
    }
  }
  sendTexture(texture, embed ? QStringLiteral("embedded") : QStringLiteral("not embedded"));
}

void AppearanceDialog::sendTexture(const QJsonValue& texture, const QString& what) {
  const Appearance* a = current();
  if (a == nullptr || a->library) {
    return;
  }
  if (!canChange()) {
    showCurrent();
    return;
  }
  const QString id = a->id;
  if (m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("edit_appearance")},
                              {QStringLiteral("id"), id},
                              {QStringLiteral("texture"), texture}})) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Appearance %1 texture %2").arg(id, what);
  } else {
    m_message->setText(m_host.lastError());
    qDebug().noquote() << QStringLiteral("Appearance %1 texture %2 refused: %3").arg(id, what, m_host.lastError());
  }
  refresh();
}

void AppearanceDialog::logLayout() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  for (int i = 0; i < m_list->count(); ++i) {
    QListWidgetItem* item = m_list->item(i);
    const QString id = item->data(kIdRole).toString();
    const QRect rect = m_list->visualItemRect(item);
    if (!id.isEmpty() && m_list->viewport()->rect().contains(rect.center())) {
      lines << QStringLiteral("Appearances item %1 at %2").arg(id, at(m_list->viewport(), rect.center()));
    }
  }
  lines << QStringLiteral("Appearances field name at %1").arg(at(m_name, m_name->rect().center()));
  for (const NumberField& field : std::as_const(m_numbers)) {
    lines << QStringLiteral("Appearances field %1 at %2")
                 .arg(QString::fromLatin1(field.key), at(field.box, field.box->rect().center()));
  }
  for (const ColorField& field : std::as_const(m_colors)) {
    lines << QStringLiteral("Appearances field %1 at %2")
                 .arg(QString::fromLatin1(field.key), at(field.hex, field.hex->rect().center()));
  }
  for (const auto& [name, field] : {std::pair<const char*, QWidget*>{"texture_path", m_texturePath},
                                    {"texture_width", m_textureWidth},
                                    {"texture_height", m_textureHeight},
                                    {"texture_rotation", m_textureRotation},
                                    {"texture_projection", m_textureProjection},
                                    {"texture_embed", m_textureEmbed},
                                    {"texture_remove", m_textureRemove}}) {
    lines << QStringLiteral("Appearances field %1 at %2").arg(QString::fromLatin1(name), at(field, field->rect().center()));
  }
  for (int i = 0; i < m_faces->count(); ++i) {
    QListWidgetItem* item = m_faces->item(i);
    const QRect rect = m_faces->visualItemRect(item);
    if (m_faces->viewport()->rect().contains(rect.center())) {
      lines << QStringLiteral("Appearances face %1 %2 %3 at %4")
                   .arg(item->data(kBodyRole).toString(), item->data(kFaceRole).toString(),
                        item->data(kIdRole).toString(), at(m_faces->viewport(), rect.center()));
    }
  }
  for (const auto& [name, button] : {std::pair<const char*, QPushButton*>{"new", m_new},
                                     {"delete", m_delete},
                                     {"assign", m_assign},
                                     {"clear_faces", m_clearFaces},
                                     {"close", m_close}}) {
    lines << QStringLiteral("Appearances %1 at %2").arg(QString::fromLatin1(name), at(button, button->rect().center()));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
