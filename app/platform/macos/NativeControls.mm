// SPDX-License-Identifier: MIT
#include "NativeControls.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <utility>

#include <QJsonObject>
#include <QJsonValue>
#include <QLineEdit>
#include <QtLogging>

#include "../../framework/ParameterSuggest.hpp"
#include "../../framework/TestSync.hpp"

namespace {

NSString* nativeText(const QString& value) { return value.toNSString(); }

QString qtText(NSString* value) { return value == nil ? QString() : QString::fromNSString(value); }

NSUInteger clampedIndex(int value, NSUInteger length) {
  return std::min<NSUInteger>(static_cast<NSUInteger>(std::max(0, value)), length);
}

} // namespace

@implementation MitcadNativeTileButton {
  NSImageView* _icon;
  NSTextField* _caption;
  NSLayoutConstraint* _captionHeight;
  NSTrackingArea* _hover;
  CGFloat _side;
  BOOL _inside;
}

- (instancetype)initWithIconSize:(CGFloat)iconSize {
  self = [super initWithFrame:NSZeroRect];
  if (self == nil) {
    return nil;
  }
  _side = std::max<CGFloat>(16, iconSize);
  [super setTitle:@""];
  [super setImage:nil];
  self.bordered = NO;
  self.translatesAutoresizingMaskIntoConstraints = NO;
  _icon = [[NSImageView alloc] initWithFrame:NSZeroRect];
  _icon.imageScaling = NSImageScaleProportionallyUpOrDown;
  _icon.translatesAutoresizingMaskIntoConstraints = NO;
  // Two lines at most (SelectionCreatePanel::buttonText), each whole; a
  // row too narrow for its tiles elides rather than overflowing the card.
  _caption = [NSTextField wrappingLabelWithString:@""];
  _caption.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
  _caption.alignment = NSTextAlignmentCenter;
  _caption.selectable = NO;
  _caption.maximumNumberOfLines = 2;
  _caption.cell.truncatesLastVisibleLine = YES;
  _caption.translatesAutoresizingMaskIntoConstraints = NO;
  for (NSLayoutConstraintOrientation orientation :
       {NSLayoutConstraintOrientationHorizontal, NSLayoutConstraintOrientationVertical}) {
    [_caption setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:orientation];
    [_caption setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:orientation];
  }
  [self addSubview:_icon];
  [self addSubview:_caption];
  _captionHeight = [_caption.heightAnchor constraintEqualToConstant:0];
  [NSLayoutConstraint activateConstraints:@[
    [_icon.widthAnchor constraintEqualToConstant:_side],
    [_icon.heightAnchor constraintEqualToConstant:_side],
    [_icon.topAnchor constraintEqualToAnchor:self.topAnchor constant:5],
    [_icon.centerXAnchor constraintEqualToAnchor:self.centerXAnchor],
    [_caption.topAnchor constraintEqualToAnchor:_icon.bottomAnchor constant:2],
    [_caption.leadingAnchor constraintEqualToAnchor:self.leadingAnchor constant:3],
    [_caption.trailingAnchor constraintEqualToAnchor:self.trailingAnchor constant:-3],
    [_caption.bottomAnchor constraintLessThanOrEqualToAnchor:self.bottomAnchor constant:-3],
    _captionHeight
  ]];
  [self setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationHorizontal];
  [self setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationVertical];
  return self;
}

- (NSSize)captionSize {
  return [_caption.cell cellSizeForBounds:NSMakeRect(0, 0, CGFLOAT_MAX, CGFLOAT_MAX)];
}

- (NSSize)intrinsicContentSize {
  const NSSize text = [self captionSize];
  return NSMakeSize(std::ceil(std::max(_side + 12, text.width + 10)), std::ceil(5 + _side + 2 + text.height + 5));
}

- (NSString*)title {
  return _caption != nil ? _caption.stringValue : @"";
}

- (void)setTitle:(NSString*)title {
  if (_caption == nil) {
    return; // NSButton's own initialisation
  }
  _caption.stringValue = title != nil ? title : @"";
  _captionHeight.constant = std::ceil([self captionSize].height);
  [self invalidateIntrinsicContentSize];
}

- (NSImage*)image {
  return _icon.image;
}

- (void)setImage:(NSImage*)image {
  _icon.image = image;
}

- (void)setEnabled:(BOOL)enabled {
  [super setEnabled:enabled];
  _caption.textColor = enabled ? NSColor.labelColor : NSColor.disabledControlTextColor;
  _icon.alphaValue = enabled ? 1.0 : 0.4;
}

// The caption and icon never take the pointer: the tile is one target.
- (NSView*)hitTest:(NSPoint)point {
  return [super hitTest:point] != nil ? self : nil;
}

- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  if (_hover != nil) {
    [self removeTrackingArea:_hover];
  }
  _hover = [[NSTrackingArea alloc]
      initWithRect:NSZeroRect
           options:NSTrackingMouseEnteredAndExited | NSTrackingActiveInKeyWindow | NSTrackingInVisibleRect
             owner:self
          userInfo:nil];
  [self addTrackingArea:_hover];
}

- (void)mouseEntered:(NSEvent*)event {
  [super mouseEntered:event];
  _inside = YES;
  self.needsDisplay = YES;
}

- (void)mouseExited:(NSEvent*)event {
  [super mouseExited:event];
  _inside = NO;
  self.needsDisplay = YES;
}

- (NSBezierPath*)tileShape {
  return [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 1, 1) xRadius:7 yRadius:7];
}

// The highlights are drawn here, not by the cell's layer.
- (BOOL)wantsUpdateLayer {
  return NO;
}

// Quiet fills as the shared macOS tool-button style has them: hover,
// pressed and checked (the accent); the resting tile is transparent.
- (void)drawRect:(NSRect)dirtyRect {
  (void)dirtyRect;
  NSColor* fill = nil;
  if (self.cell.highlighted) {
    fill = [NSColor.labelColor colorWithAlphaComponent:0.16];
  } else if (self.state == NSControlStateValueOn) {
    fill = [NSColor.controlAccentColor colorWithAlphaComponent:0.22];
  } else if (_inside && self.enabled) {
    fill = [NSColor.labelColor colorWithAlphaComponent:0.08];
  }
  if (fill != nil) {
    [fill setFill];
    [[self tileShape] fill];
  }
}

- (void)drawFocusRingMask {
  [[self tileShape] fill];
}

- (NSRect)focusRingMaskBounds {
  return self.bounds;
}

@end

@interface MitcadNativeExpressionTarget : NSObject {
@public
  mitcad::NativeExpressionField* field;
}
- (void)openMenu:(id)sender;
- (void)insertParameter:(id)sender;
- (void)insertFunction:(id)sender;
- (void)nameValue:(id)sender;
- (void)createParameter:(id)sender;
- (void)editFormula:(id)sender;
@end

@implementation MitcadNativeExpressionTarget
- (void)openMenu:(id)sender {
  (void)sender;
  if (field != nullptr) {
    field->openMenu();
  }
}
- (void)insertParameter:(id)sender {
  if (field != nullptr && [sender isKindOfClass:NSMenuItem.class]) {
    field->insertName(qtText(static_cast<NSMenuItem*>(sender).representedObject), false);
  }
}
- (void)insertFunction:(id)sender {
  if (field != nullptr && [sender isKindOfClass:NSMenuItem.class]) {
    field->insertName(qtText(static_cast<NSMenuItem*>(sender).representedObject), true);
  }
}
- (void)nameValue:(id)sender {
  (void)sender;
  if (field != nullptr) {
    field->nameValue();
  }
}
- (void)createParameter:(id)sender {
  (void)sender;
  if (field != nullptr) {
    field->createParameter();
  }
}
- (void)editFormula:(id)sender {
  (void)sender;
  if (field != nullptr) {
    field->editFormula();
  }
}
@end

namespace mitcad {

NativeExpressionField::NativeExpressionField(QWidget* host, NSTextField* field,
                                             std::function<ParameterFieldOptions()> options,
                                             std::function<void()> restoreFocus)
    : m_host(host), m_field(field), m_options(std::move(options)), m_restoreFocus(std::move(restoreFocus)) {
  m_target = [MitcadNativeExpressionTarget new];
  m_target->field = this;
  // "fx" in the secondary text colour, as the shared field's own button.
  NSFont* font = [NSFontManager.sharedFontManager convertFont:[NSFont boldSystemFontOfSize:11]
                                                  toHaveTrait:NSItalicFontMask];
  m_button = [NSButton buttonWithTitle:@"fx" target:m_target action:@selector(openMenu:)];
  m_button.bordered = NO;
  m_button.attributedTitle = [[NSAttributedString alloc]
      initWithString:@"fx"
          attributes:@{NSFontAttributeName : font, NSForegroundColorAttributeName : NSColor.secondaryLabelColor}];
  // The field keeps the keyboard, its caret and selection while the menu
  // is used.
  m_button.refusesFirstResponder = YES;
  const ParameterFieldOptions now = m_options ? m_options() : ParameterFieldOptions();
  const QString tip = now.naming
                          ? QObject::tr("Insert a parameter (fx), or type part of its name. name = value names this value.")
                          : QObject::tr("Insert a parameter (fx). Type part of a name to see the parameters.");
  m_button.toolTip = nativeText(tip);
  m_button.accessibilityLabel = nativeText(QObject::tr("Insert a parameter (fx)"));
  m_button.translatesAutoresizingMaskIntoConstraints = NO;
  [m_button.widthAnchor constraintEqualToConstant:22].active = YES;
  [m_button.heightAnchor constraintEqualToConstant:22].active = YES;
}

NativeExpressionField::~NativeExpressionField() {
  m_target->field = nullptr;
  m_button.target = nil;
  // A shared editor still open on the proxy checks it before using it; the
  // proxy's connection to this field ends with m_context.
  if (QLineEdit* edit = m_proxy.data()) {
    edit->deleteLater();
  }
}

NativeExpressionField::Anchor NativeExpressionField::currentAnchor() const {
  Anchor at;
  NSTextField* field = m_field;
  NSText* text = field.currentEditor;
  if (text != nil) {
    at.text = qtText(text.string);
    const NSRange range = text.selectedRange;
    at.caret = static_cast<int>(NSMaxRange(range));
    if (range.length > 0) {
      at.selection = static_cast<int>(range.location);
      at.selectionLength = static_cast<int>(range.length);
    }
    return at;
  }
  // A field not being edited takes a choice in place of its whole text.
  at.text = qtText(field.stringValue);
  at.caret = static_cast<int>(at.text.size());
  if (!at.text.isEmpty()) {
    at.selection = 0;
    at.selectionLength = static_cast<int>(at.text.size());
  }
  return at;
}

NSTextView* NativeExpressionField::editor() const {
  NSTextField* field = m_field;
  if (field == nil || field.window == nil || !field.enabled || !field.editable) {
    return nil;
  }
  if (field.currentEditor == nil && ![field.window makeFirstResponder:field]) {
    return nil;
  }
  NSText* text = field.currentEditor;
  return [text isKindOfClass:NSTextView.class] ? static_cast<NSTextView*>(text) : nil;
}

void NativeExpressionField::fetch() {
  const ParameterSource* source = parameterSourceOf(m_host.data());
  if (source == nullptr || !source->parameters) {
    m_parameters.clear();
    return;
  }
  try {
    m_parameters = suggestedParameters(source->parameters());
    if (source->query) {
      m_functions = source->query({{QStringLiteral("query"), QStringLiteral("formula_help")}})
                        .value(QStringLiteral("functions"))
                        .toArray();
    }
  } catch (const std::exception& e) {
    // A job computes the model: the parameters read last.
    qDebug().noquote() << QStringLiteral("Parameter browser: parameters not read now: %1")
                              .arg(QString::fromUtf8(e.what()));
  }
}

NSMenu* NativeExpressionField::menu() {
  const ParameterFieldOptions options = m_options ? m_options() : ParameterFieldOptions();
  const ParameterSource* source = parameterSourceOf(m_host.data());
  NSMenu* result = [[NSMenu alloc] initWithTitle:@"fx"];
  result.autoenablesItems = NO;
  SuggestContext context;
  context.kind = options.kind;
  context.anyKind = options.anyKind;
  context.own = options.own;
  context.sketch = options.sketch;
  const Anchor at = m_anchor;
  const auto inserted = [&at](const QString& name) {
    return insertParameter(at.text, at.caret, at.selection, at.selectionLength, name).text;
  };
  const std::vector<ParameterChoice> choices = matchParameters(m_parameters, QString(), true, context, inserted);
  int group = -1;
  for (const ParameterChoice& choice : choices) {
    const SuggestedParameter& p = *choice.parameter;
    if (static_cast<int>(p.group) != group) {
      group = static_cast<int>(p.group);
      [result addItem:[NSMenuItem sectionHeaderWithTitle:nativeText(p.group == SuggestedParameter::Group::User
                                                                        ? QObject::tr("User Parameters")
                                                                        : QObject::tr("Model Parameters"))]];
    }
    NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:nativeText(p.name)
                                                  action:@selector(insertParameter:)
                                           keyEquivalent:@""];
    item.target = m_target;
    item.representedObject = nativeText(p.name);
    // The parameter's value and comment, as the shared list's columns.
    QString tip = QStringLiteral("%1 = %2").arg(p.name, p.expression);
    if (!p.text.isEmpty() && p.text != p.expression) {
      tip += QStringLiteral(" → %1").arg(p.text);
    }
    tip += QStringLiteral(" (%1)").arg(p.comment.isEmpty() ? parameterKindName(p.kind) : p.comment);
    if (!choice.problem.isEmpty()) {
      tip += QLatin1Char('\n') + choice.problem;
    }
    item.toolTip = nativeText(tip);
    item.enabled = choice.problem.isEmpty();
    [result addItem:item];
  }
  if (choices.empty()) {
    NSMenuItem* none = [[NSMenuItem alloc]
        initWithTitle:nativeText(m_parameters.empty() ? QObject::tr("The design has no parameters yet")
                                                      : QObject::tr("No parameter fits this field"))
               action:nil
        keyEquivalent:@""];
    none.enabled = NO;
    [result addItem:none];
  }
  [result addItem:NSMenuItem.separatorItem];
  NSMenuItem* naming = nil;
  if (options.naming) {
    naming = [[NSMenuItem alloc] initWithTitle:nativeText(QObject::tr("Name This Value (name = value)"))
                                        action:@selector(nameValue:)
                                 keyEquivalent:@""];
    naming.target = m_target;
    [result addItem:naming];
  }
  if (options.create && source != nullptr && source->addParameter) {
    NSMenuItem* create = [[NSMenuItem alloc] initWithTitle:nativeText(QObject::tr("Add User Parameter…"))
                                                    action:@selector(createParameter:)
                                             keyEquivalent:@""];
    create.target = m_target;
    create.toolTip = nativeText(QObject::tr("a new parameter of its own"));
    [result addItem:create];
  }
  NSMenuItem* formula = [[NSMenuItem alloc] initWithTitle:nativeText(QObject::tr("Edit Formula…"))
                                                   action:@selector(editFormula:)
                                            keyEquivalent:@""];
  formula.target = m_target;
  formula.enabled = source != nullptr && static_cast<bool>(source->query);
  formula.toolTip =
      nativeText(QObject::tr("A larger formula editor with functions, templates and parameter limits"));
  [result addItem:formula];
  if (!m_functions.isEmpty()) {
    NSMenu* functions = [[NSMenu alloc] initWithTitle:nativeText(QObject::tr("Functions"))];
    functions.autoenablesItems = NO;
    for (const QJsonValue& value : m_functions) {
      const QJsonObject function = value.toObject();
      const QString name = function.value(QStringLiteral("name")).toString();
      if (name.isEmpty()) {
        continue;
      }
      NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:nativeText(name)
                                                    action:@selector(insertFunction:)
                                             keyEquivalent:@""];
      item.target = m_target;
      item.representedObject = nativeText(name + QStringLiteral("()"));
      item.toolTip = nativeText(function.value(QStringLiteral("description")).toString() + QLatin1Char('\n') +
                                function.value(QStringLiteral("example")).toString());
      [functions addItem:item];
    }
    NSMenuItem* holder = [[NSMenuItem alloc] initWithTitle:nativeText(QObject::tr("Functions"))
                                                    action:nil
                                             keyEquivalent:@""];
    holder.submenu = functions;
    [result addItem:holder];
  }
  return result;
}

void NativeExpressionField::openMenu() {
  NSTextField* field = m_field;
  NSButton* button = m_button;
  if (field == nil || !field.enabled || button.window == nil) {
    return;
  }
  m_anchor = currentAnchor();
  m_pending = Tool::None;
  fetch();
  NSMenu* shown = menu();
  qDebug().noquote() << QStringLiteral("Parameter browser opened for %1")
                            .arg(m_options ? m_options().label : QString());
  // Under the button, as a pull-down; its items act during the tracking.
  const NSRect bounds = button.bounds;
  const NSPoint at = NSMakePoint(NSMinX(bounds), button.flipped ? NSMaxY(bounds) + 2 : NSMinY(bounds) - 2);
  MitcadNativeExpressionTarget* target = m_target; // tells whether this field outlived the menu
  [shown popUpMenuPositioningItem:nil atLocation:at inView:button];
  if (target->field != this) {
    return;
  }
  const Tool tool = std::exchange(m_pending, Tool::None);
  if (tool != Tool::None) {
    openTool(tool);
  } else if (m_restoreFocus) {
    m_restoreFocus();
  }
}

void NativeExpressionField::insertName(const QString& name, bool function) {
  NSTextView* text = editor();
  if (text == nil || name.isEmpty()) {
    return;
  }
  const Anchor at = m_anchor;
  const QString now = qtText(text.string);
  Insertion insertion;
  if (now == at.text) {
    insertion = insertParameter(at.text, at.caret, at.selection, at.selectionLength, name);
  } else {
    // The text changed meanwhile: at the caret, as the shared field does.
    const NSRange range = text.selectedRange;
    insertion = insertParameter(now, static_cast<int>(NSMaxRange(range)),
                                range.length > 0 ? static_cast<int>(range.location) : -1,
                                static_cast<int>(range.length), name);
  }
  const NSUInteger length = text.string.length;
  const NSUInteger from = clampedIndex(insertion.from, length);
  const NSUInteger replaced = std::min<NSUInteger>(clampedIndex(insertion.length, length), length - from);
  // The field editor's own edit: its undo and the form's text notification.
  [text insertText:nativeText(name) replacementRange:NSMakeRange(from, replaced)];
  const int caret = function ? insertion.caret - 1 : insertion.caret;
  text.selectedRange = NSMakeRange(clampedIndex(caret, text.string.length), 0);
  qDebug().noquote() << QStringLiteral("Parameter %1 inserted %2 into %3: %4")
                            .arg(function ? QStringLiteral("function") : QStringLiteral("browser"), name,
                                 m_options ? m_options().label : QString(), qtText(text.string));
}

QLineEdit* NativeExpressionField::proxy() {
  if (m_host.isNull()) {
    return nullptr;
  }
  if (m_proxy.isNull()) {
    auto* edit = new QLineEdit(m_host.data());
    edit->setObjectName(QStringLiteral("nativeExpressionProxy"));
    edit->setAttribute(Qt::WA_DontShowOnScreen);
    edit->hide();
    QObject::connect(edit, &QLineEdit::textEdited, &m_context, [this](const QString& text) { proxyEdited(text); });
    m_proxy = edit;
  }
  QLineEdit* edit = m_proxy.data();
  ParameterField::attach(edit, m_options ? m_options() : ParameterFieldOptions());
  const Anchor at = m_anchor;
  edit->setText(at.text);
  if (at.selection >= 0 && at.selectionLength > 0) {
    edit->setSelection(at.selection, at.selectionLength);
  } else {
    edit->setCursorPosition(at.caret);
  }
  return edit;
}

void NativeExpressionField::proxyEdited(const QString& value) {
  NSTextView* text = editor();
  if (text == nil) {
    return;
  }
  if (qtText(text.string) != value) {
    [text insertText:nativeText(value) replacementRange:NSMakeRange(0, text.string.length)];
  }
  if (QLineEdit* edit = m_proxy.data()) {
    text.selectedRange = NSMakeRange(clampedIndex(edit->cursorPosition(), text.string.length), 0);
  }
}

void NativeExpressionField::nameValue() {
  QLineEdit* edit = proxy();
  ParameterField* tools = edit != nullptr ? ParameterField::of(edit) : nullptr;
  if (tools == nullptr) {
    return;
  }
  const QPointer<QLineEdit> guard(edit);
  // The name at the caret is the field's own parameter's new name only
  // when no parameter has it: as the design has its parameters now, not as
  // the proxy read them last (before a parameter made since, say).
  tools->refreshParameters();
  tools->startNaming(); // its edit reaches the field through proxyEdited
  NSTextView* text = guard.isNull() ? nil : editor();
  if (text == nil) {
    return;
  }
  // The name it selected, to be typed over.
  const NSUInteger length = text.string.length;
  if (guard->hasSelectedText()) {
    const NSUInteger start = clampedIndex(guard->selectionStart(), length);
    text.selectedRange = NSMakeRange(start, std::min<NSUInteger>(clampedIndex(guard->selectionLength(), length),
                                                                 length - start));
  } else {
    text.selectedRange = NSMakeRange(clampedIndex(guard->cursorPosition(), length), 0);
  }
}

// The shared editors are modal dialogs: chosen while the menu tracks, they
// open after it (openMenu), when the fx button's click has returned.
void NativeExpressionField::createParameter() { m_pending = Tool::Create; }

void NativeExpressionField::editFormula() { m_pending = Tool::Formula; }

void NativeExpressionField::openTool(Tool tool) {
  QLineEdit* edit = proxy();
  if (edit == nullptr) {
    return;
  }
  // On the event loop's next turn, as the shared list's own choice does:
  // the dialog's modal loop never runs inside the click or the menu.
  const QPointer<QLineEdit> guard(edit);
  const QPointer<QObject> lifetime(&m_context);
  const auto restore = m_restoreFocus;
  NSTextField* field = m_field;
  TestSync::singleShot(0, edit, [guard, lifetime, tool, restore, field] {
    if (!lifetime) return; // the field closed before its queued tool opened
    ParameterField* tools = guard.isNull() ? nullptr : ParameterField::of(guard.data());
    if (tools == nullptr) {
      return;
    }
    if (tool == Tool::Create) {
      tools->openCreateEditor();
    } else {
      tools->openFormulaEditor();
    }
    if (!lifetime || !guard || !restore) return;
    restore();
    if (!lifetime || !guard) return;
    // Cancel keeps the original selection; applying a helper leaves the
    // insertion caret from the shared proxy, after the chosen reference.
    if (NSText* text = field.currentEditor) {
      const NSUInteger length = text.string.length;
      const NSUInteger start = clampedIndex(guard->hasSelectedText() ? guard->selectionStart() : guard->cursorPosition(), length);
      text.selectedRange = NSMakeRange(start, guard->hasSelectedText()
          ? std::min<NSUInteger>(clampedIndex(guard->selectionLength(), length), length - start) : 0);
    }
  });
}

NSArray<NSString*>* NativeExpressionField::completions(const QString& text, int caret) {
  NSMutableArray<NSString*>* names = [NSMutableArray array];
  const NameFragment fragment = nameFragmentAt(text, caret);
  if (!fragment.active) {
    return names;
  }
  fetch();
  const ParameterFieldOptions options = m_options ? m_options() : ParameterFieldOptions();
  SuggestContext context;
  context.kind = options.kind;
  context.anyKind = options.anyKind;
  context.own = options.own;
  context.sketch = options.sketch;
  const auto inserted = [&text, caret](const QString& name) {
    return insertParameter(text, caret, -1, 0, name).text;
  };
  for (const ParameterChoice& choice : matchParameters(m_parameters, fragment.typed, false, context, inserted)) {
    if (choice.problem.isEmpty()) {
      [names addObject:nativeText(choice.parameter->name)];
    }
  }
  return names;
}

namespace {

// Keeps the hidden host window for as long as its owner lives.
class ExpressionHostHolder : public QObject {
public:
  ExpressionHostHolder(QObject* owner, QWidget* host) : QObject(owner), m_host(host) {}
  ~ExpressionHostHolder() override { delete m_host.data(); }
  QWidget* host() const { return m_host.data(); }

private:
  QPointer<QWidget> m_host;
};

} // namespace

QWidget* nativeExpressionHost(QWidget* owner) {
  const ParameterSource* source = parameterSourceOf(owner);
  if (owner == nullptr || source == nullptr) {
    return nullptr;
  }
  const QString name = QStringLiteral("nativeExpressionHost");
  auto* holder = dynamic_cast<ExpressionHostHolder*>(owner->findChild<QObject*>(name, Qt::FindDirectChildrenOnly));
  if (holder != nullptr && holder->host() != nullptr) {
    return holder->host();
  }
  // A window of its own without a parent, never shown nor created: the
  // shared editors' dialogs then have no native parent, so their native
  // presentation never ties them to the hidden Qt main window, and their
  // return activates nothing. It answers with the owner's parameters.
  auto* host = new QWidget(nullptr, Qt::Window);
  host->setObjectName(name);
  host->setAttribute(Qt::WA_DontShowOnScreen);
  host->hide();
  setParameterSource(host, *source);
  (new ExpressionHostHolder(owner, host))->setObjectName(name);
  return host;
}

} // namespace mitcad
