// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>
#include <QPoint>
#include <QRect>
#include <QtGlobal>

class QWidget;

namespace mitcad {

// Presents the shared BindingsCard widgets and actions with AppKit controls.
// The presenter remains responsible for binding semantics and pointer follow.
class NativeBindingsCardView {
public:
  virtual ~NativeBindingsCardView() = default;
  virtual void show(const QRect& anchor, bool pinned) = 0;
  virtual void hide() = 0;
  virtual void refresh() = 0;
  virtual bool contains(const QPoint& screenPoint) const = 0;
};

#if defined(MITCAD_MAC_UI) && defined(MITCAD_MACOS_NATIVE_UI)
std::unique_ptr<NativeBindingsCardView> createNativeBindingsCardView(QWidget* source,
                                                                  std::function<void()> closed);
#else
inline std::unique_ptr<NativeBindingsCardView> createNativeBindingsCardView(QWidget*,
                                                                         std::function<void()>) { return {}; }
#endif

} // namespace mitcad
