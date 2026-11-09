// SPDX-License-Identifier: MIT
#include "AppearanceDialog.hpp"

#include <algorithm>
#include <initializer_list>
#include <utility>

#include <QCheckBox>
#include <QColorDialog>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QFont>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHash>
#include <QHideEvent>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QListView>
#include <QMessageBox>
#include <QPixmap>
#include <QPushButton>
#include <QSettings>
#include <QSignalBlocker>
#include <QStackedWidget>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/FitToScreen.hpp"
#include "../framework/Icons.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/NativeDialogs.hpp"
#include "../materials/TextureBrowser.hpp"
#include "../materials/TextureUse.hpp"
#include "../print/AppearancePicker.hpp"
#include "../print/PrintLibrary.hpp"
#include "../platform/MacDialogs.hpp"
#include "DocumentHost.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad {
namespace {

constexpr int kIdRole = Qt::UserRole;
// A row's group (mitcad#1133): the key of its section, on its heading and
// its appearances; and the heading's title.
constexpr int kGroupRole = Qt::UserRole + 1;
constexpr int kTitleRole = Qt::UserRole + 2;
// The collapsed groups' keys, comma-separated (mitcad#1133).
const char* kCollapsedSetting = "appearances/collapsedGroups";
constexpr int kPreviewSize = 120;
constexpr double kDegree = 3.14159265358979323846 / 180.0;
// The largest image the design file embeds (core/model's
// MAX_EMBEDDED_IMAGE).
constexpr qint64 kMaxEmbedded = qint64(32) << 20;
// The library's swatches at Icon size (mitcad#1556).
int swatchIconSize() { return uiIconSize(22); }
// The size the dialog opens at where the screen has room for it.
constexpr QSize kPreferredSize(820, 780);

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
  resize(kPreferredSize);
  auto* layout = new QVBoxLayout(this);
  // The body scrolls on a screen too low for it, so that the dialog fits
  // the screen and the buttons below the body stay visible (mitcad#818).
  // Keep this boundary in both frontends: the dialog buttons must remain
  // outside the long material form.
  auto* scroll = new FittingScrollArea;
  scroll->setObjectName(QStringLiteral("appearanceBody"));
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  auto* body = new QWidget;
  auto* bodyLayout = new QVBoxLayout(body);
  bodyLayout->setContentsMargins(0, 0, 0, 0);
  scroll->setWidget(body);
  layout->addWidget(scroll, 1);
  m_bodyView = scroll->viewport();
  auto* columns = new QHBoxLayout;
  bodyLayout->addLayout(columns, 1);

  // The browser (mitcad#315): the appearances or a texture source, a search
  // and a category.
  auto* left = new QVBoxLayout;
  columns->addLayout(left, 1);
  m_source = new QComboBox;
  m_source->setToolTip(tr("Where to look: the appearances of the library and this design, or a texture source"));
  left->addWidget(m_source);
  auto* searchRow = new QHBoxLayout;
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("appearanceSearch"));
  m_search->setPlaceholderText(tr("Search"));
  m_search->setClearButtonEnabled(true);
  mac::styleSearchField(m_search);
  m_category = new QComboBox;
  m_category->setToolTip(tr("Show one category only"));
  searchRow->addWidget(m_search, 1);
  searchRow->addWidget(m_category);
  left->addLayout(searchRow);
  m_pages = new QStackedWidget;
  left->addWidget(m_pages, 1);
  m_searchTimer = new QTimer(this);
  m_searchTimer->setSingleShot(true);
  m_searchTimer->setInterval(250);

  m_list = new QListWidget;
  m_list->setObjectName(QStringLiteral("appearanceLibrary"));
  m_list->setIconSize(QSize(swatchIconSize(), swatchIconSize()));
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  m_list->setMinimumWidth(300);
  m_list->installEventFilter(this);
  // The AppKit frontend (mitcad#1355): Up and Down reach the group
  // headings, and these keys on a heading reach eventFilter.
  m_list->setProperty(NativeHeadingRowsProperty, true);
  m_list->setProperty(NativeModelKeysProperty, QVariantList{int(Qt::Key_Space), int(Qt::Key_Return),
                                                            int(Qt::Key_Enter), int(Qt::Key_Left),
                                                            int(Qt::Key_Right)});
  m_pages->addWidget(m_list);
  // The groups the user collapsed (mitcad#1133).
  m_collapsed =
      QSettings().value(QLatin1String(kCollapsedSetting)).toString().split(QLatin1Char(','), Qt::SkipEmptyParts);
  m_textures = new TextureBrowser;
  m_pages->addWidget(m_textures);

  auto* right = new QVBoxLayout;
  columns->addLayout(right, 1);
  m_preview = new QLabel;
  m_preview->setFixedSize(kPreviewSize, kPreviewSize);
  m_preview->setToolTip(tr("Preview: the rendered view shows the appearance lit by the sky and the sun"));
  right->addWidget(m_preview, 0, Qt::AlignHCenter);
  auto* form = new QFormLayout;
  right->addLayout(form);
  m_name = new QLineEdit;
  m_name->setObjectName(QStringLiteral("appearanceName"));
  form->addRow(tr("Name"), m_name);
  // What it is a look of (mitcad#1102): a material's Add… and the
  // Appearance tool offer the material's class first. The library's have
  // theirs.
  m_class = new QComboBox;
  m_class->setObjectName(QStringLiteral("appearanceClass"));
  for (const QString& materialClass : materialClasses()) {
    m_class->addItem(materialClassTitle(materialClass), materialClass);
  }
  m_class->setToolTip(tr("What it is a look of: the pickers offer a material's class first"));
  form->addRow(tr("Class"), m_class);
  addColor(form, "base_color", tr("Base color"));
  addNumber(form, "metalness", tr("Metalness"), 0.0, 1.0, 0.05, tr("0 for plastics, paint and glass, 1 for metals"));
  addNumber(form, "roughness", tr("Roughness"), 0.0, 1.0, 0.05, tr("0 for a mirror finish, 1 for a fully matt one"));
  addNumber(form, "specular", tr("Specular"), 0.0, 1.0, 0.05, tr("The strength of a non-metal's reflection"));
  addNumber(form, "transmission", tr("Transmission"), 0.0, 1.0, 0.05,
            tr("Light going through: 1 for glass and clear plastic"));
  addNumber(form, "ior", tr("Index of refraction"), 1.0, 5.0, 0.05, tr("1.5 for glass and most plastics"));
  addNumber(form, "coat", tr("Coat"), 0.0, 1.0, 0.05, tr("A clear lacquer over the surface"));
  addNumber(form, "coat_roughness", tr("Coat roughness"), 0.0, 1.0, 0.05, QString());
  addNumber(form, "emission", tr("Emission"), 0.0, 1000.0, 0.5, tr("Light the surface gives off; 0 for none"));
  addColor(form, "emission_color", tr("Emission color"));
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
      tr("A PNG or JPEG image for the base color, relative to the design's folder or absolute"));
  m_textureChoose = new QPushButton(tr("Choose…"));
  m_textureRemove = new QPushButton(tr("Remove"));
  m_textureRemove->setToolTip(tr("No texture: the base color only"));
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
  m_textureEmbed->setObjectName(QStringLiteral("appearanceTextureEmbed"));
  m_textureEmbed->setToolTip(tr("The design file keeps a copy of the image, so it goes with the design"));
  form->addRow(QString(), m_textureEmbed);
  m_textureInfo = new QLabel;
  m_textureInfo->setWordWrap(true);
  right->addWidget(m_textureInfo);
  m_info = new QLabel;
  m_info->setWordWrap(true);
  right->addWidget(m_info);
  right->addStretch(1);

  m_message = new QLabel;
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  layout->addWidget(m_message);

  auto* buttons = new QDialogButtonBox;
  m_new = new QPushButton(themeIcon(QStringLiteral("add")), tr("New"));
  m_new->setObjectName(QStringLiteral("appearanceNew"));
  m_new->setToolTip(tr("A copy of the selected appearance in this design, to edit"));
  m_delete = new QPushButton(themeIcon(QStringLiteral("delete")), tr("Delete"));
  m_delete->setToolTip(tr("Delete the selected appearance of this design; its bodies get the default look"));
  m_close = new QPushButton(tr("Close"));
  m_close->setObjectName(QStringLiteral("appearanceClose"));
  for (QPushButton* button : {m_new, m_delete, m_close}) {
    button->setAutoDefault(false);
  }
#ifdef MITCAD_MAC_UI
  auto* controls = new QHBoxLayout;
  controls->setSpacing(0);
  controls->addWidget(m_new);
  controls->addWidget(m_delete);
  controls->addStretch(1);
  left->addLayout(controls);
#else
  buttons->addButton(m_new, QDialogButtonBox::ResetRole);
  buttons->addButton(m_delete, QDialogButtonBox::ResetRole);
#endif
  mac::styleListButtons(m_new, m_delete);
  mac::stylePreview(m_list);
  buttons->addButton(m_close, QDialogButtonBox::RejectRole);
  layout->addWidget(buttons);
  mac::styleDialogButtons(buttons, m_close);
  // Return commits the active field; closing this editor is an explicit
  // action. The native presenter follows the same default-button state.
  mac::auxiliaryButton(m_close);
  mac::styleDialog(this);

  connect(m_close, &QPushButton::clicked, this, &QDialog::accept);
  connect(m_new, &QPushButton::clicked, this, &AppearanceDialog::newAppearance);
  connect(m_delete, &QPushButton::clicked, this, &AppearanceDialog::deleteAppearance);
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
  // A click on a group's heading collapses or expands it (mitcad#1133).
  connect(m_list, &QListWidget::itemClicked, this, [this](QListWidgetItem* item) {
    const QString key = item != nullptr && item->data(kIdRole).toString().isEmpty()
                            ? item->data(kGroupRole).toString()
                            : QString();
    if (!key.isEmpty()) {
      setGroupCollapsed(key, !m_collapsed.contains(key));
      // The click took the current row: the appearance shown stays the
      // current one, without scrolling away from the heading.
      for (int i = 0; !m_current.isEmpty() && i < m_list->count(); ++i) {
        if (m_list->item(i)->data(kIdRole).toString() == m_current) {
          m_building = true;
          m_list->setAutoScroll(false);
          m_list->setCurrentRow(i);
          m_list->setAutoScroll(true);
          m_building = false;
          break;
        }
      }
    }
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
  connect(m_textureRemove, &QPushButton::clicked, this, [this] { sendTexture(QJsonValue(), QStringLiteral("removed")); });
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
  connect(m_class, &QComboBox::activated, this, [this](int index) {
    const Appearance* a = current();
    const QString chosen = m_class->itemData(index).toString();
    if (!m_building && a != nullptr && !a->library && chosen != a->materialClass) {
      change(QStringLiteral("class"), chosen);
    }
  });
  connect(m_name, &QLineEdit::editingFinished, this, [this] {
    const Appearance* a = current();
    if (!m_building && a != nullptr && !a->library && m_name->text().trimmed() != a->name) {
      change(QStringLiteral("name"), m_name->text().trimmed());
    }
  });
  // Texture sources (mitcad#315).
  connect(m_source, &QComboBox::currentIndexChanged, this, [this] {
    if (!m_building) {
      sourceChosen();
    }
  });
  connect(m_category, &QComboBox::currentIndexChanged, this, [this] {
    if (!m_building) {
      runSearch();
    }
  });
  connect(m_search, &QLineEdit::textChanged, this, [this] { m_searchTimer->start(); });
  connect(m_search, &QLineEdit::returnPressed, this, [this] {
    m_searchTimer->stop();
    runSearch();
  });
  connect(m_searchTimer, &QTimer::timeout, this, &AppearanceDialog::runSearch);
  connect(m_textures, &TextureBrowser::useRequested, this, &AppearanceDialog::useTexture);
  connect(m_textures, &TextureBrowser::placesChanged, this,
          [this] { TestSync::singleShot(0, this, [this] { logLayout(); }); });
  loadSources();
  refresh();
}

AppearanceDialog::~AppearanceDialog() = default;

void AppearanceDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  // Within the screen's available area, title bar included (mitcad#818):
  // first at its preferred size where that fits, later as the user left
  // it; the body scrolls when the screen is lower. An AppKit presentation
  // (not shown on screen) fits its own window.
  if (!testAttribute(Qt::WA_DontShowOnScreen)) {
    if (!m_fitted) {
      m_fitted = true;
      fitWindowToContents(this, FitMode::Fit, kPreferredSize);
    } else {
      capWindowToScreen(this);
    }
    keepWindowOnScreen(this);
  }
  // Preferences may have changed the texture folders meanwhile.
  if (TextureSettings::load() != m_textureSettings) {
    loadSources();
  }
}

void AppearanceDialog::hideEvent(QHideEvent* event) {
  QDialog::hideEvent(event);
  if (event->spontaneous()) {
    return; // the main window minimised, not the window closed
  }
  qDebug().noquote() << "Appearances closed";
  // Once (mitcad#1415): taken first, so that what it calls may set another.
  std::function<void()> closed;
  closed.swap(whenClosed);
  if (closed) {
    closed();
  }
}

void AppearanceDialog::showAppearance(const QString& id) {
  const Appearance* found = findAppearance(m_appearances, id);
  if (found == nullptr) {
    return;
  }
  // Its group open, so that its row shows (mitcad#1133).
  for (int i = 0; i < m_list->count(); ++i) {
    const QListWidgetItem* item = m_list->item(i);
    if (item->data(kIdRole).toString() == id) {
      setGroupCollapsed(item->data(kGroupRole).toString(), false);
      break;
    }
  }
  m_current = id;
  select(id);
  showCurrent();
  qDebug().noquote() << QStringLiteral("Appearances on %1").arg(id);
}

void AppearanceDialog::resizeEvent(QResizeEvent* event) {
  QDialog::resizeEvent(event);
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void AppearanceDialog::loadSources() {
  const QString shown = currentSource() != nullptr ? currentSource()->id() : QString();
  m_textureSettings = TextureSettings::load();
  // The browser lets go of the old sources first; a Use still fetching from
  // one never answers.
  m_textures->search(nullptr, QString(), QString());
  m_using = false;
  m_textures->setBusy(QString());
  m_sources = makeTextureSources(m_textureSettings);
  m_building = true;
  m_source->clear();
  m_source->addItem(themeIcon(QStringLiteral("appearance")), tr("Appearances: Library and This Design"));
  int index = 0;
  for (std::size_t i = 0; i < m_sources.size(); ++i) {
    m_source->addItem(m_sources[i]->name(), m_sources[i]->id());
    if (m_sources[i]->id() == shown) {
      index = static_cast<int>(i) + 1;
    }
  }
  m_source->setCurrentIndex(index);
  m_building = false;
  QStringList ids;
  for (const auto& source : m_sources) {
    ids << source->id();
  }
  qDebug().noquote() << QStringLiteral("Appearances texture sources: %1 (%2 folders)")
                            .arg(ids.isEmpty() ? QStringLiteral("none") : ids.join(QStringLiteral(", ")))
                            .arg(m_textureSettings.folders.size());
  sourceChosen();
}

TextureSource* AppearanceDialog::currentSource() const {
  const int index = m_source != nullptr ? m_source->currentIndex() - 1 : -1;
  return index >= 0 && index < static_cast<int>(m_sources.size()) ? m_sources[std::size_t(index)].get() : nullptr;
}

void AppearanceDialog::sourceChosen() {
  TextureSource* source = currentSource();
  if (source != nullptr && source->usesNetwork() && !m_textureSettings.allowNetwork) {
    // Nothing goes to the network before the user allows it.
    const auto answer =
        sheetQuestion(this, tr("Texture Sources on the Internet"),
                      tr("%1 is searched on the internet. Allow texture sources on the internet? (Preferences, "
                         "Textures changes this later.)")
                          .arg(source->name()),
                      QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
    if (answer != QMessageBox::Yes) {
      const QSignalBlocker block(m_source);
      m_source->setCurrentIndex(m_shownSource);
      return;
    }
    m_textureSettings.allowNetwork = true;
    TextureSettings saved = TextureSettings::load();
    saved.allowNetwork = true;
    saved.save();
    qDebug().noquote() << "Appearances: texture sources on the internet allowed";
  }
  m_shownSource = m_source->currentIndex();
  m_pages->setCurrentIndex(source != nullptr ? 1 : 0);
  qDebug().noquote() << QStringLiteral("Appearances source: %1")
                            .arg(source != nullptr ? source->id() : QStringLiteral("appearances"));
  showCategories();
  runSearch();
}

void AppearanceDialog::showCategories() {
  const QString kept = m_category->currentData().toString();
  const auto fill = [this, kept](const QStringList& names) {
    m_building = true;
    m_category->clear();
    m_category->addItem(tr("All Categories"), QString());
    for (const QString& name : names) {
      m_category->addItem(name, name);
    }
    m_category->setCurrentIndex(std::max(0, m_category->findData(kept)));
    m_building = false;
  };
  TextureSource* source = currentSource();
  if (source == nullptr) {
    fill({tr("Library"), tr("This Design")});
    return;
  }
  fill({});
  const QString id = source->id();
  source->categories(this, [this, fill, id](const QStringList& names, const QString& error) {
    TextureSource* now = currentSource();
    if (now == nullptr || now->id() != id) {
      return;
    }
    if (!error.isEmpty()) {
      m_textures->setMessage(error, true);
      return;
    }
    const QString before = m_category->currentData().toString();
    fill(names);
    if (m_category->currentData().toString() != before) {
      runSearch();
    }
  });
}

void AppearanceDialog::runSearch() {
  m_searchTimer->stop();
  TextureSource* source = currentSource();
  if (source == nullptr) {
    filterList();
    return;
  }
  m_textures->search(source, m_search->text().trimmed(), m_category->currentData().toString());
}

void AppearanceDialog::filterList() {
  // The appearances whose name has every word typed, in the category.
  const QStringList words = m_search->text().toLower().split(QLatin1Char(' '), Qt::SkipEmptyParts);
  const int category = m_category->currentIndex(); // 0 all, 1 the library, 2 this design
  // Each group's heading shows while one of its appearances matches, with
  // the count of those, and ▾ open or ▸ collapsed (mitcad#1133); a
  // collapsed group hides its rows.
  QListWidgetItem* header = nullptr;
  int matches = 0;
  QStringList groups;
  const auto closeGroup = [&header, &matches, &groups, this] {
    if (header == nullptr) {
      return;
    }
    const bool collapsed = m_collapsed.contains(header->data(kGroupRole).toString());
    const QString title = header->data(kTitleRole).toString();
    header->setText(QStringLiteral("%1 %2 (%3)")
                        .arg(collapsed ? QStringLiteral("▸") : QStringLiteral("▾"), title)
                        .arg(matches));
    header->setToolTip(collapsed ? tr("Show the %1 appearances").arg(title)
                                 : tr("Hide the %1 appearances").arg(title));
    header->setHidden(matches == 0);
    if (matches > 0) {
      groups << QStringLiteral("%1 %2").arg(title).arg(matches);
    }
  };
  int shown = 0;
  for (int i = 0; i < m_list->count(); ++i) {
    QListWidgetItem* item = m_list->item(i);
    const QString id = item->data(kIdRole).toString();
    if (id.isEmpty()) {
      closeGroup();
      header = item;
      matches = 0;
      continue;
    }
    const Appearance* a = findAppearance(m_appearances, id);
    const QString name = item->text().toLower();
    bool visible = std::all_of(words.cbegin(), words.cend(), [&name](const QString& w) { return name.contains(w); });
    if (a != nullptr && category == 1) {
      visible = visible && a->library;
    } else if (a != nullptr && category == 2) {
      visible = visible && !a->library;
    }
    item->setHidden(!visible || m_collapsed.contains(item->data(kGroupRole).toString()));
    matches += visible ? 1 : 0;
    shown += visible ? 1 : 0;
  }
  closeGroup();
  if (!words.isEmpty() || category != 0) {
    qDebug().noquote() << QStringLiteral("Appearances filtered \"%1\" (%2): %3 shown")
                              .arg(m_search->text().trimmed(), m_category->currentText())
                              .arg(shown);
    qDebug().noquote() << QStringLiteral("Appearances groups shown: %1")
                              .arg(groups.isEmpty() ? QStringLiteral("none") : groups.join(QStringLiteral(" | ")));
  }
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

void AppearanceDialog::setGroupCollapsed(const QString& key, bool collapsed) {
  if (key.isEmpty() || m_collapsed.contains(key) == collapsed) {
    return;
  }
  if (collapsed) {
    m_collapsed << key;
  } else {
    m_collapsed.removeAll(key);
  }
  m_collapsed.sort();
  QSettings settings;
  if (m_collapsed.isEmpty()) {
    settings.remove(QLatin1String(kCollapsedSetting));
  } else {
    settings.setValue(QLatin1String(kCollapsedSetting), m_collapsed.join(QLatin1Char(',')));
  }
  QString title = key;
  for (int i = 0; i < m_list->count(); ++i) {
    const QListWidgetItem* item = m_list->item(i);
    if (item->data(kIdRole).toString().isEmpty() && item->data(kGroupRole).toString() == key) {
      title = item->data(kTitleRole).toString();
      break;
    }
  }
  qDebug().noquote() << QStringLiteral("Appearances group %1 %2")
                            .arg(title, collapsed ? QStringLiteral("collapsed") : QStringLiteral("expanded"));
  filterList();
}

bool AppearanceDialog::eventFilter(QObject* watched, QEvent* event) {
  // Space, Return, Left and Right on a group's heading (mitcad#1133).
  if (watched == m_list && event->type() == QEvent::KeyPress) {
    const QListWidgetItem* item = m_list->currentItem();
    const QString key = item != nullptr && item->data(kIdRole).toString().isEmpty()
                            ? item->data(kGroupRole).toString()
                            : QString();
    if (!key.isEmpty()) {
      switch (static_cast<QKeyEvent*>(event)->key()) {
      case Qt::Key_Space:
      case Qt::Key_Return:
      case Qt::Key_Enter:
        setGroupCollapsed(key, !m_collapsed.contains(key));
        return true;
      case Qt::Key_Left:
        setGroupCollapsed(key, true);
        return true;
      case Qt::Key_Right:
        setGroupCollapsed(key, false);
        return true;
      default:
        break;
      }
    }
  }
  return QDialog::eventFilter(watched, event);
}

void AppearanceDialog::useTexture(const TextureAsset& asset, const QString& resolution) {
  TextureSource* source = currentSource();
  if (source == nullptr || m_using || !canChange()) {
    return;
  }
  m_using = true;
  m_message->clear();
  const QString sourceId = source->id();
  qDebug().noquote() << QStringLiteral("Texture fetching %1 %2 %3")
                            .arg(sourceId, asset.recorded(),
                                 resolution.isEmpty() ? QStringLiteral("original") : resolution);
  m_textures->setBusy(tr("Fetching %1…").arg(asset.name));
  source->fetch(
      asset, resolution, this,
      [this, name = asset.name](qint64 done, qint64 total) {
        m_textures->setBusy(total > 0 ? tr("Fetching %1: %2 of %3…")
                                            .arg(name, textureBytesText(done), textureBytesText(total))
                                      : tr("Fetching %1: %2…").arg(name, textureBytesText(done)));
      },
      [this, sourceId, asset](const TextureFiles& files, const QString& error) {
        finishUse(sourceId, asset, files, error);
      });
}

void AppearanceDialog::finishUse(const QString& sourceId, const TextureAsset& asset, const TextureFiles& files,
                                 const QString& error) {
  m_using = false;
  m_textures->setBusy(QString());
  const auto refuse = [this](const QString& why) {
    m_message->setText(why);
    m_textures->setMessage(why, true);
    qDebug().noquote() << QStringLiteral("Texture not used: %1").arg(why);
  };
  if (!error.isEmpty()) {
    refuse(error);
    return;
  }
  if (!canChange()) {
    return;
  }
  // A new name: "Oak veneer", "Oak veneer 2", ...
  QString name = asset.name.trimmed().isEmpty() ? tr("Texture") : asset.name.trimmed();
  const QString stem = name;
  for (int n = 2; std::any_of(m_appearances.begin(), m_appearances.end(),
                              [&name](const Appearance& other) { return other.name == name; });
       ++n) {
    name = QStringLiteral("%1 %2").arg(stem).arg(n);
  }
  QJsonArray listed;
  try {
    listed = m_host.model().queryArray(QStringLiteral("appearances"));
  } catch (const std::exception& e) {
    refuse(QString::fromUtf8(e.what()));
    return;
  }
  const TextureUse use = textureUseCommand(sourceId, asset, files, name, embeddedImageDigests(listed));
  if (!use.error.isEmpty()) {
    refuse(use.error);
    return;
  }
  QJsonObject result;
  if (!m_host.runModelCommand(use.command, &result)) {
    refuse(m_host.lastError());
    return;
  }
  m_message->clear();
  m_current = result.value(QStringLiteral("id")).toString();
  qDebug().noquote() << QStringLiteral("Texture used: %1 %2 %3 as %4 (%5 maps; %6 images embedded, %7 shared)")
                            .arg(sourceId, asset.recorded(),
                                 files.resolution.isEmpty() ? QStringLiteral("original") : files.resolution,
                                 m_current)
                            .arg(files.maps.size())
                            .arg(use.sent)
                            .arg(use.shared);
  m_textures->setMessage(tr("%1 is now an appearance of this design.").arg(name), false);
  refresh();
}

void AppearanceDialog::addNumber(QFormLayout* form, const char* key, const QString& label, double low, double high,
                                 double step, const QString& tip) {
  auto* box = new QDoubleSpinBox;
  box->setObjectName(QStringLiteral("appearance_") + QString::fromLatin1(key));
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
  button->setToolTip(tr("Choose a color"));
  auto* hex = new QLineEdit;
  hex->setObjectName(QStringLiteral("appearance_") + QString::fromLatin1(key));
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
      m_message->setText(tr("“%1” is no color: write it as #rrggbb.").arg(field.hex->text()));
      showCurrent();
      return;
    }
    const QColor old = qstrcmp(field.key, "base_color") == 0 ? a->baseColor : a->emissionColor;
    if (hexOf(color) != hexOf(old)) {
      change(QString::fromLatin1(field.key), colorJson(color));
    }
  });
}

const Appearance* AppearanceDialog::current() const { return findAppearance(m_appearances, m_current); }

bool AppearanceDialog::canChange() {
  // Not in the middle of a command or a sketch, whose undo steps the
  // change would come between; but beside the Material and Appearance
  // tools, which preview nothing and run their commands on OK (mitcad#1415:
  // Edit Appearances… of the Appearance tool's picker, as the print
  // managers beside them).
  if (m_host.canChangePrintSettings()) {
    return true;
  }
  m_message->setText(m_host.changeBlockedReason());
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
  // The design's own first, then the library's by class, each group under
  // its heading (mitcad#1133; filterList writes the heading's text).
  const auto header = [this](const AppearanceSection& group) {
    auto* item = new QListWidgetItem(group.title, m_list);
    item->setFlags(Qt::ItemIsEnabled); // clicked, never selected
    item->setData(kGroupRole, group.key);
    item->setData(kTitleRole, group.title);
    item->setData(NativeControlIdentifierRole, QStringLiteral("appearance.group.") + group.key);
    QFont font = item->font();
    font.setBold(true);
    item->setFont(font);
  };
  const qreal ratio = devicePixelRatioF();
  const QVector<AppearanceSection> groups = appearanceSections(m_appearances, QString(), tr("This Design"));
  QStringList members;
  for (const AppearanceSection& group : groups) {
    header(group);
    QStringList ids;
    for (const Appearance* a : group.appearances) {
      auto* item =
          new QListWidgetItem(QIcon(QPixmap::fromImage(appearanceSwatch(*a, swatchIconSize(), ratio))), a->name,
                              m_list);
      item->setData(kIdRole, a->id);
      item->setData(kGroupRole, group.key);
      item->setData(NativeControlIdentifierRole, QStringLiteral("appearance.") + a->id);
      if (!a->bodies.isEmpty() || !a->faces.isEmpty()) {
        item->setToolTip(tr("Used by %n body(ies)", nullptr, static_cast<int>(a->bodies.size())) +
                         (a->faces.isEmpty() ? QString()
                                             : tr(" and %n face(s)", nullptr, static_cast<int>(a->faces.size()))));
      }
      ids << a->id;
    }
    members << QStringLiteral("Appearances group %1: %2").arg(group.title, ids.join(QStringLiteral(", ")));
  }
  m_building = false;
  // The groups and their appearances, logged when they change.
  const QString groupsLine = QStringLiteral("Appearances groups: %1").arg(appearanceSectionsText(groups));
  const QString logged = groupsLine + QLatin1Char('\n') + members.join(QLatin1Char('\n'));
  if (logged != m_loggedGroups) {
    m_loggedGroups = logged;
    qDebug().noquote() << groupsLine;
    for (const QString& line : std::as_const(members)) {
      qDebug().noquote() << line;
    }
  }
  // The search and the category (mitcad#315) hide what does not match.
  filterList();
  select(m_current);
  showCurrent();
}

void AppearanceDialog::showCurrent() {
  const Appearance* a = current();
  m_building = true;
  const bool editable = a != nullptr && !a->library;
  m_name->setEnabled(editable);
  m_name->setText(a != nullptr ? a->name : QString());
  m_class->setEnabled(editable);
  // Other for one from a model before classes.
  const QString shownClass = a == nullptr || a->materialClass.isEmpty() ? QStringLiteral("other") : a->materialClass;
  m_class->setCurrentIndex(a == nullptr ? -1 : m_class->findData(shownClass));
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
    lines << (a->library ? tr("MitCad's library: read-only. New makes a copy in this design to edit.")
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
  const QColor color = dialogColor(base ? a->baseColor : a->emissionColor, this,
                                              base ? tr("Base Color") : tr("Emission Color"));
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
  m_textureInfo->setToolTip(QString());
  if (!has) {
    m_textureInfo->setText(editable ? tr("No texture: choose a PNG or JPEG image for the base color.") : QString());
  } else if (!texture.missing.isEmpty()) {
    m_textureInfo->setText(tr("The texture is not drawn: %1. The rendered view shows the base color.")
                               .arg(texture.missing));
    qDebug().noquote() << QStringLiteral("Appearance %1 texture missing: %2").arg(a->id, texture.missing);
  } else {
    m_textureInfo->setText((texture.embedded ? tr("The image is embedded in the design. ") : QString()) +
                           tr("The rendered view draws the texture; the shaded view shows the base color."));
  }
  if (!has) {
    return;
  }
  // The set's other maps and where it came from (mitcad#313, mitcad#315).
  QStringList maps;
  for (const QString& map : textureSetMapNames()) {
    if (map != QLatin1String("base_color") && texture.json.value(map).isObject()) {
      maps << map;
    }
  }
  const QJsonObject source = texture.json.value(QStringLiteral("source")).toObject();
  QStringList lines;
  if (!maps.isEmpty()) {
    lines << tr("Maps: base color, %1%2.")
                 .arg(maps.join(QStringLiteral(", ")), texture.embedded ? tr(" (embedded)") : QString());
  }
  if (!source.isEmpty()) {
    QString from = source.value(QStringLiteral("asset")).toString();
    QString sourceName = source.value(QStringLiteral("id")).toString();
    for (const auto& known : m_sources) {
      if (known->id() == sourceName) {
        sourceName = known->name();
      }
    }
    QStringList details;
    for (const char* key : {"resolution", "license"}) {
      const QString value = source.value(QLatin1String(key)).toString();
      if (!value.isEmpty()) {
        details << value;
      }
    }
    lines << tr("From %1: %2%3.")
                 .arg(sourceName, from,
                      details.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(details.join(QStringLiteral(", "))));
    const QString url = source.value(QStringLiteral("url")).toString();
    if (!url.isEmpty()) {
      lines.last() += QLatin1Char(' ') + url;
    }
  }
  if (!lines.isEmpty() && texture.missing.isEmpty()) {
    // In the place of the general line, which becomes its tooltip, so the
    // dialog stays as high as before.
    m_textureInfo->setToolTip(m_textureInfo->text());
    m_textureInfo->setText(lines.join(QLatin1Char('\n')));
  }
  const QString logged =
      QStringLiteral("Appearance %1 texture: %2, maps %3, source %4")
          .arg(a->id, texture.embedded ? QStringLiteral("embedded") : QStringLiteral("file"),
               maps.isEmpty() ? QStringLiteral("none") : maps.join(QLatin1Char(' ')),
               source.isEmpty() ? QStringLiteral("none")
                                : QStringLiteral("%1 %2 %3")
                                      .arg(source.value(QStringLiteral("id")).toString(),
                                           source.value(QStringLiteral("asset")).toString(),
                                           source.value(QStringLiteral("resolution")).toString())
                                      .trimmed());
  if (logged != m_loggedTexture) {
    m_loggedTexture = logged;
    qDebug().noquote() << logged;
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
  // The Texture group's last folder (mitcad#498), else the texture's, else
  // the design's.
  const QString fallback = a->hasTexture && !a->texture.file.isEmpty() ? QFileInfo(a->texture.file).absolutePath()
                                                                        : m_host.documentFolder();
  const QString start = dialogStartFolder(DialogGroup::Texture, fallback);
  const QString path =
      dialogOpenFileName(this, tr("Texture Image"), start, tr("Images (*.png *.jpg *.jpeg);;All files (*)"));
  if (!path.isEmpty()) {
    rememberDialogFolder(DialogGroup::Texture, path);
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
  for (int i = 0; m_pages->currentWidget() == m_list && i < m_list->count(); ++i) {
    QListWidgetItem* item = m_list->item(i);
    const QString id = item->data(kIdRole).toString();
    const QRect rect = m_list->visualItemRect(item);
    // In sight in the list and in the body, which scrolls on a low screen.
    const QPoint inBody = m_list->viewport()->mapTo(m_bodyView, rect.center());
    if (item->isHidden() || !m_list->viewport()->rect().contains(rect.center()) ||
        !m_bodyView->rect().contains(inBody)) {
      continue;
    }
    if (!id.isEmpty()) {
      lines << QStringLiteral("Appearances item %1 at %2").arg(id, at(m_list->viewport(), rect.center()));
    } else {
      // A group's heading (mitcad#1133), by its key.
      lines << QStringLiteral("Appearances group %1 at %2")
                   .arg(item->data(kGroupRole).toString(), at(m_list->viewport(), rect.center()));
    }
  }
  // The browser (mitcad#315).
  lines << QStringLiteral("Appearances source at %1").arg(at(m_source, m_source->rect().center()));
  lines << QStringLiteral("Appearances search at %1").arg(at(m_search, m_search->rect().center()));
  lines << QStringLiteral("Appearances category at %1").arg(at(m_category, m_category->rect().center()));
  if (m_pages->currentWidget() == m_textures) {
    for (const auto& [name, center] : m_textures->assetsInSight()) {
      lines << QStringLiteral("Appearances texture %1 at %2").arg(name, at(m_textures->view()->viewport(), center));
    }
    QWidget* resolution = m_textures->resolutionBox();
    QWidget* use = m_textures->useButton();
    lines << QStringLiteral("Appearances resolution at %1").arg(at(resolution, resolution->rect().center()));
    lines << QStringLiteral("Appearances use at %1").arg(at(use, use->rect().center()));
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
  for (const auto& [name, button] : {std::pair<const char*, QPushButton*>{"new", m_new},
                                     {"delete", m_delete},
                                     {"close", m_close}}) {
    if (!button->isVisibleTo(this)) {
      continue;
    }
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
