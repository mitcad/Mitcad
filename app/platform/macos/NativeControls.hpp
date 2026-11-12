// SPDX-License-Identifier: MIT
#pragma once

// AppKit controls that the generic presenter bridge (NativeDialogs.mm) and
// the command forms (NativeForms.mm) share (mitcad#815). Objective-C++
// only: include it from .mm files.

#import <AppKit/AppKit.h>

#include <functional>
#include <vector>

#include <QJsonArray>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QWidget>

#include "../../framework/ParameterField.hpp"

class QLineEdit;

// A tool button with its text under its icon (Qt::ToolButtonTextUnderIcon:
// the selection card's Create row): the shared artwork over a caption of
// one or two lines, the whole tile one pointer target, with hover, pressed
// and checked highlights. Its title and image are the caption's and the
// icon's, so the generic bridge sets them as on any button.
@interface MitcadNativeTileButton : NSButton
- (instancetype)initWithIconSize:(CGFloat)iconSize;
@end

@class MitcadNativeExpressionTarget;

namespace mitcad {

// The shared expression tools of an AppKit value field (mitcad#567,
// mitcad#568, mitcad#815): an fx button beside it whose menu lists the
// design's parameters by ParameterSuggest's rules (those the field cannot
// take disabled, with the reason in their tooltip), then Name This Value,
// Add User Parameter… and Edit Formula…, which run the shared
// ParameterField workflow and its dialogs on a hidden Qt proxy of the
// field, and the parameter names for AppKit's completion (F5 or
// Option-Esc). A chosen name goes into the field as typing would, so the
// command form's own text notifications carry it to the session.
class NativeExpressionField {
public:
  // `host` is nativeExpressionHost's window; `options` says what the field
  // is now (its own parameter's name follows the command's state).
  // An inline child panel restores its own key field after a menu/modal tool;
  // ordinary document forms need no additional focus handling.
  NativeExpressionField(QWidget* host, NSTextField* field, std::function<ParameterFieldOptions()> options,
                        std::function<void()> restoreFocus = {});
  ~NativeExpressionField();
  NativeExpressionField(const NativeExpressionField&) = delete;
  NativeExpressionField& operator=(const NativeExpressionField&) = delete;

  // The fx button, to be placed beside the field.
  NSButton* button() const { return m_button; }
  // The parameter names that complete the name at `caret` in `text` and
  // that the field can take; empty where no name is being typed.
  NSArray<NSString*>* completions(const QString& text, int caret);

  // The fx button and its menu's items (MitcadNativeExpressionTarget).
  void openMenu();
  void insertName(const QString& name, bool function);
  void nameValue();
  void createParameter();
  void editFormula();

private:
  // The text, caret and selection a choice goes into.
  struct Anchor {
    QString text;
    int caret = 0;
    int selection = -1; // none
    int selectionLength = 0;
  };

  Anchor currentAnchor() const;
  // The field's editor, giving the field the keyboard; nil when it cannot.
  NSTextView* editor() const;
  // The design's parameters and functions now (the last ones read while a
  // job computes the model).
  void fetch();
  NSMenu* menu();
  // The hidden Qt field the shared tools act on, holding the field's text,
  // caret and selection as the menu found them.
  QLineEdit* proxy();
  // What the shared tools wrote into the proxy goes into the field.
  void proxyEdited(const QString& text);
  // A shared editor chosen in the menu opens once the menu has closed and
  // the fx button's click has returned: never inside the menu's tracking.
  enum class Tool { None, Create, Formula };
  void openTool(Tool tool);

  QPointer<QWidget> m_host;
  NSTextField* __weak m_field = nil;
  std::function<ParameterFieldOptions()> m_options;
  std::function<void()> m_restoreFocus;
  MitcadNativeExpressionTarget* __strong m_target = nil;
  NSButton* __strong m_button = nil;
  QPointer<QLineEdit> m_proxy;
  QObject m_context; // the proxy's connections end with this field
  Anchor m_anchor;
  std::vector<SuggestedParameter> m_parameters;
  QJsonArray m_functions;
  Tool m_pending = Tool::None;
};

// A hidden window, kept for as long as `owner` lives, that the expression
// fields' proxies and the shared editors' dialogs belong to: it carries a
// copy of the owner's ParameterSource and is never shown, created or
// activated. Null when `owner` has no ParameterSource.
QWidget* nativeExpressionHost(QWidget* owner);

} // namespace mitcad
