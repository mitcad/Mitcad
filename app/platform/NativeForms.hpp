// SPDX-License-Identifier: MIT
#pragma once

#include <memory>

#include "../framework/CommandPresenter.hpp"

class QAction;
class QWidget;

namespace mitcad {

class CommandSession;

// Owns an AppKit form. The window retains its NSView while installed; remove
// it from the inspector before destroying the form. Destroying the session
// first is also safe: it clears the presenter's actions.
class NativeCommandForm : public CommandPresenter {
public:
  virtual void* nativeView() const = 0;
  // OK and Cancel, apart from nativeView (NSView*, mitcad#815): the window
  // keeps them under the scrolling inputs, as the shared panel's page does
  // (mitcad#655). Null when nativeView holds them.
  virtual void* nativeFooterView() const { return nullptr; }
  // A command in the title row, after the command's name (Change
  // Parameters, mitcad#564), as CommandPanel::addHeaderAction.
  virtual void addHeaderAction(QAction*) {}
  // Takes ownership of a shared presenter section above the result, as
  // CommandPanel::addSection does. Its controls are adapted to native views;
  // the QWidget itself is never shown. It survives input-row rebuilds and
  // is destroyed with the form, after detaching its native bindings.
  virtual void addSection(QWidget* section) = 0;
};

#ifdef MITCAD_MAC_UI
// `parameterOwner` is the window whose ParameterSource the value fields'
// expression tools use (setParameterSource); none gives no fx tools.
std::unique_ptr<NativeCommandForm> createNativeCommandForm(CommandSession& session,
                                                           QWidget* parameterOwner = nullptr);
#else
inline std::unique_ptr<NativeCommandForm> createNativeCommandForm(CommandSession&, QWidget* = nullptr) { return {}; }
#endif

} // namespace mitcad
