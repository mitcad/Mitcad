// SPDX-License-Identifier: MIT
#include "MaterialsDialog.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <optional>
#include <utility>

#include <QAbstractItemDelegate>
#include <QAbstractItemView>
#include <QAction>
#include <QColor>
#include <QComboBox>
#include <QElapsedTimer>
#include <QCompleter>
#include <QEvent>
#include <QFont>
#include <QFormLayout>
#include <QFrame>
#include <QHash>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QIcon>
#include <QJsonValue>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QModelIndex>
#include <QMessageBox>
#include <QPainter>
#include <QPen>
#include <QPixmap>
#include <QPoint>
#include <QPushButton>
#include <QRect>
#include <QRectF>
#include <QSet>
#include <QSettings>
#include <QSize>
#include <QSignalBlocker>
#include <QSplitter>
#include <QStyle>
#include <QStyleOptionViewItem>
#include <QToolButton>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "../browser/Bindings.hpp"
#include "../browser/DocumentHost.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "../platform/NativeDialogs.hpp"
#include "AppearancePicker.hpp"
#include "MaterialMenuChoices.hpp"
#include "MaterialVariantsPage.hpp"
#include "SettingsForm.hpp"
#include "SlicerImport.hpp"

namespace mitcad {
namespace {

constexpr int kKeyRole = Qt::UserRole; // a row's key; empty on a group's heading
constexpr int kSwatchSize = 20; // a material's list of appearances (mitcad#1055)
constexpr int kListIndent = 12; // the materials under their group's heading (mitcad#1083)
constexpr int kAppearanceRowsShown = 8; // the appearance list grows to this many rows, then scrolls
constexpr int kAppearanceRowsMin = 3;   // ... and is never shorter than this many
constexpr int kAppearanceButtonSpacing = 4; // between Add…, Remove and Make Default

QString text(const QJsonObject& object, const char* field) {
  return object.value(QLatin1String(field)).toString();
}

// The size a row's cell asks for, as its delegate paints it (no
// indentation).
QSize cellHint(const QTreeView* view, const QModelIndex& index) {
  QStyleOptionViewItem option;
  option.initFrom(view);
  option.font = view->font();
  option.decorationSize = view->iconSize();
  option.displayAlignment = Qt::AlignLeft | Qt::AlignVCenter;
  option.widget = view;
  QAbstractItemDelegate* delegate = view->itemDelegateForIndex(index);
  return delegate != nullptr ? delegate->sizeHint(option, index) : QSize();
}

// A tree item's model index in `column` (QTreeWidget::indexFromItem is
// protected).
QModelIndex indexOfItem(const QTreeWidget* tree, QTreeWidgetItem* item, int column) {
  QTreeWidgetItem* parent = item->parent();
  if (parent == nullptr) {
    return tree->model()->index(tree->indexOfTopLevelItem(item), column);
  }
  return tree->model()->index(parent->indexOfChild(item), column, indexOfItem(tree, parent, 0));
}

// A square of a colour (none: the default look's grey).
QIcon swatch(const QColor& color) {
  QPixmap pixmap(18, 18);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  const QColor fill = color.isValid() ? color : QColor(204, 204, 204);
  painter.setBrush(fill);
  // An edge from the role table, also on the dark panel (mitcad#1083).
  painter.setPen(QPen(swatchEdgeColor(), 1));
  painter.drawRoundedRect(QRectF(1.5, 1.5, 15.0, 15.0), 3.0, 3.0);
  return QIcon(pixmap);
}

// How deep a row is in the list: 0 for a place's heading.
int depthOf(const QTreeWidgetItem* row) {
  int depth = 0;
  for (const QTreeWidgetItem* parent = row->parent(); parent != nullptr; parent = parent->parent()) {
    ++depth;
  }
  return depth;
}

const QStringList& groupOrder() {
  // The classes' groups (mitcad#655, mitcad#1102): FDM plastics hold the
  // filaments.
  return materialGroups();
}

} // namespace

MaterialsDialog::MaterialsDialog(DocumentHost& host, QWidget* parent) : QDialog(parent), m_hosts(host) {
  setWindowTitle(tr("Materials"));
  setObjectName(QStringLiteral("materialsManager"));
  setWindowIcon(themeIcon(QStringLiteral("material")));
  resize(960, 640);
  m_hosts.choose();
  auto* layout = new QVBoxLayout(this);
  // Import from Slicer… (mitcad#829, mitcad#1135): a slicer's filaments
  // into the place chosen in the box before it, the default as New's.
  m_import = new SlicerImportBar(m_hosts.current(), QStringLiteral("Materials"));
  m_import->target = [this] { return defaultPlace(m_hosts.onDesign()); };
  layout->addWidget(m_import);
  m_panel = new MaterialsPanel(m_hosts.current(), QStringLiteral("Materials"));
  layout->addWidget(m_panel, 1);
  m_panel->editAppearances = [this] {
    if (editAppearances) {
      editAppearances();
    }
  };
  // The first material the import brought, selected where it went.
  connect(m_import, &SlicerImportBar::imported, this, [this](const SlicerImportResult& result) {
    if (!m_hosts.onDesign()) {
      m_changed = true;
    }
    QString id;
    for (const QJsonValue& value : result.answer.value(QStringLiteral("materials")).toArray()) {
      if (id.isEmpty()) {
        id = value.isObject() ? text(value.toObject(), "id") : value.toString();
      }
    }
    for (const QJsonValue& value : result.answer.value(QStringLiteral("items")).toArray()) {
      if (id.isEmpty() && text(value.toObject(), "kind") == QStringLiteral("material")) {
        id = text(value.toObject(), "id");
      }
    }
    if (id.isEmpty()) {
      m_panel->refresh();
      return;
    }
    m_panel->showItem(result.into + QLatin1Char('/') + id);
  });
  connect(m_panel, &MaterialsPanel::changed, this, [this] {
    if (!m_hosts.onDesign()) {
      m_changed = true;
    }
  });
  auto* close = new QPushButton(tr("Close"));
  close->setObjectName(QStringLiteral("printManagerClose"));
  m_panel->addButton(close, QStringLiteral("close"));
  mac::styleDialog(this);
  connect(close, &QPushButton::clicked, this, &QDialog::reject);
}

MaterialsDialog::~MaterialsDialog() {
  // The panel and the import before the models they work through.
  delete m_panel;
  delete m_import;
}

void MaterialsDialog::open(const QString& key) {
  if (m_hosts.choose()) {
    m_panel->setHost(m_hosts.current(), false);
    m_import->setHost(m_hosts.current());
  }
  m_import->setPlaces(m_panel->places(), defaultPlace(m_hosts.onDesign()));
  if (m_hosts.onDesign()) {
    if (key.isEmpty()) {
      m_panel->refresh();
    } else {
      m_panel->showItem(key);
    }
    return;
  }
  // Without a design (mitcad#1135): the libraries are read off the UI
  // thread (mitcad#1018), the list fills when they came.
  const int reading = ++m_reads;
  readAhead(this, managerRequests(QStringLiteral("material"), m_hosts.library().projectFolder()),
            [this, reading, key](const QueryAnswers& answers, qint64 readMs) {
              if (reading != m_reads || m_hosts.onDesign()) {
                return;
              }
              m_hosts.library().setAnswers(answers);
              QElapsedTimer timer;
              timer.start();
              if (key.isEmpty()) {
                m_panel->refresh();
              } else {
                m_panel->showItem(key);
              }
              qDebug().noquote() << QStringLiteral("Materials loaded: read %1 ms, list %2 ms")
                                        .arg(readMs)
                                        .arg(timer.elapsed());
            });
}

void MaterialsDialog::refresh() {
  // A design opened or changed meanwhile: on its model now.
  if (m_hosts.choose()) {
    m_panel->setHost(m_hosts.current(), false);
    m_import->setHost(m_hosts.current());
  }
  m_import->setPlaces(m_panel->places(), defaultPlace(m_hosts.onDesign()));
  ++m_reads; // read now: a reading under way shows nothing older
  m_panel->refresh();
}

void MaterialsDialog::showItem(const QString& key) { m_panel->showItem(key); }

void MaterialsDialog::hideEvent(QHideEvent* event) {
  QDialog::hideEvent(event);
  qDebug().noquote() << QStringLiteral("Materials closed%1").arg(m_changed ? QStringLiteral(" after changes")
                                                                          : QString());
  if (m_changed) {
    m_changed = false;
    emit libraryChanged();
  }
}

MaterialsPanel::MaterialsPanel(PrintItemsHost& host, const QString& logName, QWidget* parent)
    : QWidget(parent), m_host(&host), m_log(logName) {
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  auto* splitter = new QSplitter(Qt::Horizontal);
  splitter->setObjectName(QStringLiteral("printManagerSplit"));

  // The search across the groups, then the list grouped by place and in
  // each place by class (mitcad#655, mitcad#1135).
  auto* side = new QWidget;
  auto* sideBox = new QVBoxLayout(side);
  sideBox->setContentsMargins(0, 0, 0, 0);
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("printManagerSearch"));
  m_search->setPlaceholderText(tr("Search materials"));
  m_search->setAccessibleName(tr("Search"));
  m_search->setClearButtonEnabled(true);
  sideBox->addWidget(m_search);
  m_list = new QTreeWidget;
  m_list->setObjectName(QStringLiteral("materialsList"));
  m_list->setAccessibleName(tr("Materials by place and class"));
  m_list->setColumnCount(2);
  m_list->setHeaderHidden(true);
  m_list->setIconSize(QSize(18, 18));
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  // Shallow (mitcad#1083): the groups' headings flush left, their
  // materials one small step in, always shown. The names take the width
  // and are elided only when the list is narrowed (the full name in the
  // tooltip); the mark of one the design uses at the right; no horizontal
  // scroll bar.
  m_list->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_list->header()->setStretchLastSection(false);
  m_list->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_list->header()->setSectionResizeMode(1, QHeaderView::Fixed);
  m_list->header()->resizeSection(1, m_list->fontMetrics().horizontalAdvance(QStringLiteral("●")) + 12);
  m_list->setTextElideMode(Qt::ElideRight);
  styleItemList(m_list);
  m_list->installEventFilter(this);
  mac::styleListTree(m_list);
  m_list->setRootIsDecorated(false);
  m_list->setItemsExpandable(false);
  m_list->setIndentation(kListIndent);
  sideBox->addWidget(m_list, 1);
  splitter->addWidget(side);

  // The editor of the selected one: the header the managers share (its
  // name to edit, Built-in, Copy to), the line that says where it is
  // edited (mitcad#1135), then the tabs: General (Class, Density,
  // Appearances) and the print settings' (mitcad#655).
  m_editor = new QWidget;
  auto* editorBox = new QVBoxLayout(m_editor);
  editorBox->setContentsMargins(8, 0, 0, 0);
  editorBox->addLayout(itemHeader(&m_name, &m_usedState, &m_builtIn, &m_headerDuplicate, &m_copyTo, &m_editing));
  m_name->setObjectName(QStringLiteral("materialName"));
  editorBox->addWidget(m_editing);
  // Nothing selected: a quiet line, not a blank side.
  m_emptyState = new QLabel(tr("No material selected"));
  m_emptyState->setAlignment(Qt::AlignCenter);
  setRoleStyleSheet(m_emptyState, ColorRole::TextSecondary);
  m_emptyState->hide();
  editorBox->addWidget(m_emptyState, 1);

  auto* general = new QWidget;
  auto* form = new QFormLayout(general);
  form->setContentsMargins(4, 8, 4, 4);
  // The class is the group the list shows it in (mitcad#655).
  m_class = new QComboBox;
  for (const QString& materialClass : materialClasses()) {
    m_class->addItem(materialClassTitle(materialClass), materialClass);
  }
  m_class->setFixedWidth(160);
  form->addRow(tr("Class"), m_class);
  auto* densityRow = new QHBoxLayout;
  m_density = new QLineEdit;
  // The same widths as the settings form's fields (mitcad#655).
  m_density->setFixedWidth(150);
  densityRow->addWidget(m_density);
  auto* densityUnit = new QLabel(QStringLiteral("g/cm³"));
  densityUnit->setFixedWidth(80);
  densityRow->addWidget(densityUnit);
  densityRow->addStretch(1);
  form->addRow(tr("Density"), densityRow);
  // A refused density under its field (mitcad#655).
  m_densityError = new QLabel;
  m_densityError->setWordWrap(true);
  setErrorStyleSheet(m_densityError);
  m_densityError->hide();
  form->addRow(QString(), m_densityError);
  // Its appearances (mitcad#1055, mitcad#1083): one list, each row its
  // swatch and name, the default marked "Default" in its row; Add…,
  // Remove and Make Default beside it, the last two also in a row's
  // context menu. The list is as tall as its rows.
  auto* appearancesRow = new QHBoxLayout;
  appearancesRow->setSpacing(8);
  m_appearanceList = new QTreeWidget;
  m_appearanceList->setObjectName(QStringLiteral("materialAppearances"));
  m_appearanceList->setProperty(NativeModelKeysProperty,
                               QVariantList{int(Qt::Key_Delete), int(Qt::Key_Backspace)});
  m_appearanceList->setAccessibleName(tr("Appearances"));
  m_appearanceList->setColumnCount(2);
  m_appearanceList->setHeaderHidden(true);
  m_appearanceList->setRootIsDecorated(false);
  m_appearanceList->setItemsExpandable(false);
  m_appearanceList->setUniformRowHeights(true);
  m_appearanceList->setIconSize(QSize(kSwatchSize, kSwatchSize));
  m_appearanceList->setSelectionMode(QAbstractItemView::SingleSelection);
  m_appearanceList->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_appearanceList->setContextMenuPolicy(Qt::CustomContextMenu);
  m_appearanceList->header()->setStretchLastSection(false);
  m_appearanceList->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_appearanceList->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
  m_appearanceList->installEventFilter(this);
  // The selected row at full strength also after a button took the
  // keyboard, as the list of materials.
  styleItemList(m_appearanceList);
  mac::styleListTree(m_appearanceList);
  appearancesRow->addWidget(m_appearanceList, 0, Qt::AlignTop);
  auto* appearanceButtons = new QVBoxLayout;
  appearanceButtons->setSpacing(kAppearanceButtonSpacing);
  appearanceButtons->setContentsMargins(0, 0, 0, 0);
  m_addAppearance = new QPushButton(tr("Add…"));
  m_addAppearance->setObjectName(QStringLiteral("materialAppearanceAdd"));
  m_addAppearance->setToolTip(tr("Add an appearance from the appearance library"));
  m_removeAppearance = new QPushButton(tr("Remove"));
  m_removeAppearance->setObjectName(QStringLiteral("materialAppearanceRemove"));
  m_removeAppearance->setToolTip(tr("Remove the selected appearance from the list"));
  m_defaultAppearance = new QPushButton(tr("Make Default"));
  m_defaultAppearance->setObjectName(QStringLiteral("materialAppearanceDefault"));
  m_defaultAppearance->setToolTip(tr("Make the selected appearance the material's default"));
  appearanceButtons->addWidget(m_addAppearance);
  appearanceButtons->addWidget(m_removeAppearance);
  appearanceButtons->addWidget(m_defaultAppearance);
  appearanceButtons->addStretch(1);
  appearancesRow->addLayout(appearanceButtons);
  appearancesRow->addStretch(1);
  form->addRow(tr("Appearances"), appearancesRow);
  m_addPopover = new AppearancePopover(m_log + QStringLiteral(" add appearance"), this);
  m_form = new SettingsForm;
  m_form->setGeneralPage(general, tr("General"));
  m_form->setGeneralNames({tr("Class"), tr("Density"), tr("Appearances")});
  // A filament's values per printer and nozzle (mitcad#1054): the last
  // tab, with the print settings.
  m_variants = new MaterialVariantsPage(m_log);
  m_form->setEndPage(m_variants, tr("Variants"));
  m_variants->printers = [this] {
    QJsonArray rows;
    for (const LibraryItem& item : queryLibraryItems(m_host->model(), QStringLiteral("printer"), project())) {
      rows.append(item.row);
    }
    return rows;
  };
  // The slicer presets the material builds on, one per slicer
  // (mitcad#1062): the export writes them as the filament's preset and
  // what it inherits, so that the slicer starts from its own preset.
  m_form->setSlicerKeys({QStringLiteral("filament_settings_id")});
  m_form->slicerChoices = [this](const QString& key, const QString& slicer) { return presetChoices(key, slicer); };
  editorBox->addWidget(m_form, 1);
  editorBox->addStretch(0);
  splitter->addWidget(m_editor);
  // A narrow list, the editor wide; the list as wide as its names need
  // (fitListWidth) until the user moves the divider.
  splitter->setStretchFactor(0, 0);
  splitter->setStretchFactor(1, 1);
  splitter->setSizes({240, 720});
  m_splitter = splitter;
  connect(splitter, &QSplitter::splitterMoved, this, [this] {
    m_listWidthByUser = true;
    scheduleLogPlaces();
  });
  layout->addWidget(splitter, 1);

  m_message = new QLabel;
  m_message->setWordWrap(true);
  layout->addWidget(m_message);
  m_buttons = new QHBoxLayout;
  m_new = new QPushButton(tr("New ▾"));
  m_new->setObjectName(QStringLiteral("printManagerNew"));
  m_duplicate = new QPushButton(tr("Duplicate"));
  m_duplicate->setObjectName(QStringLiteral("printManagerDuplicate"));
  m_delete = new QPushButton(tr("Delete"));
  m_delete->setObjectName(QStringLiteral("printManagerDelete"));
  m_buttons->addWidget(m_new);
  m_buttons->addWidget(m_duplicate);
  m_buttons->addWidget(m_delete);
  m_buttons->addStretch(1);
  layout->addLayout(m_buttons);
  // Enter in a field saves it: no button takes Enter.
  for (QPushButton* button : {m_headerDuplicate, m_copyTo, m_new, m_duplicate, m_delete, m_addAppearance,
                              m_removeAppearance, m_defaultAppearance}) {
    mac::auxiliaryButton(button);
  }

  connect(m_search, &QLineEdit::textChanged, this, [this](const QString& wanted) {
    rebuildList();
    int shown = 0;
    for (const LibraryItem& item : std::as_const(m_items)) {
      shown += matchesSearch(item, wanted) && m_places.contains(item.place) ? 1 : 0;
    }
    qDebug().noquote() << QStringLiteral("%1 search: %2, %3 shown").arg(m_log, wanted).arg(shown);
    logRows();
  });

  connect(m_class, &QComboBox::activated, this, [this](int index) {
    const LibraryItem* item = current();
    const QString chosen = m_class->itemData(index).toString();
    if (item != nullptr && !item->readOnly && chosen != item->materialClass) {
      edit({{QStringLiteral("class"), chosen}}, QStringLiteral("class = %1").arg(chosen));
    }
  });
  connect(m_list, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* item) {
    if (m_building || item == nullptr || item->data(0, kKeyRole).toString().isEmpty()) {
      return;
    }
    m_currentKey = item->data(0, kKeyRole).toString();
    const LibraryItem* chosen = current();
    setMessage(QString());
    showCurrent();
    qDebug().noquote() << QStringLiteral("Material selected: %1")
                              .arg(chosen != nullptr
                                       ? QStringLiteral("%1 (%2)").arg(chosen->name, chosen->place)
                                       : QStringLiteral("none"));
    scheduleLogPlaces();
  });
  connect(m_name, &QLineEdit::editingFinished, this, [this] {
    const LibraryItem* item = current();
    const QString name = m_name->text().trimmed();
    if (item != nullptr && !item->readOnly && !name.isEmpty() && name != item->name) {
      edit({{QStringLiteral("name"), name}}, QStringLiteral("name = %1").arg(name));
    }
  });
  connect(m_density, &QLineEdit::editingFinished, this, [this] {
    const LibraryItem* item = current();
    if (item == nullptr || item->readOnly) {
      return;
    }
    const std::optional<double> density = parsePlainNumber(m_density->text());
    const double now = itemFields(*item, m_designMaterials, QJsonObject()).value(QStringLiteral("density")).toDouble();
    if (!density || *density <= 0.0) {
      m_densityError->setText(tr("Use a value above 0 g/cm³."));
      m_densityError->show();
      m_density->setText(printNumberText(now));
      return;
    }
    m_densityError->hide();
    if (std::abs(*density - now) > 1e-12) {
      edit({{QStringLiteral("density"), *density}}, QStringLiteral("density = %1").arg(printNumberText(*density)));
    }
  });
  connect(m_addPopover, &AppearancePopover::chosen, this, &MaterialsPanel::addAppearance);
  connect(m_addAppearance, &QPushButton::clicked, this, [this] {
    const LibraryItem* item = current();
    if (item == nullptr || item->readOnly) {
      return;
    }
    // The appearances of its class first (mitcad#1102).
    const QStringList offered = offeredAppearances(itemFields(*item, m_designMaterials, QJsonObject()));
    m_addPopover->open(m_addAppearance, QString(), offered, item->materialClass);
  });
  connect(m_removeAppearance, &QPushButton::clicked, this, &MaterialsPanel::removeAppearance);
  connect(m_defaultAppearance, &QPushButton::clicked, this, &MaterialsPanel::setDefaultAppearance);
  // The buttons and the log follow the selected row. Both signals: a click
  // makes the row current before it selects it, the keyboard selects
  // before it makes it current, and a deselection (Ctrl+click, a click
  // below the rows) changes no current row.
  const auto selectionChanged = [this] {
    const LibraryItem* item = current();
    if (m_building || item == nullptr) {
      return;
    }
    const QString selected = selectedAppearance();
    if (selected != m_loggedSelection) {
      m_loggedSelection = selected;
      if (!selected.isEmpty()) {
        qDebug().noquote() << QStringLiteral("%1 appearance selected: %2").arg(m_log, appearanceName(selected));
      }
    }
    showAppearances(itemFields(*item, m_designMaterials, QJsonObject()), !item->readOnly);
  };
  connect(m_appearanceList, &QTreeWidget::currentItemChanged, this, selectionChanged);
  connect(m_appearanceList, &QTreeWidget::itemSelectionChanged, this, selectionChanged);
  connect(m_appearanceList, &QWidget::customContextMenuRequested, this, &MaterialsPanel::appearanceMenu);
  connect(m_form, &SettingsForm::valueChanged, this, [this](const QString& key, const QJsonValue& value) {
    const PrintKey* printKey = m_keys.find(key);
    const QString shown = value.isNull() ? QStringLiteral("cleared")
                                         : (printKey != nullptr ? printValueText(*printKey, value)
                                                                : value.toVariant().toString());
    edit({{QStringLiteral("filament"), QJsonObject{{key, value}}}}, QStringLiteral("%1 = %2").arg(key, shown), key);
  });
  connect(m_form, &SettingsForm::layoutChanged, this, &MaterialsPanel::scheduleLogPlaces);
  connect(m_variants, &MaterialVariantsPage::changeRequested, this,
          [this](const QJsonObject& variant, const QString& what) {
            edit({{QStringLiteral("variant"), variant}}, what);
          });
  connect(m_variants, &MaterialVariantsPage::layoutChanged, this, &MaterialsPanel::scheduleLogPlaces);
  connect(m_copyTo, &QPushButton::clicked, this, &MaterialsPanel::copyTo);
  connect(m_new, &QPushButton::clicked, this, &MaterialsPanel::newMaterial);
  connect(m_duplicate, &QPushButton::clicked, this, [this] { duplicate(m_duplicate); });
  connect(m_headerDuplicate, &QPushButton::clicked, this, [this] { duplicate(m_headerDuplicate); });
  connect(m_delete, &QPushButton::clicked, this, &MaterialsPanel::remove);
}

MaterialsPanel::~MaterialsPanel() = default;

void MaterialsPanel::setHost(PrintItemsHost& host, bool read) {
  m_host = &host;
  m_currentKey.clear();
  m_loggedRows.clear();
  m_loggedEditing.clear();
  m_keys = PrintKeyTable(); // asked of the new model
  if (read) {
    refresh();
  } else {
    m_items.clear();
    m_places.clear();
    rebuildList();
    showCurrent();
  }
}

QString MaterialsPanel::project() const { return m_host->projectFolder(); }

QStringList MaterialsPanel::places() const { return writablePlaces(hasDesign(), !project().isEmpty()); }

void MaterialsPanel::addButton(QPushButton* button, const QString& name) {
  mac::auxiliaryButton(button);
  m_buttons->addWidget(button);
  m_windowButtons.append({name, button});
}

const LibraryItem* MaterialsPanel::current() const {
  if (m_currentKey.isEmpty()) {
    return nullptr;
  }
  return findItem(m_items, m_currentKey);
}

void MaterialsPanel::showItem(const QString& key) {
  m_currentKey = key;
  refresh();
  // A picker's key of a library item: the design's copy of it, which the
  // design uses (mitcad#1135).
  const LibraryItem* found = current();
  if (found != nullptr && !found->copy.isEmpty() && hasDesign()) {
    m_currentKey = QStringLiteral("design/") + found->copy;
    rebuildList();
    showCurrent();
  }
  if (const LibraryItem* item = current()) {
    if (m_list->currentItem() != nullptr) {
      m_list->scrollToItem(m_list->currentItem());
    }
    qDebug().noquote() << QStringLiteral("Material selected: %1 (%2)").arg(item->name, item->place);
  }
}

void MaterialsPanel::refresh() {
  QString error;
  m_items.clear();
  QStringList order;
  for (const LibraryItem& item :
       queryPlaceItems(m_host->model(), QStringLiteral("material"), project(), &order, &error)) {
    // Without a design its model's empty design is no place (mitcad#1135).
    if (hasDesign() || item.place != QStringLiteral("design")) {
      m_items.append(item);
    }
  }
  if (order.isEmpty()) {
    order = writablePlaces(true, !project().isEmpty());
    order << QStringLiteral("mitcad");
  }
  if (!hasDesign()) {
    order.removeAll(QStringLiteral("design"));
  }
  m_places = order;
  try {
    m_designMaterials = m_host->model().queryArray(QStringLiteral("materials"));
    m_appearances = appearancesOf(m_host->model().queryArray(QStringLiteral("appearances")));
    m_bodies = m_host->model().queryArray(QStringLiteral("bodies"));
    // The design's slicer: whose percentages the fields offer (mitcad#655).
    m_dialect = designDialectOf(m_host->model());
    if (m_keys.isEmpty()) {
      m_keys = PrintKeyTable(m_host->model().queryObject({{QStringLiteral("query"), QStringLiteral("print_keys")}}));
    }
  } catch (const std::exception& e) {
    error = QString::fromUtf8(e.what());
  }
  if (!error.isEmpty()) {
    setMessage(error, true);
  }
  if (current() == nullptr) {
    m_currentKey.clear();
  }
  // The appearances, the design's and the library's, with Edit
  // Appearances… last (none without a design).
  m_addPopover->setAppearances(m_appearances);
  if (editAppearances && hasDesign()) {
    m_addPopover->editAppearances = [this] {
      if (editAppearances) {
        editAppearances();
      }
    };
  } else {
    m_addPopover->editAppearances = nullptr;
  }
  rebuildList();
  showCurrent();
  logRows();
  scheduleLogPlaces();
  if (!m_pendingFocus.isEmpty() && m_pendingFocus == m_currentKey) {
    m_pendingFocus.clear();
    // A new material of another class: its density to be set, not a silent
    // 1 g/cm³ (mitcad#655); else its name.
    QLineEdit* field = m_pendingDensity ? m_density : m_name;
    m_pendingDensity = false;
    if (field == m_density) {
      m_form->showGeneral();
    }
    field->setFocus(Qt::OtherFocusReason);
    field->selectAll();
    requestNativeFocus(field);
  }
}

void MaterialsPanel::rebuildList() {
  m_building = true;
  m_list->clear();
  QTreeWidgetItem* selected = nullptr;
  // What the design's bodies use (mitcad#655): a material its bodies or
  // components name, and the default of a body without one.
  QStringList used;
  bool bodyWithout = false;
  for (const QJsonValue& value : std::as_const(m_bodies)) {
    bodyWithout = bodyWithout || text(value.toObject(), "material").isEmpty();
  }
  QString rootDefault = QStringLiteral("steel");
  for (const QJsonValue& value : std::as_const(m_designMaterials)) {
    const QJsonObject material = value.toObject();
    if (!material.value(QStringLiteral("bodies")).toArray().isEmpty() ||
        !material.value(QStringLiteral("components")).toArray().isEmpty()) {
      used << text(material, "id");
    }
    if (material.value(QStringLiteral("components")).toArray().contains(QStringLiteral("C0"))) {
      rootDefault = text(material, "id");
    }
  }
  if (bodyWithout) {
    used << rootDefault;
  }
  m_usedIds = used;
  const auto heading = [this](const QString& title, const QString& group) {
    auto* made = new QTreeWidgetItem(m_list, {title});
    made->setFlags(Qt::ItemIsEnabled);
    made->setFirstColumnSpanned(true);
    made->setData(1, kKeyRole, group);
    QFont bold = made->font(0);
    bold.setBold(true);
    made->setFont(0, bold);
    return made;
  };
  const auto addRow = [&](QTreeWidgetItem* under, const LibraryItem& item) {
    auto* row = new QTreeWidgetItem(under, {item.name, QString()});
    row->setIcon(0, swatch(item.displayColor));
    row->setData(0, kKeyRole, item.key());
    row->setData(0, NativeControlIdentifierRole, QStringLiteral("material.") + item.key());
    const bool inUse = !item.designId().isEmpty() && used.contains(item.designId());
    markInDesign(row, inUse, tr("Used in this design"));
    // The full name, also when the list is narrowed and elides it.
    row->setToolTip(0, inUse ? tr("%1, used in this design").arg(item.name) : item.name);
    if (item.key() == m_currentKey) {
      selected = row;
    }
  };
  // By place (mitcad#1135), each a heading with its count, then in each
  // place by class in the query's order (the built-in ones first, then by
  // name, mitcad#655); the search across the places.
  const QString wanted = m_search->text();
  for (const QString& place : std::as_const(m_places)) {
    int count = 0;
    for (const LibraryItem& item : std::as_const(m_items)) {
      count += item.place == place && matchesSearch(item, wanted) ? 1 : 0;
    }
    QTreeWidgetItem* placeRow = heading(placeHeading(place, count), place);
    placeRow->setData(0, NativeControlIdentifierRole, QStringLiteral("printManager.group.") + place);
    for (const QString& group : groupOrder()) {
      QTreeWidgetItem* groupRow = nullptr;
      for (const LibraryItem& item : std::as_const(m_items)) {
        const QString itemGroup = groupOrder().contains(item.group) ? item.group : QStringLiteral("other");
        if (item.place != place || itemGroup != group || !matchesSearch(item, wanted)) {
          continue;
        }
        if (groupRow == nullptr) {
          groupRow = new QTreeWidgetItem(placeRow, {materialGroupTitle(group)});
          groupRow->setFlags(Qt::ItemIsEnabled);
          groupRow->setFirstColumnSpanned(true);
          groupRow->setData(1, kKeyRole, group);
          groupRow->setData(0, NativeControlIdentifierRole, QStringLiteral("printManager.class.") + place +
                                                             QLatin1Char('.') + group);
          groupRow->setForeground(0, themeColor(ColorRole::TextSecondary, m_list->palette()));
        }
        addRow(groupRow, item);
      }
    }
  }
  m_list->expandAll();
  m_list->setCurrentItem(selected);
  m_building = false;
  fitListWidth();
  scheduleLogPlaces();
}

int MaterialsPanel::listWidthNeeded() const {
  int widest = 0;
  for (QTreeWidgetItemIterator it(m_list); *it != nullptr; ++it) {
    QTreeWidgetItem* row = *it;
    const int indent = depthOf(row) * m_list->indentation();
    widest = std::max(widest, indent + cellHint(m_list, indexOfItem(m_list, row, 0)).width());
  }
  return widest + m_list->header()->sectionSize(1) + 2 * m_list->frameWidth() +
         m_list->style()->pixelMetric(QStyle::PM_ScrollBarExtent, nullptr, m_list);
}

void MaterialsPanel::fitListWidth() {
  // Until the user moves the divider: wide enough for every name, at most
  // under half the window, so that the editor keeps its room.
  if (m_listWidthByUser || !m_splitter->isVisible() || m_splitter->count() < 2) {
    return;
  }
  const int total = m_splitter->width() - m_splitter->handleWidth();
  // Never more than the editor leaves: the splitter would take it back.
  const int most = std::min(total * 45 / 100, total - m_editor->minimumSizeHint().width());
  const int wanted = std::clamp(listWidthNeeded(), 200, std::max(200, most));
  const QList<int> sizes = m_splitter->sizes();
  if (total > wanted && !sizes.isEmpty() && sizes.first() != wanted) {
    m_splitter->setSizes({wanted, total - wanted});
  }
}

QStringList MaterialsPanel::cutNames() const {
  // A name is cut when its cell asks for more than the list gives it: the
  // viewport less the mark's column and its indentation.
  QStringList cut;
  const int room = m_list->viewport()->width() - m_list->header()->sectionSize(1);
  for (QTreeWidgetItemIterator it(m_list); *it != nullptr; ++it) {
    QTreeWidgetItem* row = *it;
    if (row->data(0, kKeyRole).toString().isEmpty()) {
      continue;
    }
    const int indent = depthOf(row) * m_list->indentation();
    if (indent + cellHint(m_list, indexOfItem(m_list, row, 0)).width() > room) {
      cut << row->text(0);
    }
  }
  return cut;
}

void MaterialsPanel::selectKey(const QString& key) {
  for (QTreeWidgetItemIterator it(m_list); *it != nullptr; ++it) {
    if ((*it)->data(0, kKeyRole).toString() == key) {
      m_list->setCurrentItem(*it);
      m_list->scrollToItem(*it);
      return;
    }
  }
}

void MaterialsPanel::showCurrent() {
  const LibraryItem* item = current();
  m_duplicate->setEnabled(item != nullptr);
  m_delete->setEnabled(item != nullptr && !item->readOnly);
  // Never a blank side (mitcad#655): nothing selected a quiet line; an item
  // its editor.
  m_emptyState->setVisible(item == nullptr);
  m_name->setVisible(item != nullptr);
  m_form->setVisible(item != nullptr);
  m_editing->setVisible(item != nullptr);
  if (item == nullptr) {
    m_name->clear();
    m_name->setReadOnly(true);
    m_builtIn->hide();
    m_usedState->hide();
    m_copyTo->hide();
    m_editing->clear();
    m_density->clear();
    showAppearances(QJsonObject(), false);
    m_class->setCurrentIndex(-1);
    m_form->setValues({});
    return;
  }
  // The fields are made for the first material shown (mitcad#1018: not
  // while the window opens with none selected).
  m_form->setKeys(m_keys, QStringLiteral("filament"), m_dialect);
  // The material's density is shown once, in General; its colour is its
  // appearance (mitcad#1055): no colour field.
  m_form->setHiddenKeys({QStringLiteral("filament_density"), QStringLiteral("filament_colour")});
  // Its own fields where it lives (mitcad#1135).
  const QJsonObject fields = itemFields(*item, m_designMaterials, QJsonObject());
  const bool editable = !item->readOnly;
  if (!m_name->hasFocus() || !m_name->isModified()) {
    m_name->setText(item->name);
    m_name->setModified(false);
  }
  m_name->setReadOnly(!editable);
  m_builtIn->setVisible(item->readOnly);
  m_usedState->setVisible(!item->designId().isEmpty() && m_usedIds.contains(item->designId()));
  m_copyTo->setVisible(!copyTargets(*item, hasDesign(), !project().isEmpty()).isEmpty());
  // Where it is edited and whom an edit reaches, one line (mitcad#1135).
  const QString line = editingLine(*item);
  m_editing->setText(line);
  if (isVisible() && item->key() + line != m_loggedEditing) {
    m_loggedEditing = item->key() + line;
    qDebug().noquote() << QStringLiteral("%1 editing: %2").arg(m_log, line);
  }
  const double density = fields.value(QStringLiteral("density")).toDouble(1.0);
  m_density->setText(printNumberText(density));
  m_density->setReadOnly(!editable);
  const QString appearanceId = text(fields, "appearance");
  showAppearances(fields, editable);
  {
    const QSignalBlocker block(m_class);
    m_class->setCurrentIndex(m_class->findData(item->materialClass));
  }
  // A material with print settings is an FDM plastic (mitcad#1102): its
  // class shows so and cannot change.
  m_class->setEnabled(editable && !item->classFixed);
  m_class->setToolTip(item->classFixed ? tr("A material with print settings is an FDM plastic")
                                       : tr("The group the material is listed in"));
  const QString classShown = QStringLiteral("%1%2").arg(
      materialClassTitle(item->materialClass),
      item->classFixed ? QStringLiteral(", fixed by its print settings") : QString());
  if (item->key() + QLatin1Char('|') + classShown != m_loggedClass) {
    m_loggedClass = item->key() + QLatin1Char('|') + classShown;
    qDebug().noquote() << QStringLiteral("%1 class: %2").arg(m_log, classShown);
  }
  // Print settings only for an FDM plastic (mitcad#655, mitcad#1102):
  // metals, other plastics, resins and others have none to give a slicer;
  // an FDM plastic shows them, empty when unset.
  m_form->setPrintShown(item->materialClass == QStringLiteral("fdm_plastic"));
  m_form->setReadOnly(!editable);
  const QJsonObject filament = fields.value(QStringLiteral("filament")).toObject();
  m_form->setValues(filament);
  // Its variants per printer and nozzle (mitcad#1054).
  m_variants->setMaterial(fields, m_keys, m_dialect, !editable);
  // Unset purge volumes are those of the colour it prints with (mitcad#705):
  // its default appearance's (mitcad#1055), as the model works them out.
  QJsonObject placeholders;
  QHash<QString, QString> words;
  QString colour;
  if (const Appearance* look = findAppearance(m_appearances, appearanceId)) {
    colour = look->displayColor.name().toUpper();
  } else if (!appearanceId.isEmpty() && item->displayColor.isValid()) {
    colour = item->displayColor.name().toUpper();
  }
  try {
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("purge_defaults")}};
    if (!colour.isEmpty()) {
      query.insert(QStringLiteral("color"), colour);
    }
    const QJsonObject defaults = m_host->model().queryObject(query);
    for (const QString& key : {QStringLiteral("purge_into"), QStringLiteral("purge_out")}) {
      placeholders.insert(key, defaults.value(key));
    }
  } catch (const std::exception&) {
    // Without them: Slicer default.
  }
  m_form->setPlaceholderValues(placeholders, words);
  // Another design changed it: asked once, when the design uses it or it
  // is edited (mitcad#655), not on a mere selection.
  if (item->changed && !changeAsked(*item) && usedByDesign(*item)) {
    TestSync::singleShot(0, this, [this] { askChanges(); });
  }
}

QStringList MaterialsPanel::presetChoices(const QString& key, const QString& slicer) {
  QStringList names;
  // The filament presets of the slicer installed here (`slicer_presets`:
  // the Bambu-family slicers' own and the user's).
  for (const QJsonValue& entry : installedSlicers(*m_host, defaultPlace(hasDesign()))) {
    const QJsonObject found = entry.toObject();
    if (found.value(QStringLiteral("dialect")).toString() != slicer) {
      continue;
    }
    for (const QJsonValue& preset : found.value(QStringLiteral("presets")).toArray()) {
      const QJsonObject item = preset.toObject();
      const QString name = item.value(QStringLiteral("name")).toString();
      if (item.value(QStringLiteral("kind")).toString() == QStringLiteral("material") && !name.isEmpty() &&
          !names.contains(name)) {
        names << name;
      }
    }
  }
  // The names the design's materials give that slicer.
  for (const QJsonValue& material : std::as_const(m_designMaterials)) {
    const QString name = slicerPresetOf(
        material.toObject().value(QStringLiteral("filament")).toObject().value(key).toObject().value(slicer), slicer);
    if (!name.isEmpty() && !names.contains(name)) {
      names << name;
    }
  }
  qDebug().noquote() << QStringLiteral("%1 presets for %2: %3").arg(m_log, slicer).arg(names.size());
  return names;
}

bool MaterialsPanel::canChange() {
  if (m_host->canChangePrintSettings()) {
    return true;
  }
  setMessage(m_host->changeBlockedReason(), true);
  return false;
}

bool MaterialsPanel::run(const QJsonObject& command, QJsonObject* result) {
  if (m_host->runModelCommand(command, result)) {
    setMessage(QString());
    m_host->commandInputsChanged();
    emit changed();
    return true;
  }
  if (!m_host->lastCancelled()) {
    const LibraryItem* item = current();
    setMessage(plainModelError(m_host->lastError(), m_keys, command, item != nullptr ? item->name : QString()),
               true); // plain words (mitcad#655)
  }
  qDebug().noquote() << QStringLiteral("%1: %2 refused: %3").arg(m_log)
                            .arg(command.value(QStringLiteral("cmd")).toString(), m_host->lastError());
  return false;
}

void MaterialsPanel::setMessage(const QString& message, bool error) {
  m_message->setText(message);
  if (error) {
    setErrorStyleSheet(m_message);
  } else {
    m_message->setStyleSheet(QString());
  }
}

void MaterialsPanel::edit(const QJsonObject& change, const QString& what, const QString& field) {
  const LibraryItem* item = current();
  if (item == nullptr || item->readOnly) {
    return;
  }
  const LibraryItem target = *item;
  // Not while the field that changed still handles its signal: the model
  // command shows the window again.
  TestSync::singleShot(0, this, [this, target, change, what, field] {
    if (!canChange()) {
      refresh();
      return;
    }
    // Edited while another design's change waits: asked first
    // (mitcad#655).
    if (target.changed && !changeAsked(target)) {
      run(askChangedElsewhere(this, target, m_log, project()));
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_material")}};
    nameItem(command, target, project());
    for (auto it = change.begin(); it != change.end(); ++it) {
      command.insert(it.key(), it.value());
    }
    const bool done = run(command);
    if (done) {
      qDebug().noquote() << QStringLiteral("Material %1 (%2) %3").arg(target.name, target.place, what);
    }
    refresh();
    if (!done && !field.isEmpty() && !m_host->lastCancelled()) {
      // A refusal of a print setting under its field (mitcad#655).
      setMessage(QString());
      m_form->setError(field, m_keys.plainText(m_host->lastError()));
    }
  });
}

void MaterialsPanel::makeDefaultAppearance(const QString& id) {
  const LibraryItem* item = current();
  if (item == nullptr || item->readOnly || id.isEmpty() ||
      id == text(itemFields(*item, m_designMaterials, QJsonObject()), "appearance")) {
    return;
  }
  // One of its list becomes the default, the old one staying in the list
  // (the model's `appearance`).
  edit({{QStringLiteral("appearance"), id}}, QStringLiteral("appearance = %1").arg(id));
}

QStringList MaterialsPanel::offeredAppearances(const QJsonObject& fields) const {
  // The `materials` query lists them all, its default first; a library's
  // item its default and the others (mitcad#1055).
  QStringList list;
  const QString first = text(fields, "appearance");
  if (!first.isEmpty()) {
    list << first;
  }
  for (const QJsonValue& value : fields.value(QStringLiteral("appearances")).toArray()) {
    const QString id = value.toString();
    if (!id.isEmpty() && !list.contains(id)) {
      list << id;
    }
  }
  return list;
}

QString MaterialsPanel::appearanceName(const QString& id) const {
  if (const Appearance* look = findAppearance(m_appearances, id)) {
    return look->name;
  }
  // One a library's material carries from the design it was saved from
  // (mitcad#704), not known here: the model names a material's own colour
  // "<name> color" (`<id>_colour`, print_files::plastic_appearance), so
  // after the material of that id when it is listed; another by its id in
  // words, never the bare id.
  const auto inWords = [](QString words) {
    words.replace(QLatin1Char('_'), QLatin1Char(' '));
    if (!words.isEmpty()) {
      words[0] = words.at(0).toUpper();
    }
    return words;
  };
  const QString suffix = QStringLiteral("_colour");
  if (id.endsWith(suffix)) {
    const QString materialId = id.left(id.size() - suffix.size());
    for (const LibraryItem& item : std::as_const(m_items)) {
      QString itemId = item.id;
      itemId.replace(QLatin1Char('-'), QLatin1Char('_')).replace(QLatin1Char('/'), QLatin1Char('_'));
      if (itemId == materialId) {
        return tr("%1 color").arg(item.name);
      }
    }
    return tr("%1 color").arg(inWords(materialId));
  }
  return inWords(id);
}

QString MaterialsPanel::selectedAppearance() const {
  const QTreeWidgetItem* row = m_appearanceList->currentItem();
  return row != nullptr && row->isSelected() ? row->data(0, kKeyRole).toString() : QString();
}

void MaterialsPanel::showAppearances(const QJsonObject& fields, bool editable) {
  const QStringList offered = offeredAppearances(fields);
  const QString defaultId = text(fields, "appearance");
  const LibraryItem* item = current();
  // The rows again only when the list changed (a click keeps its
  // selection).
  QStringList shown;
  for (int row = 0; row < m_appearanceList->topLevelItemCount(); ++row) {
    const QString id = m_appearanceList->topLevelItem(row)->data(0, kKeyRole).toString();
    if (!id.isEmpty()) {
      shown << id;
    }
  }
  const bool same = shown == offered && m_appearanceList->property("defaultId").toString() == defaultId &&
                    m_appearanceList->property("material").toString() == (item != nullptr ? item->key() : QString());
  if (!same || m_appearanceList->topLevelItemCount() == 0) {
    // The selected one again, or the one Add… added (the new default after
    // Remove) just now.
    const QString keep = !m_selectAfterEdit.isEmpty() && offered.contains(m_selectAfterEdit) ? m_selectAfterEdit
                                                                                              : selectedAppearance();
    m_selectAfterEdit.clear();
    const QSignalBlocker block(m_appearanceList);
    m_appearanceList->clear();
    m_appearanceList->setProperty("defaultId", defaultId);
    m_appearanceList->setProperty("material", item != nullptr ? item->key() : QString());
    const QColor secondary = themeColor(ColorRole::TextSecondary, m_appearanceList->palette());
    for (const QString& id : offered) {
      const Appearance* look = findAppearance(m_appearances, id);
      const bool isDefault = id == defaultId;
      const QString name = appearanceName(id);
      // One not known here by the material's colour when it is the default.
      const QIcon icon = look != nullptr ? appearanceIcon(*look, kSwatchSize)
                                         : colorIcon(isDefault && item != nullptr ? item->displayColor : QColor(),
                                                     kSwatchSize);
      auto* row = new QTreeWidgetItem(m_appearanceList, {name, isDefault ? tr("Default") : QString()});
      row->setIcon(0, icon);
      row->setData(0, kKeyRole, id);
      row->setData(0, NativeControlIdentifierRole, QStringLiteral("material.appearance.") + id);
      row->setToolTip(0, isDefault ? tr("%1, the default").arg(name) : name);
      if (isDefault) {
        row->setForeground(1, secondary);
        row->setToolTip(1, tr("A body given this material gets this appearance"));
      }
      if (id == keep) {
        m_appearanceList->setCurrentItem(row);
      }
    }
    if (offered.isEmpty()) {
      // None: the state in a muted row of its own, which cannot be
      // selected, so that the empty box does not read as a text field.
      auto* none = new QTreeWidgetItem(m_appearanceList, {tr("None")});
      none->setFlags(Qt::ItemIsEnabled);
      none->setForeground(0, secondary);
    }
    fitAppearanceList();
    // Selected again without a signal: no new "selected" line for it.
    m_loggedSelection = selectedAppearance();
  }
  const QString selected = selectedAppearance();
  m_appearanceList->setEnabled(!offered.isEmpty() || editable);
  m_addAppearance->setEnabled(editable);
  m_removeAppearance->setEnabled(editable && !selected.isEmpty());
  // The default removed, the next row becomes the default: said where it
  // is decided.
  m_removeAppearance->setToolTip(selected == defaultId && offered.size() > 1
                                     ? tr("Remove the default; %1 becomes the default").arg(appearanceName(offered.at(1)))
                                     : tr("Remove the selected appearance from the list"));
  m_defaultAppearance->setEnabled(editable && !selected.isEmpty() && selected != defaultId);
  // Logged when it changes: "Materials appearances: Bright Green (default)
  // | Black".
  QStringList names;
  for (const QString& id : offered) {
    names << appearanceName(id) + (id == defaultId ? QStringLiteral(" (default)") : QString());
  }
  const QString logged = item == nullptr ? QString() : item->name + QLatin1Char(':') + names.join(QStringLiteral(" | "));
  if (logged != m_loggedAppearances) {
    m_loggedAppearances = logged;
    if (item != nullptr) {
      qDebug().noquote() << QStringLiteral("%1 appearances: %2")
                                .arg(m_log, names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(" | ")));
    }
  }
  scheduleLogPlaces();
}

void MaterialsPanel::fitAppearanceList() {
  // As tall as its rows, at least kAppearanceRowsMin and its buttons, up
  // to kAppearanceRowsShown, then it scrolls; as wide as its names need
  // within 260 to 320 pixels (a longer name elided, its tooltip in full),
  // so that the editor never takes width from the list of materials.
  const int rows = m_appearanceList->topLevelItemCount();
  int rowHeight = 0;
  int widest = 0;
  for (int row = 0; row < rows; ++row) {
    QTreeWidgetItem* item = m_appearanceList->topLevelItem(row);
    const QSize hint = cellHint(m_appearanceList, indexOfItem(m_appearanceList, item, 0));
    rowHeight = std::max(rowHeight, hint.height());
    widest = std::max(widest, hint.width());
  }
  if (rowHeight <= 0) {
    // No rows: the height one would have.
    rowHeight = std::max(kSwatchSize + 6, m_appearanceList->fontMetrics().height() + 8);
  }
  const int frame = 2 * m_appearanceList->frameWidth();
  const int shown = std::clamp(rows, kAppearanceRowsMin, kAppearanceRowsShown);
  // Never shorter than its buttons beside it: Add…, Remove and Make
  // Default end within the list's height.
  int buttons = 0;
  for (const QPushButton* button : {m_addAppearance, m_removeAppearance, m_defaultAppearance}) {
    buttons += button->sizeHint().height();
  }
  buttons += 2 * kAppearanceButtonSpacing;
  const int scrollBar =
      rows > kAppearanceRowsShown ? m_appearanceList->style()->pixelMetric(QStyle::PM_ScrollBarExtent, nullptr,
                                                                           m_appearanceList)
                                  : 0;
  const int defaultWord = m_appearanceList->fontMetrics().horizontalAdvance(tr("Default")) + 24;
  m_appearanceList->setFixedHeight(std::max(shown * rowHeight + frame, buttons));
  m_appearanceList->setFixedWidth(std::clamp(widest + defaultWord + frame + scrollBar, 260, 320));
}

void MaterialsPanel::appearanceMenu(const QPoint& at) {
  const LibraryItem* item = current();
  QTreeWidgetItem* row = m_appearanceList->itemAt(at);
  // Not on None, the empty list's row.
  if (item == nullptr || row == nullptr || row->data(0, kKeyRole).toString().isEmpty()) {
    return;
  }
  m_appearanceList->setCurrentItem(row);
  const QString id = row->data(0, kKeyRole).toString();
  const bool editable = !item->readOnly;
  const bool isDefault = id == text(itemFields(*item, m_designMaterials, QJsonObject()), "appearance");
  QMenu menu(this);
  QAction* makeDefault = menu.addAction(tr("Make Default"));
  makeDefault->setEnabled(editable && !isDefault);
  QAction* remove = menu.addAction(tr("Remove"));
  remove->setEnabled(editable);
  qDebug().noquote() << QStringLiteral("%1 appearance menu: %2: Make Default | Remove").arg(m_log, appearanceName(id));
  QAction* chosen = execMenu(&menu, m_appearanceList->viewport()->mapToGlobal(at));
  if (chosen == makeDefault) {
    makeDefaultAppearance(id);
  } else if (chosen == remove) {
    removeAppearance();
  }
}

void MaterialsPanel::addAppearance(const QString& id) {
  const LibraryItem* item = current();
  if (item == nullptr || item->readOnly || id.isEmpty()) {
    return;
  }
  QStringList list = offeredAppearances(itemFields(*item, m_designMaterials, QJsonObject()));
  if (list.contains(id)) {
    // Offered already: selected.
    for (int row = 0; row < m_appearanceList->topLevelItemCount(); ++row) {
      if (m_appearanceList->topLevelItem(row)->data(0, kKeyRole).toString() == id) {
        m_appearanceList->setCurrentItem(m_appearanceList->topLevelItem(row));
      }
    }
    return;
  }
  list << id;
  // Selected once it is listed: Make Default is one click away.
  m_selectAfterEdit = id;
  edit({{QStringLiteral("appearances"), QJsonArray::fromStringList(list)}},
       QStringLiteral("appearances = %1").arg(list.join(QStringLiteral(", "))));
}

void MaterialsPanel::removeAppearance() {
  const LibraryItem* item = current();
  const QString id = selectedAppearance();
  if (item == nullptr || item->readOnly || id.isEmpty()) {
    return;
  }
  const QJsonObject fields = itemFields(*item, m_designMaterials, QJsonObject());
  QStringList list = offeredAppearances(fields);
  list.removeAll(id);
  // The default removed: the next one is the default (the model's rule:
  // the list's first), selected so that its Default shows where it went.
  if (id == text(fields, "appearance") && !list.isEmpty()) {
    m_selectAfterEdit = list.first();
    qDebug().noquote() << QStringLiteral("%1 appearance default removed: %2 becomes the default")
                              .arg(m_log, appearanceName(list.first()));
  }
  edit({{QStringLiteral("appearances"), QJsonArray::fromStringList(list)}},
       QStringLiteral("appearances = %1").arg(list.isEmpty() ? QStringLiteral("none") : list.join(QStringLiteral(", "))));
}

void MaterialsPanel::setDefaultAppearance() { makeDefaultAppearance(selectedAppearance()); }

void MaterialsPanel::newMaterial() {
  if (!canChange()) {
    return;
  }
  // Its class (mitcad#655), then where it goes (mitcad#1135); selected.
  QMenu menu(this);
  QStringList titles;
  for (const QString& materialClass : materialClasses()) {
    menu.addAction(materialClassTitle(materialClass))->setData(materialClass);
    titles << materialClassTitle(materialClass);
  }
  qDebug().noquote() << QStringLiteral("%1 new menu: %2").arg(m_log).arg(titles.join(QStringLiteral(" | ")));
  QAction* chosen = execMenu(&menu, m_new->mapToGlobal(QPoint(0, m_new->height())));
  if (chosen == nullptr) {
    return;
  }
  // The model names it (Material 1). Its density is its class's typical
  // one (mitcad#655): steel's for a metal, PLA's for a plastic, the
  // built-in Resin - Standard's for a resin (mitcad#1102); another
  // material has none typical, so Density takes the keyboard to be set.
  const QString materialClass = chosen->data().toString();
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("create_material")},
                      {QStringLiteral("class"), materialClass}};
  if (materialClass == QStringLiteral("metal")) {
    command.insert(QStringLiteral("density"), 7.85);
  } else if (materialClass == QStringLiteral("plastic") || materialClass == QStringLiteral("fdm_plastic")) {
    command.insert(QStringLiteral("density"), 1.24);
  } else if (materialClass == QStringLiteral("resin")) {
    command.insert(QStringLiteral("density"), 1.15);
  }
  const QString place = askWhere(m_new, QStringLiteral("new"));
  if (place.isEmpty()) {
    return;
  }
  inPlace(command, place);
  QJsonObject result;
  const bool created = run(command, &result);
  if (created) {
    m_currentKey = place + QLatin1Char('/') + text(result, "id");
    m_pendingFocus = m_currentKey;
    m_pendingDensity = materialClass == QStringLiteral("other");
    qDebug().noquote() << QStringLiteral("Material created: %1 in %2").arg(text(result, "id"), place);
  }
  refresh();
}

QString MaterialsPanel::askWhere(QPushButton* anchor, const QString& what) {
  return choosePlaceAmong(anchor, QStringLiteral("%1 %2").arg(m_log, what), places(), defaultPlace(hasDesign()));
}

void MaterialsPanel::inPlace(QJsonObject& command, const QString& place) const {
  // In a library a file of its own; in the design nothing to add.
  if (place != QStringLiteral("design")) {
    command.insert(QStringLiteral("library"), place);
    if (!project().isEmpty()) {
      command.insert(QStringLiteral("project"), project());
    }
  }
}

void MaterialsPanel::duplicate(QPushButton* anchor) {
  const LibraryItem* item = current();
  if (item == nullptr || !canChange()) {
    return;
  }
  const LibraryItem source = *item;
  const QString place = askWhere(anchor, QStringLiteral("duplicate"));
  if (place.isEmpty()) {
    return;
  }
  // The user's own copy (mitcad#655) where they chose (mitcad#1135):
  // from the design's item or a built-in one by its id, else from the
  // library's item with the appearance it carries.
  const QJsonValue basedOn = source.library.isEmpty()
                                 ? QJsonValue(source.designId())
                                 : QJsonValue(QJsonObject{{QStringLiteral("library"), source.library},
                                                          {QStringLiteral("id"), source.id}});
  // The model names it after the source ("PLA (copy)").
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("create_material")},
                      {QStringLiteral("based_on"), basedOn}};
  if (!project().isEmpty() && basedOn.isObject()) {
    command.insert(QStringLiteral("project"), project());
  }
  inPlace(command, place);
  QJsonObject result;
  const bool created = run(command, &result);
  if (created) {
    m_currentKey = place + QLatin1Char('/') + text(result, "id");
    m_pendingFocus = m_currentKey;
    qDebug().noquote() << QStringLiteral("Material duplicated: %1 as %2 in %3").arg(source.name, text(result, "id"), place);
  }
  refresh();
}

void MaterialsPanel::remove() {
  const LibraryItem* item = current();
  if (item == nullptr || item->readOnly || !canChange()) {
    return;
  }
  const LibraryItem target = *item;
  // Always asked, naming the bodies that use it and what they then use
  // (mitcad#655): Default, the root's default material, else Steel; the
  // bodies without a material of their own when it is the root's default.
  QStringList users;
  for (const QJsonValue& uid : target.row.value(QStringLiteral("bodies")).toArray()) {
    for (const QJsonValue& value : std::as_const(m_bodies)) {
      if (text(value.toObject(), "uid") == uid.toString()) {
        users << text(value.toObject(), "name");
      }
    }
  }
  QString rootDefault;
  bool isRootDefault = false;
  for (const QJsonValue& value : std::as_const(m_designMaterials)) {
    const QJsonObject material = value.toObject();
    if (material.value(QStringLiteral("components")).toArray().contains(QStringLiteral("C0"))) {
      rootDefault = text(material, "name");
      isRootDefault = text(material, "id") == target.designId();
    }
  }
  QString steel = tr("Steel");
  for (const QJsonValue& value : std::as_const(m_designMaterials)) {
    if (text(value.toObject(), "id") == QStringLiteral("steel")) {
      steel = text(value.toObject(), "name");
    }
  }
  if (isRootDefault) {
    for (const QJsonValue& value : std::as_const(m_bodies)) {
      if (text(value.toObject(), "material").isEmpty()) {
        users << text(value.toObject(), "name");
      }
    }
  }
  users.removeDuplicates();
  const QString fallback =
      tr("Default (%1)").arg(rootDefault.isEmpty() || isRootDefault ? steel : rootDefault);
  if (!confirmDelete(this, target, m_log, users, fallback)) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("delete_material")}};
  nameItem(command, target, project());
  if (run(command)) {
    qDebug().noquote() << QStringLiteral("Material deleted: %1 (%2)").arg(target.name, target.place);
  }
  refresh();
}

void MaterialsPanel::copyTo() {
  const LibraryItem* item = current();
  if (item == nullptr || !canChange()) {
    return;
  }
  const LibraryItem source = *item;
  // The other places (mitcad#1135); a copy keeps a link to its source only
  // as before (copyCommand).
  const QStringList targets = copyTargets(source, hasDesign(), !project().isEmpty());
  const QString place = choosePlaceAmong(m_copyTo, m_log + QStringLiteral(" copy_to"), targets, targets.value(0));
  if (place.isEmpty()) {
    return;
  }
  QJsonObject result;
  if (run(copyCommand(source, place, project()), &result)) {
    qDebug().noquote() << QStringLiteral("Material copied to %1: %2").arg(place, source.name);
    // The copy selected; a design's item saved into a library stays
    // selected, its copy now.
    if (!text(result, "id").isEmpty()) {
      m_currentKey = place + QLatin1Char('/') + text(result, "id");
    }
  }
  refresh();
}

void MaterialsPanel::askChanges() {
  const LibraryItem* item = current();
  if (item == nullptr || !item->changed || changeAsked(*item) || !isVisible() ||
      !m_host->canChangePrintSettings()) {
    return;
  }
  const LibraryItem target = *item;
  // Use the changes or Keep these, one undo step either way (mitcad#655).
  const QJsonObject command = askChangedElsewhere(this, target, m_log, project());
  if (run(command)) {
    qDebug().noquote() << QStringLiteral("Material %1: %2").arg(target.name, command.value(QStringLiteral("cmd")).toString());
  }
  refresh();
}

BodyMaterialCommands bodyMaterialCommands(QWidget* parent, const LibraryItem* item, const QStringList& offered,
                                          const QJsonArray& bodyRows, const QStringList& bodies,
                                          const QString& project) {
  BodyMaterialCommands result;
  result.appearance = QStringLiteral("replace");
  QStringList conflicts;
  for (const QJsonValue& value : bodyRows) {
    const QJsonObject body = value.toObject();
    // Only an appearance set by hand the new material does not offer
    // (mitcad#1067): one it offers stays the body's either way.
    if (bodies.contains(text(body, "uid")) && item != nullptr && asksToReplaceAppearance(body, offered)) {
      conflicts << text(body, "name");
    }
  }
  if (!conflicts.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Material: replace the appearance of %1?")
                              .arg(conflicts.join(QStringLiteral(", ")));
    QMessageBox box(QMessageBox::Question, MaterialsPanel::tr("Replace Appearance"),
                    conflicts.size() == 1
                        ? MaterialsPanel::tr("%1 has an appearance set by hand.").arg(conflicts.first())
                        : MaterialsPanel::tr("%1 bodies have appearances set by hand.").arg(conflicts.size()),
                    QMessageBox::Cancel, parent);
    box.setInformativeText(MaterialsPanel::tr("Replace it with the appearance of %1, or keep it?").arg(item->name));
    QPushButton* replace = box.addButton(MaterialsPanel::tr("&Replace"), QMessageBox::AcceptRole);
    QPushButton* keep = box.addButton(MaterialsPanel::tr("&Keep"), QMessageBox::RejectRole);
    box.setDefaultButton(replace);
    prepareModal(&box);
    execDialog(&box);
    if (box.clickedButton() == keep) {
      result.appearance = QStringLiteral("keep");
    } else if (box.clickedButton() != replace) {
      qDebug() << "Material cancelled";
      result.cancelled = true;
      return result;
    }
  }
  for (const QString& body : bodies) {
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_body_material")},
                        {QStringLiteral("uid"), body},
                        {QStringLiteral("appearance"), result.appearance}};
    if (item == nullptr) {
      command.insert(QStringLiteral("material"), QJsonValue());
    } else {
      // A library's material is copied into the design in the same undo
      // step (the command's `library`).
      nameItem(command, *item, project, QStringLiteral("material"));
    }
    result.commands.append(command);
  }
  return result;
}

void MaterialsPanel::logRows() {
  if (!isVisible()) {
    return;
  }
  QStringList rows;
  for (const LibraryItem& item : std::as_const(m_items)) {
    rows << QStringLiteral("%1/%2 %3 (%4)%5")
                .arg(item.group.isEmpty() ? QStringLiteral("other") : item.group, item.id, item.name, item.place,
                     item.changed ? QStringLiteral(" changed") : QString());
  }
  // The places as the list shows them (mitcad#1135), "This design 1 | ...".
  QStringList groups;
  for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
    QTreeWidgetItem* heading = m_list->topLevelItem(i);
    int count = 0;
    for (int j = 0; j < heading->childCount(); ++j) {
      count += heading->child(j)->childCount();
    }
    groups << QStringLiteral("%1 %2").arg(placeName(heading->data(1, kKeyRole).toString())).arg(count);
  }
  const QString logged = rows.join(QLatin1Char('\n')) + QLatin1Char('|') + groups.join(QLatin1Char('|'));
  if (logged == m_loggedRows) {
    return;
  }
  m_loggedRows = logged;
  qDebug().noquote() << QStringLiteral("%1: %2 items").arg(m_log).arg(rows.size());
  qDebug().noquote() << QStringLiteral("%1 groups: %2").arg(m_log, groups.join(QStringLiteral(" | ")));
  for (const QString& row : std::as_const(rows)) {
    qDebug().noquote() << QStringLiteral("%1 row: %2").arg(m_log).arg(row);
  }
  if (const LibraryItem* item = current()) {
    for (const QString& value : m_form->rows()) {
      qDebug().noquote() << QStringLiteral("Material %1 value %2").arg(item->name, value);
    }
  }
}

void MaterialsPanel::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  m_loggedRows.clear();
  m_loggedPlaces.clear();
  m_loggedNames.clear();
  m_loggedEditing.clear();
  logRows();
  // Once the window is laid out: the list as wide as its names.
  TestSync::singleShot(0, this, [this] {
    fitListWidth();
    scheduleLogPlaces();
  });
}

void MaterialsPanel::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  fitListWidth();
  scheduleLogPlaces();
}

bool MaterialsPanel::eventFilter(QObject* watched, QEvent* event) {
  // F2 on a row: its name.
  if (watched == m_list && event->type() == QEvent::KeyPress &&
      static_cast<QKeyEvent*>(event)->key() == Qt::Key_F2 && current() != nullptr && !current()->readOnly) {
    m_name->setFocus(Qt::OtherFocusReason);
    m_name->selectAll();
    requestNativeFocus(m_name);
    return true;
  }
  // Delete or Backspace on an appearance's row: Remove.
  if (watched == m_appearanceList && event->type() == QEvent::KeyPress && m_removeAppearance->isEnabled()) {
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key == Qt::Key_Delete || key == Qt::Key_Backspace) {
      removeAppearance();
      return true;
    }
  }
  return QWidget::eventFilter(watched, event);
}

void MaterialsPanel::scheduleLogPlaces() {
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void MaterialsPanel::logPlaces() {
  // In the main window's coordinates (the parent of the panel's window), as
  // the other logged places.
  if (!isVisible() || window()->parentWidget() == nullptr) {
    return;
  }
  const QPoint origin = window()->parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  // Each row by its place and name; by its name alone the first of that
  // name (This design's before a library's, mitcad#1135).
  QSet<QString> named;
  for (QTreeWidgetItemIterator it(m_list); *it != nullptr; ++it) {
    QTreeWidgetItem* row = *it;
    const QString key = row->data(0, kKeyRole).toString();
    const QRect rect = m_list->visualItemRect(row);
    if (key.isEmpty() || row->isHidden() || !rect.isValid() || !m_list->viewport()->rect().contains(rect.center())) {
      continue;
    }
    const LibraryItem* item = findItem(m_items, key);
    const QString name = item != nullptr ? item->name : key;
    const QString place = at(m_list->viewport(), QPoint(60, rect.center().y()));
    if (!named.contains(name)) {
      named.insert(name);
      lines << QStringLiteral("%1 item %2 at %3").arg(m_log).arg(name, place);
    }
    lines << QStringLiteral("%1 %2 item %3 at %4").arg(m_log, key.section(QLatin1Char('/'), 0, 0), name, place);
  }
  QVector<std::pair<QString, QWidget*>> widgets{
      {QStringLiteral("search"), m_search},
      {QStringLiteral("name"), m_name},
      {QStringLiteral("class"), m_class},
      {QStringLiteral("copy_to"), m_copyTo},
      {QStringLiteral("header_duplicate"), m_headerDuplicate},
      {QStringLiteral("density"), m_density},
      {QStringLiteral("appearance_add"), m_addAppearance},
      {QStringLiteral("appearance_remove"), m_removeAppearance},
      {QStringLiteral("appearance_default"), m_defaultAppearance},
      {QStringLiteral("new"), m_new},
      {QStringLiteral("duplicate"), m_duplicate},
      {QStringLiteral("delete"), m_delete}};
  // The window's buttons (Close).
  for (const auto& [name, button] : std::as_const(m_windowButtons)) {
    widgets.append(std::pair<QString, QWidget*>(name, button));
  }
  for (const auto& [name, widget] : std::as_const(widgets)) {
    if (widget->isVisibleTo(this)) {
      lines << QStringLiteral("%1 %2 at %3").arg(m_log).arg(name, at(widget, widget->rect().center()));
    }
  }
  // The rows of the material's appearances (mitcad#1055, mitcad#1083), at
  // their names.
  if (m_appearanceList->isVisible()) {
    for (int row = 0; row < m_appearanceList->topLevelItemCount(); ++row) {
      QTreeWidgetItem* look = m_appearanceList->topLevelItem(row);
      const QRect rect = m_appearanceList->visualItemRect(look);
      if (!look->data(0, kKeyRole).toString().isEmpty() && rect.isValid() &&
          m_appearanceList->viewport()->rect().contains(rect.center())) {
        lines << QStringLiteral("%1 appearance row %2 at %3")
                     .arg(m_log, look->text(0),
                          at(m_appearanceList->viewport(), QPoint(kSwatchSize + 40, rect.center().y())));
      }
    }
  }
  if (m_form->isVisibleTo(this)) {
    lines << m_form->places(m_log + QStringLiteral(" settings"), at);
  }
  // The Variants tab's list, buttons and form (mitcad#1054).
  lines << m_variants->places(at);
  // Whether every material's name shows in full (mitcad#1083): logged when
  // the names or the cut ones change, "Materials list: 31 names, none cut"
  // or "... cut: <name> | <name>".
  QStringList names;
  for (QTreeWidgetItemIterator it(m_list); *it != nullptr; ++it) {
    if (!(*it)->data(0, kKeyRole).toString().isEmpty()) {
      names << (*it)->text(0);
    }
  }
  const QStringList cut = cutNames();
  const QString namesState = names.join(QLatin1Char('\n')) + QLatin1Char('|') + cut.join(QLatin1Char('\n'));
  if (namesState != m_loggedNames) {
    m_loggedNames = namesState;
    qDebug().noquote() << QStringLiteral("%1 list: %2 names, %3")
                              .arg(m_log)
                              .arg(names.size())
                              .arg(cut.isEmpty() ? QStringLiteral("none cut")
                                                 : QStringLiteral("cut: ") + cut.join(QStringLiteral(" | ")));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
