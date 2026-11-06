// SPDX-License-Identifier: MIT
#include "SupplierPreferences.hpp"

#include <algorithm>

#include <QComboBox>
#include <QFormLayout>
#include <QJsonArray>
#include <QJsonValue>
#include <QLabel>
#include <QListWidget>
#include <QListWidgetItem>
#include <QSignalBlocker>
#include <QStringList>
#include <QtLogging>

#include "../platform/MacDialogs.hpp"

namespace mitcad {
namespace {

constexpr int kSupplierRole = Qt::UserRole + 1;

QString listText(const QStringList& items) {
  return items.isEmpty() ? QStringLiteral("none") : items.join(QStringLiteral(", "));
}

} // namespace

SupplierPreferencesBox::SupplierPreferencesBox(QWidget* parent)
#ifdef MITCAD_MAC_UI
    : QWidget(parent),
#else
    : QGroupBox(tr("Suppliers"), parent),
#endif
      m_list(supplierShops()), m_settings(ShopSettings::load()) {
  auto* form = new QFormLayout(this);
  m_region = new QComboBox;
  m_region->setObjectName(QStringLiteral("supplierRegion"));
  m_region->setToolTip(tr("The country whose shops the suppliers' part numbers link to"));
  m_region->addItem(tr("None"), QString());
  for (const QJsonValue& value : m_list.value(QStringLiteral("regions")).toArray()) {
    const QJsonObject region = value.toObject();
    m_region->addItem(region.value(QStringLiteral("name")).toString(), region.value(QStringLiteral("id")).toString());
  }
  // A region kept that the list no longer has: shown by its code.
  if (!m_settings.region.isEmpty() && m_region->findData(m_settings.region) < 0) {
    m_region->addItem(m_settings.region, m_settings.region);
  }
  m_region->setCurrentIndex(std::max(0, m_region->findData(m_settings.region)));
  form->addRow(tr("&Region:"), m_region);
  m_shops = new QListWidget;
  m_shops->setObjectName(QStringLiteral("supplierShops"));
  m_shops->setAccessibleName(tr("Shops you buy from"));
  m_shops->setToolTip(tr("The Supplier filters start with the shops checked here instead of All"));
  m_shops->setMinimumHeight(90);
  mac::stylePreview(m_shops);
  form->addRow(tr("&Shops you buy from:"), m_shops);
  auto* note = new QLabel(tr("The suppliers' part numbers of library components link to their product pages in "
                             "the region's shops. The Supplier filters of the component picker and the size "
                             "choosers start with the shops checked; with none checked they start with All."));
  note->setWordWrap(true);
  form->addRow(note);
  fillShops();
  connect(m_region, &QComboBox::currentIndexChanged, this, [this] {
    fillShops();
    emit changed();
  });
  connect(m_shops, &QListWidget::itemChanged, this, [this] { emit changed(); });
}

void SupplierPreferencesBox::fillShops() {
  // The checks made here before the region changed, else the kept shops.
  QStringList checked = m_settings.shops;
  if (m_shops->count() > 0) {
    checked = chosen().shops;
  }
  const QSignalBlocker blocker(m_shops);
  m_shops->clear();
  const QList<RegionShop> shops = regionShops(m_list, m_region->currentData().toString());
  for (const RegionShop& shop : shops) {
    auto* item = new QListWidgetItem(tr("%1 (%2)").arg(shop.name, shop.supplierName));
    item->setData(kSupplierRole, shop.supplier);
    item->setToolTip(shop.url);
    item->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsUserCheckable);
    item->setCheckState(checked.contains(shop.supplier, Qt::CaseInsensitive) ? Qt::Checked : Qt::Unchecked);
    m_shops->addItem(item);
  }
  if (m_shops->count() > 0) {
    m_shops->setCurrentRow(0);
  }
  m_shops->setEnabled(!shops.isEmpty());
}

ShopSettings SupplierPreferencesBox::chosen() const {
  ShopSettings out;
  out.region = m_region->currentData().toString();
  for (int i = 0; i < m_shops->count(); ++i) {
    const QListWidgetItem* item = m_shops->item(i);
    if (item->checkState() == Qt::Checked) {
      out.shops << item->data(kSupplierRole).toString();
    }
  }
  return out;
}

void SupplierPreferencesBox::apply() {
  const ShopSettings now = chosen();
  if (now == m_settings) {
    return;
  }
  now.save();
  m_settings = ShopSettings::load();
  qDebug().noquote() << QStringLiteral("Preferences: suppliers region %1, shops %2")
                            .arg(m_settings.region.isEmpty() ? QStringLiteral("none") : m_settings.region,
                                 listText(m_settings.shops));
}

} // namespace mitcad
