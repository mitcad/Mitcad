// SPDX-License-Identifier: MIT
#pragma once

// The Suppliers page of Preferences (mitcad#950; app/COMMANDS.md,
// "Suppliers' shops"): the Region the product links are made for (None,
// then the regions MitCad knows shops in) and the Shops you buy from (the
// shops of that region, each a check box). The shops chosen are the
// Supplier filters' default (SupplierShops.hpp). Saved when Preferences
// is accepted, at once in the macOS Settings window.

#include <QGroupBox>
#include <QJsonObject>
#include <QWidget>

#include "SupplierShops.hpp"

class QComboBox;
class QListWidget;

namespace mitcad {

// A group with a frame and a title, except on macOS, where the settings
// window's panes have none: a plain widget.
#ifdef MITCAD_MAC_UI
using SupplierPreferencesBase = QWidget;
#else
using SupplierPreferencesBase = QGroupBox;
#endif

class SupplierPreferencesBox : public SupplierPreferencesBase {
  Q_OBJECT

public:
  explicit SupplierPreferencesBox(QWidget* parent = nullptr);

  // The region and shops as the box shows them.
  ShopSettings chosen() const;
  // Saves them when they changed and logs "Preferences: suppliers region
  // <code|none>, shops <ids|none>".
  void apply();

signals:
  void changed();

private:
  // The region's shops, the checked ones kept.
  void fillShops();

  QJsonObject m_list;
  ShopSettings m_settings;
  QComboBox* m_region = nullptr;
  QListWidget* m_shops = nullptr;
};

} // namespace mitcad
