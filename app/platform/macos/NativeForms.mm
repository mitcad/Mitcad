// SPDX-License-Identifier: MIT
#import <AppKit/AppKit.h>

#include "../NativeForms.hpp"
#include "../NativeDialogs.hpp"
#include "NativeControls.hpp"

#include <algorithm>
#include <iterator>
#include <memory>
#include <vector>

#include <QAction>
#include <QBuffer>
#include <QHash>
#include <QJsonObject>
#include <QPointer>
#include <QTextDocument>
#include <QTimer>
#include <QWidget>
#include <QtLogging>

#include "../../framework/CommandSession.hpp"
#include "../../framework/CutTargetsPanel.hpp"
#include "../../framework/Icons.hpp"
#include "../../framework/TestSync.hpp"

namespace mitcad {
class AppKitCommandForm;
}

@interface MitcadCommandFormTarget : NSObject <NSTextFieldDelegate> {
@public
  mitcad::AppKitCommandForm* form;
}
- (void)performAction:(id)sender;
@end

namespace mitcad {
namespace {

// The caption column's widest caption; a longer one wraps rather than
// squeezing the controls beside it (mitcad#815).
constexpr CGFloat kCaptionWidth = 110;

NSString* nativeString(const QString& value) {
  const QByteArray utf8 = value.toUtf8();
  return [[NSString alloc] initWithBytes:utf8.constData()
                                length:static_cast<NSUInteger>(utf8.size())
                              encoding:NSUTF8StringEncoding];
}

QString qtString(NSString* value) {
  return value == nil ? QString() : QString::fromUtf8(value.UTF8String);
}

// Shared CAD artwork at `points` logical points, drawn at twice that for
// Retina screens.
NSImage* iconImage(const QIcon& icon, int points, const QString& description) {
  if (icon.isNull()) { return nil; }
  QByteArray data;
  QBuffer buffer(&data);
  buffer.open(QIODevice::WriteOnly);
  icon.pixmap(points * 2, points * 2).save(&buffer, "PNG");
  NSImage* image = [[NSImage alloc] initWithData:[NSData dataWithBytes:data.constData()
                                                              length:static_cast<NSUInteger>(data.size())]];
  image.size = NSMakeSize(points, points);
  image.accessibilityDescription = nativeString(description);
  return image;
}

NSImage* choiceImage(const QString& name, const QString& description) {
  return iconImage(themeIcon(name), 20, description);
}

// A system symbol at the form's small control size.
NSImage* symbolImage(NSString* name, const QString& description) {
  NSImage* image = [NSImage imageWithSystemSymbolName:name accessibilityDescription:nativeString(description)];
  NSImageSymbolConfiguration* configuration =
      [NSImageSymbolConfiguration configurationWithPointSize:12 weight:NSFontWeightRegular];
  return image != nil ? [image imageWithSymbolConfiguration:configuration] : nil;
}

// A selection input's colour on its button, as the shared panel's swatch.
NSImage* swatchImage(const QColor& color) {
  NSColor* fill = [NSColor colorWithSRGBRed:color.redF() green:color.greenF() blue:color.blueF() alpha:color.alphaF()];
  return [NSImage imageWithSize:NSMakeSize(12, 12) flipped:NO drawingHandler:^BOOL(NSRect rect) {
    [fill setFill];
    [[NSBezierPath bezierPathWithRoundedRect:NSInsetRect(rect, 0.5, 0.5) xRadius:3 yRadius:3] fill];
    return YES;
  }];
}

// A choice tile's picture (InputDef::choiceTile) within 48 points.
NSImage* tileImage(const QImage& picture) {
  if (picture.isNull()) { return nil; }
  QByteArray data;
  QBuffer buffer(&data);
  buffer.open(QIODevice::WriteOnly);
  picture.save(&buffer, "PNG");
  NSImage* image = [[NSImage alloc] initWithData:[NSData dataWithBytes:data.constData()
                                                              length:static_cast<NSUInteger>(data.size())]];
  const QSize fitted = picture.size().scaled(48, 48, Qt::KeepAspectRatio);
  image.size = NSMakeSize(fitted.width(), fitted.height());
  return image;
}

NSStackView* column() {
  NSStackView* stack = [[NSStackView alloc] initWithFrame:NSZeroRect];
  stack.orientation = NSUserInterfaceLayoutOrientationVertical;
  stack.alignment = NSLayoutAttributeLeading;
  stack.spacing = 6;
  stack.detachesHiddenViews = YES;
  stack.translatesAutoresizingMaskIntoConstraints = NO;
  return stack;
}

NSStackView* line() {
  NSStackView* stack = [[NSStackView alloc] initWithFrame:NSZeroRect];
  stack.orientation = NSUserInterfaceLayoutOrientationHorizontal;
  stack.alignment = NSLayoutAttributeCenterY;
  stack.spacing = 6;
  stack.translatesAutoresizingMaskIntoConstraints = NO;
  return stack;
}

// What takes a line's free width: the controls before it keep their size.
NSView* spring() {
  NSView* view = [[NSView alloc] initWithFrame:NSZeroRect];
  view.translatesAutoresizingMaskIntoConstraints = NO;
  [view setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
  [view setContentCompressionResistancePriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
  return view;
}

void addFullWidth(NSStackView* parent, NSView* child) {
  child.translatesAutoresizingMaskIntoConstraints = NO;
  [parent addArrangedSubview:child];
  [child.leadingAnchor constraintEqualToAnchor:parent.leadingAnchor
                                     constant:parent.edgeInsets.left].active = YES;
  [child.trailingAnchor constraintEqualToAnchor:parent.trailingAnchor
                                      constant:-parent.edgeInsets.right].active = YES;
}

NSTextField* label(const QString& text, bool secondary = false) {
  NSTextField* field = [NSTextField wrappingLabelWithString:nativeString(text)];
  field.font = secondary ? [NSFont systemFontOfSize:[NSFont smallSystemFontSize]]
                         : [NSFont systemFontOfSize:[NSFont systemFontSize]];
  field.textColor = secondary ? NSColor.secondaryLabelColor : NSColor.labelColor;
  field.selectable = YES;
  [field setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                 forOrientation:NSLayoutConstraintOrientationHorizontal];
  return field;
}

// The caption beside an input, in the caption column (mitcad#815).
NSTextField* captionLabel(const QString& text, const QString& tooltip) {
  NSTextField* field = label(text);
  field.selectable = NO;
  field.toolTip = nativeString(tooltip);
  field.preferredMaxLayoutWidth = kCaptionWidth;
  field.translatesAutoresizingMaskIntoConstraints = NO;
  [field.widthAnchor constraintLessThanOrEqualToConstant:kCaptionWidth].active = YES;
  [field setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationHorizontal];
  [field setContentCompressionResistancePriority:NSLayoutPriorityDefaultHigh
                                 forOrientation:NSLayoutConstraintOrientationHorizontal];
  return field;
}

void setHidden(NSView* view, bool hidden) {
  if (view != nil && static_cast<bool>(view.hidden) != hidden) { view.hidden = hidden; }
}

void setHidden(NSGridRow* row, bool hidden) {
  if (row != nil && static_cast<bool>(row.hidden) != hidden) { row.hidden = hidden; }
}

// The inputs: captions in the first column beside their controls in the
// second, which takes the rest of the width, as the shared panel's
// QFormLayout (mitcad#815). A hidden row takes no space, and no row's
// constraints depend on another row's views.
NSGridView* formGrid() {
  NSGridView* grid = [NSGridView gridViewWithNumberOfColumns:2 rows:0];
  grid.translatesAutoresizingMaskIntoConstraints = NO;
  grid.rowSpacing = 0;
  grid.columnSpacing = 8;
  grid.yPlacement = NSGridCellPlacementCenter;
  [grid columnAtIndex:0].xPlacement = NSGridCellPlacementLeading;
  [grid columnAtIndex:1].xPlacement = NSGridCellPlacementFill;
  return grid;
}

// A caption (none: an empty cell) and its controls.
NSGridRow* gridRow(NSGridView* grid, NSView* caption, NSView* field, CGFloat top, CGFloat bottom) {
  field.translatesAutoresizingMaskIntoConstraints = NO;
  NSView* first = NSGridCell.emptyContentView;
  if (caption != nil) {
    caption.translatesAutoresizingMaskIntoConstraints = NO;
    first = caption;
  }
  NSGridRow* row = [grid addRowWithViews:@[first, field]];
  row.topPadding = top;
  row.bottomPadding = bottom;
  return row;
}

// A row across both columns, as the shared form's spanning rows.
NSGridRow* spanningRow(NSGridView* grid, NSView* view, CGFloat top, CGFloat bottom) {
  view.translatesAutoresizingMaskIntoConstraints = NO;
  NSGridRow* row = [grid addRowWithViews:@[view, NSGridCell.emptyContentView]];
  [row mergeCellsInRange:NSMakeRange(0, 2)];
  [row cellAtIndex:0].xPlacement = NSGridCellPlacementFill;
  row.topPadding = top;
  row.bottomPadding = bottom;
  return row;
}

QString displaySelection(const InputDef& input, int count) {
  if (count == 0) {
    // Match the shared form: an optional single pick is none, while an
    // empty optional list can stand for all of its candidates.
    return input.min > 0 ? QObject::tr("Select") : input.max == 1 ? QObject::tr("None") : QObject::tr("None (all)");
  }
  return QObject::tr("%1 selected").arg(count);
}

const QColor kSelectionColors[] = {QColor(0x1f, 0x7a, 0xff), QColor(0xff, 0x8c, 0x00),
                                    QColor(0x2c, 0xa0, 0x2c), QColor(0x9b, 0x30, 0xff)};

QColor selectionColor(int index) {
  return kSelectionColors[static_cast<std::size_t>(index) % std::size(kSelectionColors)];
}

bool sameNote(const InputNote& a, const InputNote& b) {
  return a.text == b.text && a.actions == b.actions && a.icons == b.icons && a.oneLine == b.oneLine;
}

} // namespace

class AppKitCommandForm final : public NativeCommandForm {
public:
  AppKitCommandForm(CommandSession& session, QWidget* parameterOwner)
      : m_session(&session), m_def(session.def()), m_state(session.state()),
        m_expressionHost(nativeExpressionHost(parameterOwner)) {
    m_logTimer.setSingleShot(true);
    m_logTimer.setInterval(0);
    QObject::connect(&m_logTimer, &QTimer::timeout, &session, [this] { logLayout(); });
    TestSync::watch(&m_logTimer);
    m_target = [MitcadCommandFormTarget new];
    m_target->form = this;
    m_root = column();
    m_root.edgeInsets = NSEdgeInsetsMake(12, 14, 8, 14);
    m_root.spacing = 10;
    m_root.accessibilityIdentifier = @"commandPanel";
    // The title row, as the shared panel's: the command's CAD artwork, its
    // name, then its commands (addHeaderAction).
    m_header = line();
    m_header.spacing = 8;
    m_header.distribution = NSStackViewDistributionFill;
    if (NSImage* artwork = iconImage(themeIcon(m_def.icon), 24, m_def.name)) {
      NSImageView* icon = [NSImageView imageViewWithImage:artwork];
      icon.translatesAutoresizingMaskIntoConstraints = NO;
      icon.accessibilityIdentifier = @"commandIcon";
      [icon.widthAnchor constraintEqualToConstant:24].active = YES;
      [icon.heightAnchor constraintEqualToConstant:24].active = YES;
      [m_header addArrangedSubview:icon];
    }
    NSTextField* title = label(m_def.name + (session.isEditing() ? QObject::tr(" (edit)") : QString()));
    title.font = [NSFont systemFontOfSize:[NSFont systemFontSize] weight:NSFontWeightSemibold];
    title.accessibilityIdentifier = @"commandTitle";
    [title setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
    [m_header addArrangedSubview:title];
    addFullWidth(m_root, m_header);
    m_fields = column();
    addFullWidth(m_root, m_fields);
    m_sections = column();
    m_sections.accessibilityIdentifier = @"commandSections";
    m_sections.hidden = YES;
    addFullWidth(m_root, m_sections);
    m_result = label(QString());
    m_result.accessibilityIdentifier = @"commandResult";
    m_result.hidden = YES;
    addFullWidth(m_root, m_result);
    m_message = label(QString(), true);
    m_message.accessibilityIdentifier = @"commandMessage";
    m_message.hidden = YES;
    addFullWidth(m_root, m_message);
    // OK and Cancel apart from the inputs (nativeFooterView): the window
    // keeps them in view under the inputs, which scroll above them.
    m_footer = line();
    m_footer.distribution = NSStackViewDistributionFill;
    m_footer.edgeInsets = NSEdgeInsetsMake(6, 14, 12, 14);
    m_footer.accessibilityIdentifier = @"commandButtons";
    [m_footer addArrangedSubview:spring()];
    // A command that repeats (BUILD's Place, mitcad#375) keeps what it
    // applied: its Cancel only closes it.
    NSButton* cancel = button(m_def.repeats ? QObject::tr("Close") : QObject::tr("Cancel"), QStringLiteral("cancel"));
    cancel.accessibilityIdentifier = @"commandCancel";
    // A command's Escape hook belongs to the document responder chain
    // (3D Print's bed face choice, mitcad#857). Its Cancel button remains
    // an unconditional rejection, including when activated with Space.
    cancel.keyEquivalent = m_def.escape ? @"" : @"\033";
    cancel.toolTip = nativeString(m_def.repeats ? QObject::tr("Close; what was placed stays (Esc)")
                                                : QObject::tr("Close without changes (Esc)"));
    m_ok = button(m_def.apply               ? (m_def.okLabel.isEmpty() ? QObject::tr("OK") : m_def.okLabel)
                  : m_def.inspect           ? QObject::tr("Close")
                  : m_def.okLabel.isEmpty() ? QObject::tr("OK")
                                            : m_def.okLabel,
                  QStringLiteral("accept"));
    m_ok.accessibilityIdentifier = @"commandOk";
    m_ok.keyEquivalent = @"\r";
    for (NSButton* control in @[cancel, m_ok]) {
      [control setContentHuggingPriority:NSLayoutPriorityDefaultHigh
                          forOrientation:NSLayoutConstraintOrientationHorizontal];
      [control.widthAnchor constraintGreaterThanOrEqualToConstant:64].active = YES;
      [m_footer addArrangedSubview:control];
      // A horizontal stack's optional centre alignment can discard its
      // vertical padding. Preserve the whole button frame within the
      // declared insets, including AppKit's alignment outsets (mitcad#993).
      const NSEdgeInsets alignment = control.alignmentRectInsets;
      [NSLayoutConstraint activateConstraints:@[
        [control.topAnchor constraintGreaterThanOrEqualToAnchor:m_footer.topAnchor
                                                       constant:m_footer.edgeInsets.top + alignment.top],
        [control.bottomAnchor constraintLessThanOrEqualToAnchor:m_footer.bottomAnchor
                                                      constant:-(m_footer.edgeInsets.bottom + alignment.bottom)]
      ]];
    }
    int colorIndex = 0;
    for (const InputDef& input : m_def.inputs) {
      if (input.type == InputDef::Type::Selection) {
        m_colors.insert(input.id, input.color.isValid() ? input.color : selectionColor(colorIndex));
        ++colorIndex;
      } else if (input.type == InputDef::Type::List) {
        m_listColors.insert(input.id, colorIndex++);
      }
    }
    rebuildFields();
    session.setPresenter(*this);
  }

  ~AppKitCommandForm() override {
    // AppKit's target and delegates are non-owning. Clear the controller's
    // back pointer before any retained view can send another event.
    m_target->form = nullptr;
    for (const Row& row : m_rows) {
      row.edit.delegate = nil;
    }
  }

  void* nativeView() const override { return (__bridge void*)m_root; }
  void* nativeFooterView() const override { return (__bridge void*)m_footer; }

  void addSection(QWidget* section) override {
    if (section == nullptr) return;
    Section added;
    // Keep the retained presenter an embedded widget, not a hidden top-level
    // window with window-sized Qt layout constraints. This host is never shown.
    section->setParent(m_sectionHost.get());
    section->setProperty("nativeCompactPanel", true);
    added.presenter.reset(section);
    added.view = createNativeWidgetView(section);
    addFullWidth(m_sections, (__bridge NSView*)added.view->nativeView());
    m_sections.hidden = NO;
    m_auxiliary.push_back(std::move(added));
    requestLayout();
  }

  void addHeaderAction(QAction* action) override {
    if (action == nullptr) {
      return;
    }
    // "command_solid.parameters": "parameters", as the shared header's.
    const QString id = action->objectName().section(QLatin1Char('.'), -1);
    const QString name = QString(action->text()).remove(QLatin1Char('&'));
    const QString kind = QStringLiteral("header|%1").arg(m_headerActions.size());
    NSImage* image = iconImage(action->icon(), 18, name);
    NSButton* control = image != nil ? iconButton(image, name, kind) : button(name, kind);
    control.accessibilityIdentifier = nativeString(QStringLiteral("commandHeader_") + id);
    control.accessibilityLabel = nativeString(name);
    control.toolTip = nativeString(action->toolTip().isEmpty() ? name : action->toolTip());
    control.enabled = action->isEnabled();
    [m_header addArrangedSubview:control];
    m_headerActions.push_back({QPointer<QAction>(action), control});
  }

  void refresh(const CommandState& state) override {
    m_state = state;
    bool structureChanged = false;
    for (const InputDef& input : m_def.inputs) {
      if (input.type == InputDef::Type::List && m_counts.value(input.id, -1) != state.rows(input.id)) {
        structureChanged = true;
      }
    }
    if (structureChanged) {
      rebuildFields();
    }
    for (Row& row : m_rows) {
      const CommandState scope = row.list.isEmpty() ? state : state.row(row.list, row.index);
      const InputDef& input = *row.input;
      bool visible = !input.visible || input.visible(scope);
      if (!row.list.isEmpty()) {
        const InputDef& owner = m_def.inputs[m_def.inputIndex(row.list)];
        visible = visible && (!owner.visible || owner.visible(state));
      }
      // A label that follows other inputs (a pattern's Spacing or Extent).
      if (row.caption != nil && input.labelFor) {
        const QString text = input.labelFor(scope);
        if (qtString(row.caption.stringValue) != text) {
          row.caption.stringValue = nativeString(text);
        }
      }
      switch (input.type) {
      case InputDef::Type::Value:
      case InputDef::Type::Text:
        if (qtString(row.edit.stringValue) != scope.text(input.id)) {
          row.edit.stringValue = nativeString(scope.text(input.id));
        }
        if (row.mark != nil) {
          refreshMark(row, input.markFor(scope));
        }
        // What the feature's own parameter is, as a sketch dimension's
        // tooltip says (mitcad#250).
        if (input.type == InputDef::Type::Value && !scope.parameterTooltip(input.id).isEmpty()) {
          row.edit.toolTip = nativeString(scope.parameterTooltip(input.id) + QLatin1Char('\n') +
                                          QObject::tr("Type name = value to rename it; a click on a dimension "
                                                      "or a value in the view inserts its name"));
        }
        break;
      case InputDef::Type::Choice:
        refreshChoices(row, scope);
        break;
      case InputDef::Type::Check:
      case InputDef::Type::Flip:
        row.check.state = scope.checked(input.id) ? NSControlStateValueOn : NSControlStateValueOff;
        if (input.disclosure) {
          row.check.accessibilityValue = nativeString(scope.checked(input.id) ? QObject::tr("Expanded")
                                                                              : QObject::tr("Collapsed"));
        }
        break;
      case InputDef::Type::Selection: {
        const Selection& items = scope.items(input.id);
        row.select.title = nativeString(displaySelection(input, static_cast<int>(items.size())));
        row.select.state = row.key == m_active ? NSControlStateValueOn : NSControlStateValueOff;
        row.select.accessibilityValue = nativeString(row.key == m_active ? QObject::tr("Active selection input")
                                                                        : QObject::tr("Inactive selection input"));
        row.clear.enabled = !items.isEmpty();
        if (row.order != nil) {
          refreshOrder(row, items);
        }
        break;
      }
      case InputDef::Type::CutTargets:
        row.targets->setScope(scope.object(input.id));
        break;
      case InputDef::Type::Note: {
        const InputNote note = input.noteFor ? input.noteFor(scope) : InputNote();
        visible = visible && !note.isEmpty();
        refreshNote(row, note);
        break;
      }
      case InputDef::Type::List:
        break;
      }
      setRowShown(row, visible);
      updateNote(row);
    }
    for (const HeaderAction& header : m_headerActions) {
      header.button.enabled = !header.action.isNull() && header.action->isEnabled();
    }
    for (const Section& section : m_auxiliary) section.view->refresh();
    refreshControlAvailability();
    requestLayout();
  }

  void setPreviewReport(const QJsonObject& report) override {
    for (Row& row : m_rows) {
      if (row.targets) {
        row.targets->setPreview(report.value(QStringLiteral("cut_targets")).toObject());
        row.targetsView->refresh();
      }
    }
  }

  void inputActionApplied(const QString& key) override {
    m_pendingStructure.remove(key);
    refreshControlAvailability();
  }

  bool inputEnabled(const QString& key) const override {
    for (auto pending = m_pendingStructure.cbegin(); pending != m_pendingStructure.cend(); ++pending) {
      if (key == pending.value() || key.startsWith(pending.value() + QLatin1Char('.'))) {
        return false;
      }
    }
    return true;
  }

  void setActiveSelection(const QString& key) override {
    m_active = key;
    for (Row& row : m_rows) {
      if (row.select != nil) {
        row.select.state = row.key == key ? NSControlStateValueOn : NSControlStateValueOff;
      }
    }
  }

  void setInputError(const QString& key, const QString& text) override {
    m_notes[key].first = text;
    if (Row* row = findRow(key)) {
      updateNote(*row);
      requestLayout();
    }
  }

  void setValueHint(const QString& key, const QString& text) override {
    m_notes[key].second = text;
    if (Row* row = findRow(key)) {
      updateNote(*row);
      requestLayout();
    }
  }

  // An empty value the model works out (InputDef::autoText, mitcad#656).
  void setPlaceholder(const QString& key, const QString& text) override {
    if (Row* row = findRow(key); row != nullptr && row->edit != nil) {
      row->edit.placeholderString = nativeString(text);
    }
  }

  void setMessage(const QString& text, bool error) override {
    m_message.stringValue = nativeString(text);
    m_message.textColor = error ? NSColor.systemRedColor : NSColor.secondaryLabelColor;
    m_message.hidden = text.isEmpty();
    requestLayout();
  }

  void setResult(const QString& text) override {
    // The command definitions return simple HTML. AppKit labels preserve
    // readable paragraphs without routing the result through a Qt widget.
    QTextDocument document;
    document.setHtml(text);
    m_result.stringValue = nativeString(document.toPlainText());
    m_result.hidden = text.isEmpty();
    requestLayout();
  }

  void setOkEnabled(bool enabled) override { m_ok.enabled = enabled; }

  // What the controls show, for live MCP (mitcad#265).
  QJsonObject shownState() const override {
    QJsonObject notes;
    for (auto it = m_notes.cbegin(); it != m_notes.cend(); ++it) {
      if (!it.value().first.isEmpty()) {
        notes.insert(it.key(), QJsonObject{{QStringLiteral("error"), it.value().first}});
      } else if (!it.value().second.isEmpty()) {
        notes.insert(it.key(), QJsonObject{{QStringLiteral("hint"), it.value().second}});
      }
    }
    return {{QStringLiteral("message"), m_message.hidden ? QString() : qtString(m_message.stringValue)},
            {QStringLiteral("result"), m_result.hidden ? QString() : qtString(m_result.stringValue)},
            {QStringLiteral("ok_enabled"), static_cast<bool>(m_ok.enabled)},
            {QStringLiteral("notes"), notes}};
  }

  QColor colorOf(const QString& key) const override {
    if (m_colors.contains(key)) {
      return m_colors.value(key);
    }
    const QStringList parts = key.split(QLatin1Char('.'));
    if (parts.size() >= 3 && m_listColors.contains(parts[0])) {
      if (const Row* row = findRow(key); row != nullptr && row->input->color.isValid()) {
        return row->input->color;
      }
      return selectionColor(m_listColors.value(parts[0]) + parts[1].toInt());
    }
    return selectionColor(0);
  }

  bool focusFirstValue() override {
    for (const Row& row : m_rows) {
      if (row.input->type == InputDef::Type::Value && shown(row)) {
        return focusValue(row.key);
      }
    }
    return false;
  }

  bool focusValue(const QString& key) override {
    const Row* row = findRow(key);
    if (row == nullptr || row->edit == nil || !shown(*row) || m_root.window == nil) {
      return false;
    }
    [m_root.window makeFirstResponder:row->edit];
    [row->edit selectText:nil];
    editing(key, true);
    return true;
  }

  bool insertIntoValue(const QString& key, const QString& text) override {
    const Row* row = findRow(key);
    if (row == nullptr || row->edit == nil || !shown(*row) || m_root.window == nil) {
      return false;
    }
    // The field takes the keyboard back; the name goes at the end of its
    // text (AppKit selects all of it when it takes the keyboard).
    NSTextField* field = row->edit;
    [m_root.window makeFirstResponder:field];
    NSText* editor = field.currentEditor;
    if (editor == nil) {
      return false;
    }
    editor.selectedRange = NSMakeRange(editor.string.length, 0);
    [editor replaceCharactersInRange:editor.selectedRange withString:nativeString(text)];
    editing(key, true);
    qDebug().noquote() << QStringLiteral("Panel %1 input %2: inserted %3, now %4")
                              .arg(m_def.name, key, text, qtString(field.stringValue));
    textChanged(field);
    return true;
  }

  void logLayout() const override {
    [m_root layoutSubtreeIfNeeded];
    for (const Row& row : m_rows) {
      NSView* control = row.edit != nil ? static_cast<NSView*>(row.edit)
                          : row.select != nil ? static_cast<NSView*>(row.select)
                          : row.popup != nil ? static_cast<NSView*>(row.popup)
                          : row.check != nil ? static_cast<NSView*>(row.check)
                          : row.tile != nil ? static_cast<NSView*>(row.tile)
                                             : static_cast<NSView*>(row.box);
      const NSPoint at = [control convertPoint:NSMakePoint(NSMidX(control.bounds), NSMidY(control.bounds))
                                        toView:nil];
      NSView* content = m_root.window.contentView;
      const int x = static_cast<int>(at.x);
      const int y = static_cast<int>(content.bounds.size.height - at.y);
      qDebug().noquote() << QStringLiteral("Panel %1 input %2 at %3,%4%5")
                                .arg(m_def.name, row.key).arg(x).arg(y)
                                .arg(shown(row) ? QString() : QStringLiteral(" (hidden)"));
      if (row.mark != nil && !row.mark.hidden && shown(row)) {
        // A value field's mark (mitcad#980).
        const NSPoint point = [row.mark convertPoint:NSMakePoint(NSMidX(row.mark.bounds), NSMidY(row.mark.bounds))
                                              toView:nil];
        qDebug().noquote() << QStringLiteral("Panel %1 mark %2 at %3,%4")
                                  .arg(m_def.name, row.key)
                                  .arg(static_cast<int>(point.x))
                                  .arg(static_cast<int>(content.bounds.size.height - point.y));
      }
      for (NSButton* button in row.choiceButtons) {
        const NSPoint point = [button convertPoint:NSMakePoint(NSMidX(button.bounds), NSMidY(button.bounds))
                                            toView:nil];
        qDebug().noquote() << QStringLiteral("Panel %1 input %2=%3 at %4,%5%6")
                                  .arg(m_def.name, row.key, qtString(button.identifier).section(QLatin1Char('|'), 2))
                                  .arg(static_cast<int>(point.x))
                                  .arg(static_cast<int>(content.bounds.size.height - point.y))
                                  .arg(shown(row) ? QString() : QStringLiteral(" (hidden)"));
      }
    }
  }

  void action(NSControl* sender) {
    // Copy the callback before invoking it: accepting a command may cause
    // the controller to remove the form and destroy its session.
    const QStringList parts = qtString(sender.identifier).split(QLatin1Char('|'));
    if (parts.isEmpty()) {
      return;
    }
    const QString key = parts.value(1);
    const QString& kind = parts[0];
    if (kind == QStringLiteral("header")) {
      // A command of the title row (Change Parameters, mitcad#564).
      const int index = key.toInt();
      if (index >= 0 && index < static_cast<int>(m_headerActions.size())) {
        const QPointer<QAction> header = m_headerActions[static_cast<std::size_t>(index)].action;
        if (!header.isNull() && header->isEnabled()) {
          header->trigger();
        }
      }
      return;
    }
    if (!key.isEmpty() && !inputEnabled(key)) { return; }
    const bool structural = kind == QStringLiteral("add") || kind == QStringLiteral("removeRow") ||
        kind == QStringLiteral("move") || kind == QStringLiteral("removeItem") ||
        kind == QStringLiteral("clear");
    if (structural) {
      if (![sender.window makeFirstResponder:nil]) { return; }
      const Row* row = findRow(key);
      const QString group = row && !row->list.isEmpty() ? row->list : key;
      m_pendingStructure.insert(key, group);
      refreshControlAvailability();
    }
    if (kind == QStringLiteral("accept")) {
      auto call = actions().accepted;
      if (call) { call(); }
    } else if (kind == QStringLiteral("cancel")) {
      auto call = actions().rejected;
      if (call) { call(); }
    } else if (kind == QStringLiteral("select")) {
      auto call = actions().selectionActivated;
      if (call) { call(key); }
    } else if (kind == QStringLiteral("clear")) {
      auto call = actions().selectionCleared;
      if (call) { call(key); }
    } else if (kind == QStringLiteral("iconChoice")) {
      const QString value = parts.value(2);
      auto call = actions().choiceChanged;
      if (call) { call(key, value); }
    } else if (kind == QStringLiteral("choice")) {
      NSPopUpButton* popup = static_cast<NSPopUpButton*>(sender);
      const QString value = qtString(static_cast<NSString*>(popup.selectedItem.representedObject));
      auto call = actions().choiceChanged;
      if (call) { call(key, value); }
    } else if (kind == QStringLiteral("check")) {
      const bool checked = static_cast<NSButton*>(sender).state == NSControlStateValueOn;
      auto call = actions().checkChanged;
      if (call) { call(key, checked); }
    } else if (kind == QStringLiteral("button")) {
      // The button beside a choice (InputDef::onButton: 3D Print's Edit).
      auto call = actions().buttonClicked;
      if (call) { call(key); }
    } else if (kind == QStringLiteral("add")) {
      auto call = actions().rowAdded;
      if (call) { call(key); }
    } else if (kind == QStringLiteral("removeRow")) {
      auto call = actions().rowRemoved;
      if (call) { call(key, parts.value(2).toInt()); }
    } else if (kind == QStringLiteral("removeItem")) {
      auto call = actions().itemRemoved;
      if (call) { call(key, parts.value(2).toInt()); }
    } else if (kind == QStringLiteral("move")) {
      auto call = actions().itemMoved;
      if (call) { call(key, parts.value(2).toInt(), parts.value(3).toInt()); }
    } else if (kind == QStringLiteral("note")) {
      auto call = actions().noteAction;
      if (call) { call(key, parts.value(2).toInt()); }
    } else if (kind == QStringLiteral("mark")) {
      // A value field's mark (InputDef::onMark, mitcad#980: Reset to size).
      auto call = actions().markClicked;
      if (call) { call(key); }
    } else if (kind == QStringLiteral("tile")) {
      // The choice tile's button (InputDef::onButton): BUILD's part picker.
      auto call = actions().buttonClicked;
      if (call) { call(key); }
    }
  }

  void textChanged(NSTextField* field) {
    if (!inputEnabled(qtString(field.identifier))) { return; }
    // AppKit tells of a field's editing as it is typed in: the first edit
    // is when it took the keyboard (mitcad#250).
    if (m_editingKey != qtString(field.identifier)) { editing(qtString(field.identifier), true); }
    auto call = actions().textEdited;
    if (call) { call(qtString(field.identifier), qtString(field.stringValue)); }
  }

  void valueCommitted(NSTextField* field) {
    if (!inputEnabled(qtString(field.identifier))) { return; }
    auto call = actions().valueCommitted;
    if (call) { call(qtString(field.identifier)); }
    if (m_editingKey == qtString(field.identifier)) { editing(m_editingKey, false); }
  }

  // A value field took the keyboard or lost it (mitcad#250).
  void editing(QString key, bool focused) {
    m_editingKey = focused ? key : QString();
    auto call = actions().valueFocused;
    if (call) { call(key, focused); }
  }

  bool textCommand(NSTextField* field, SEL command) {
    if (command == @selector(insertNewline:)) {
      valueCommitted(field);
      auto call = actions().accepted;
      if (call) { call(); }
      return true;
    }
    if (command == @selector(cancelOperation:)) {
      if (m_session && m_session->escape()) { return true; }
      auto call = actions().rejected;
      if (call) { call(); }
      return true;
    }
    return false;
  }

  // AppKit's completion in a value field (F5 or Option-Esc): the
  // parameters the field can take; nil for a field without expressions.
  NSArray<NSString*>* completions(NSTextField* field, NSTextView* text, NSRange range) {
    for (const Row& row : m_rows) {
      if (row.edit == field && row.expression) {
        return row.expression->completions(qtString(text.string), static_cast<int>(NSMaxRange(range)));
      }
    }
    return nil;
  }

private:
  void requestLayout() {
    [m_root layoutSubtreeIfNeeded];
    // The enclosing card measures the form and applies its height cap.
    // Messages and results can change that height after an input refresh.
    m_root.window.contentView.needsLayout = YES;
    m_logTimer.start();
  }

  struct Row {
    const InputDef* input = nullptr;
    QString key;
    QString list;
    int index = -1;
    // The input's controls in the second column (row_<key>), or across
    // both; the grid rows of its heading, controls and note.
    NSView* __strong box = nil;
    NSTextField* __strong caption = nil;
    NSTextField* __strong heading = nil;
    NSGridRow* __strong headingRow = nil;
    NSGridRow* __strong mainRow = nil;
    NSGridRow* __strong noteRow = nil;
    NSTextField* __strong note = nil;
    NSTextField* __strong edit = nil;
    NSPopUpButton* __strong popup = nil;
    NSButton* __strong button = nil; // beside a choice (InputDef::onButton)
    NSMutableArray<NSButton*>* __strong choiceButtons = nil;
    NSButton* __strong check = nil;
    NSButton* __strong select = nil;
    NSButton* __strong clear = nil;
    NSStackView* __strong order = nil;
    QStringList orderDescriptions;
    std::shared_ptr<CutTargetsPanel> targets;
    // A note row (InputDef::Type::Note, mitcad#655): what it shows, made
    // again when its text or buttons change.
    NSStackView* __strong noteContent = nil;
    InputNote shownNote;
    bool noteBuilt = false;
    std::shared_ptr<NativeWidgetView> targetsView;
    // A value field's fx tools (mitcad#815).
    std::shared_ptr<NativeExpressionField> expression;
    // A value field's mark (InputDef::markFor, mitcad#980) beside it, and
    // the mark it shows.
    NSButton* __strong mark = nil;
    FieldMark shownMark;
    // A choice shown as a tile (InputDef::choiceTile with onButton,
    // mitcad#819): the chosen option's picture, name and detail.
    NSButton* __strong tile = nil;
    QString tileText;
    qint64 tilePicture = 0;
  };

  struct HeaderAction {
    QPointer<QAction> action;
    NSButton* __strong button = nil;
  };

  NSButton* button(const QString& text, const QString& action) {
    NSButton* control = [NSButton buttonWithTitle:nativeString(text)
                                           target:m_target
                                           action:@selector(performAction:)];
    control.identifier = nativeString(action);
    control.accessibilityIdentifier = nativeString(QStringLiteral("native.action.") + action);
    control.bezelStyle = NSBezelStyleRounded;
    return control;
  }

  // A compact borderless control with an image only (the shared panel's
  // small tool buttons: clear, remove, edit), its text the tooltip.
  NSButton* iconButton(NSImage* image, const QString& tooltip, const QString& action) {
    NSButton* control = button(QString(), action);
    control.bordered = NO;
    control.image = image;
    control.imagePosition = NSImageOnly;
    control.imageScaling = NSImageScaleProportionallyDown;
    control.toolTip = nativeString(tooltip);
    control.accessibilityLabel = nativeString(tooltip);
    control.translatesAutoresizingMaskIntoConstraints = NO;
    [control.widthAnchor constraintEqualToConstant:22].active = YES;
    [control.heightAnchor constraintEqualToConstant:22].active = YES;
    return control;
  }

  // A link in a note (linkActionIcon(), mitcad#655): underlined in the link
  // colour, elided rather than wider than the card.
  NSButton* linkButton(const QString& text, const QString& action) {
    NSButton* control = button(text, action);
    control.bordered = NO;
    control.attributedTitle = [[NSAttributedString alloc]
        initWithString:nativeString(text)
            attributes:@{NSForegroundColorAttributeName : NSColor.linkColor,
                         NSUnderlineStyleAttributeName : @(NSUnderlineStyleSingle),
                         NSFontAttributeName : [NSFont systemFontOfSize:[NSFont systemFontSize]]}];
    control.lineBreakMode = NSLineBreakByTruncatingTail;
    control.toolTip = nativeString(text);
    control.accessibilityLabel = nativeString(text);
    [control setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                      forOrientation:NSLayoutConstraintOrientationHorizontal];
    return control;
  }

  void refreshControlAvailability() {
    for (const Row& row : m_rows) {
      // A list owner's view contains all its row fields and actions.
      // Visiting children separately could undo their owner's lock.
      if (row.list.isEmpty() && !row.targets) {
        // Rows a state disables (mitcad#655: 3D Print's Printer with STL).
        const bool enabled = !row.input->enabled || row.input->enabled(m_state);
        setControlAvailability(row.box, inputEnabled(row.key) && enabled);
      }
    }
  }

  void setControlAvailability(NSView* view, bool groupEnabled) {
    if (view == nil) {
      return;
    }
    if ([view isKindOfClass:[NSControl class]]) {
      NSControl* control = static_cast<NSControl*>(view);
      const QStringList parts = qtString(control.identifier).split(QLatin1Char('|'));
      const QString kind = parts.value(0);
      const QString key = parts.value(1);
      bool enabled = groupEnabled;
      if (kind == QStringLiteral("clear")) {
        enabled = enabled && !m_state.items(key).isEmpty();
      } else if (kind == QStringLiteral("removeRow")) {
        const int index = m_def.inputIndex(key);
        enabled = enabled && index >= 0 && m_state.rows(key) > m_def.inputs[index].minRows;
      } else if (kind == QStringLiteral("move") || kind == QStringLiteral("removeItem")) {
        const int count = m_state.items(key).size();
        const int from = parts.value(2).toInt();
        enabled = enabled && from >= 0 && from < count;
        if (kind == QStringLiteral("move")) {
          const int to = parts.value(3).toInt();
          enabled = enabled && to >= 0 && to < count;
        }
      } else if (kind == QStringLiteral("button") || kind == QStringLiteral("tile")) {
        // Edit on Slicer default has nothing to edit (mitcad#655).
        const Row* owner = findRow(key);
        enabled = enabled && owner != nullptr &&
                  (!owner->input->buttonEnabled || owner->input->buttonEnabled(m_state));
      }
      control.enabled = enabled;
    }
    for (NSView* child in view.subviews) {
      setControlAvailability(child, groupEnabled);
    }
  }

  void rebuildFields() {
    // A row-count change may remove an active field editor. End editing
    // before removing its view, then restore the same key when it survives.
    QString focused;
    for (const Row& row : m_rows) {
      if (row.edit != nil && row.edit.currentEditor != nil) {
        focused = row.key;
        row.edit.delegate = nil;
      }
    }
    if (!focused.isEmpty()) {
      [m_root.window makeFirstResponder:nil];
    }
    m_rows.clear();
    m_counts.clear();
    if (m_grid != nil) {
      [m_fields removeArrangedSubview:m_grid];
      [m_grid removeFromSuperview];
    }
    m_grid = formGrid();
    m_grid.accessibilityIdentifier = @"commandInputs";
    for (const InputDef& input : m_def.inputs) {
      if (input.type == InputDef::Type::List) {
        createList(input);
      } else {
        createField(input, input.id, QString(), -1, m_grid);
      }
    }
    addFullWidth(m_fields, m_grid);
    if (!focused.isEmpty()) {
      focusValue(focused);
    }
  }

  // A list's rows (fillet sets, hole positions) take the whole width under
  // its name; each row's inputs are a form of their own.
  void createList(const InputDef& input) {
    Row owner;
    owner.input = &input;
    owner.key = input.id;
    owner.heading = label(input.label);
    owner.heading.font = [NSFont systemFontOfSize:[NSFont systemFontSize] weight:NSFontWeightSemibold];
    owner.heading.toolTip = nativeString(input.tooltip);
    owner.headingRow = spanningRow(m_grid, owner.heading, 8, 2);
    NSStackView* groups = column();
    groups.spacing = 6;
    groups.accessibilityIdentifier = nativeString(QStringLiteral("row_") + input.id);
    owner.box = groups;
    owner.mainRow = spanningRow(m_grid, groups, 0, 3);
    m_rows.push_back(owner);
    const int count = m_state.rows(input.id);
    m_counts.insert(input.id, count);
    for (int index = 0; index < count; ++index) {
      NSStackView* group = column();
      group.spacing = 2;
      group.edgeInsets = NSEdgeInsetsMake(0, 8, 4, 0);
      NSStackView* header = line();
      header.distribution = NSStackViewDistributionFill;
      NSTextField* name = label(input.rowLabel.arg(index + 1), true);
      [name setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
      [header addArrangedSubview:name];
      NSButton* remove = iconButton(symbolImage(@"xmark", QObject::tr("Remove")),
                                    QObject::tr("Remove %1").arg(input.rowLabel.arg(index + 1)),
                                    QStringLiteral("removeRow|%1|%2").arg(input.id).arg(index));
      remove.enabled = count > input.minRows;
      [header addArrangedSubview:remove];
      addFullWidth(group, header);
      NSGridView* grid = formGrid();
      for (const InputDef& child : input.children) {
        createField(child, CommandState::rowKey(input.id, index, child.id), input.id, index, grid);
      }
      addFullWidth(group, grid);
      addFullWidth(groups, group);
    }
    if (!input.addLabel.isEmpty()) {
      NSButton* add = button(input.addLabel, QStringLiteral("add|") + input.id);
      add.accessibilityIdentifier = nativeString(QStringLiteral("add_") + input.id);
      add.image = symbolImage(@"plus", input.addLabel);
      add.imagePosition = NSImageLeading;
      [groups addArrangedSubview:add];
    }
  }

  // What the value field `key` is for its expression tools, as the shared
  // panel attaches them (CommandPanel::createRow).
  ParameterFieldOptions expressionOptions(const QString& key) const {
    ParameterFieldOptions options;
    const Row* row = findRow(key);
    if (row == nullptr) {
      return options;
    }
    const InputDef& input = *row->input;
    options.kind = input.valueKind == ValueKind::Angle      ? ParameterKind::Angle
                   : input.valueKind == ValueKind::Unitless ? ParameterKind::Unitless
                                                            : ParameterKind::Length;
    const CommandState scope = row->list.isEmpty() ? m_state : m_state.row(row->list, row->index);
    options.own = scope.parameterName(input.id);
    options.label = QStringLiteral("%1 %2").arg(m_def.name, input.label);
    options.naming = true;
    return options;
  }

  void createField(const InputDef& input, const QString& key, const QString& list, int index, NSGridView* grid) {
    Row row;
    row.input = &input;
    row.key = key;
    row.list = list;
    row.index = index;
    if (!input.heading.isEmpty()) {
      // The heading of a group of inputs, a row of its own above, hidden
      // with the input.
      row.heading = label(input.heading);
      row.heading.font = [NSFont boldSystemFontOfSize:[NSFont systemFontSize]];
      row.headingRow = spanningRow(grid, row.heading, 8, 2);
    }
    // Across both columns, as the shared form's spanning rows: a cut scope
    // under its name, a note without a label of its own.
    bool spanning = false;
    NSView* field = nil;
    switch (input.type) {
    case InputDef::Type::Value:
    case InputDef::Type::Text: {
      row.edit = [[NSTextField alloc] initWithFrame:NSZeroRect];
      row.edit.editable = YES;
      row.edit.selectable = YES;
      row.edit.bezeled = YES;
      row.edit.bezelStyle = NSTextFieldRoundedBezel;
      row.edit.font = [NSFont systemFontOfSize:[NSFont systemFontSize]];
      row.edit.identifier = nativeString(key);
      row.edit.accessibilityIdentifier = nativeString(QStringLiteral("input_") + key);
      row.edit.accessibilityLabel = nativeString(input.label);
      row.edit.delegate = m_target;
      row.edit.toolTip = nativeString(input.tooltip.isEmpty() && input.type == InputDef::Type::Value
                                         ? QObject::tr("A value or an expression, such as 20, 2 in or d1 * 2")
                                         : input.tooltip);
      if (input.type == InputDef::Type::Value && input.autoText) {
        // Empty: the model works it out (mitcad#656).
        row.edit.placeholderString = nativeString(QObject::tr("Auto"));
      }
      [row.edit setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                         forOrientation:NSLayoutConstraintOrientationHorizontal];
      [row.edit setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
      NSStackView* controls = line();
      controls.spacing = 2;
      controls.distribution = NSStackViewDistributionFill;
      [controls addArrangedSubview:row.edit];
      if (input.type == InputDef::Type::Value && input.markFor) {
        // A mark beside the field (mitcad#980: a Hole field that no longer
        // follows the screw size), as the shared panel's in it: hidden until
        // refreshMark shows one; a click does its action.
        row.mark = iconButton(nil, QString(), QStringLiteral("mark|") + key);
        row.mark.accessibilityIdentifier = nativeString(QStringLiteral("mark_") + key);
        row.mark.hidden = YES;
        [controls addArrangedSubview:row.mark];
      }
      if (input.type == InputDef::Type::Value && !m_expressionHost.isNull()) {
        // The shared expression tools beside the field (mitcad#815).
        row.expression = std::make_shared<NativeExpressionField>(m_expressionHost.data(), row.edit,
                                                                 [this, key] { return expressionOptions(key); });
        NSButton* fx = row.expression->button();
        fx.identifier = nativeString(QStringLiteral("fx|") + key);
        fx.accessibilityIdentifier = nativeString(QStringLiteral("fx_") + key);
        [controls addArrangedSubview:fx];
      }
      field = controls;
      break;
    }
    case InputDef::Type::Choice: {
      NSStackView* controls = line();
      controls.spacing = 4;
      controls.distribution = NSStackViewDistributionFill;
      if (input.choiceTile && input.onButton) {
        // A tile, as the shared panel shows it: a click on it (or Space)
        // does what its button does, here BUILD's part picker (mitcad#819).
        row.tile = button(QString(), QStringLiteral("tile|") + key);
        row.tile.bezelStyle = NSBezelStyleSmallSquare;
        row.tile.alignment = NSTextAlignmentLeft;
        row.tile.imagePosition = NSImageLeading;
        row.tile.imageScaling = NSImageScaleProportionallyDown;
        row.tile.imageHugsTitle = YES;
        row.tile.cell.wraps = YES;
        row.tile.cell.lineBreakMode = NSLineBreakByWordWrapping;
        row.tile.toolTip = nativeString(input.buttonTooltip.isEmpty() ? input.tooltip : input.buttonTooltip);
        row.tile.accessibilityIdentifier = nativeString(QStringLiteral("input_") + key);
        row.tile.accessibilityLabel = nativeString(input.label);
        [row.tile.heightAnchor constraintGreaterThanOrEqualToConstant:56].active = YES;
        // Its name wraps within the field column rather than widening the card.
        [row.tile setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                           forOrientation:NSLayoutConstraintOrientationHorizontal];
        [row.tile setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
        // In a line of its own: the row's identifier is the line's, the
        // tile keeps the input's (input_<key>).
        [controls addArrangedSubview:row.tile];
        field = controls;
        break;
      }
      if (!input.choiceIcons.isEmpty() && !input.choicesFor) {
        row.choiceButtons = [NSMutableArray array];
        for (const auto& option : input.choices) {
          NSButton* choice = button(QString(), QStringLiteral("iconChoice|%1|%2").arg(key, option.first));
          [choice setButtonType:NSButtonTypePushOnPushOff];
          choice.bezelStyle = NSBezelStyleSmallSquare;
          choice.image = choiceImage(input.choiceIcons.value(option.first), option.second);
          choice.imagePosition = choice.image == nil ? NSNoImage : NSImageOnly;
          if (choice.image == nil) { choice.title = nativeString(option.second); }
          choice.accessibilityIdentifier = nativeString(QStringLiteral("input_%1_%2").arg(key, option.first));
          choice.accessibilityLabel = nativeString(option.second);
          choice.toolTip = nativeString(input.tooltip.isEmpty() ? option.second
                                      : QStringLiteral("%1: %2").arg(option.second, input.tooltip));
          [choice.widthAnchor constraintGreaterThanOrEqualToConstant:32].active = YES;
          [choice.heightAnchor constraintGreaterThanOrEqualToConstant:28].active = YES;
          [choice setContentHuggingPriority:NSLayoutPriorityDefaultHigh
                             forOrientation:NSLayoutConstraintOrientationHorizontal];
          [controls addArrangedSubview:choice];
          [row.choiceButtons addObject:choice];
        }
        [controls addArrangedSubview:spring()];
        field = controls;
        break;
      }
      row.popup = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
      row.popup.identifier = nativeString(QStringLiteral("choice|") + key);
      row.popup.accessibilityIdentifier = nativeString(QStringLiteral("input_") + key);
      row.popup.accessibilityLabel = nativeString(input.label);
      row.popup.toolTip = nativeString(input.tooltip);
      row.popup.target = m_target;
      row.popup.action = @selector(performAction:);
      // A long option is elided, never wider than the card (mitcad#655).
      [row.popup setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                          forOrientation:NSLayoutConstraintOrientationHorizontal];
      [row.popup setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
      [controls addArrangedSubview:row.popup];
      if (input.onButton) {
        // The button beside the drop-down (3D Print's Edit, mitcad#655): its
        // small icon, its text the accessible name.
        NSImage* image = input.buttonIcon.isEmpty() ? nil : iconImage(themeIcon(input.buttonIcon), 16, input.buttonText);
        const QString kind = QStringLiteral("button|") + key;
        row.button = image != nil ? iconButton(image, input.buttonText, kind) : button(input.buttonText, kind);
        row.button.accessibilityIdentifier = nativeString(QStringLiteral("button_") + key);
        row.button.accessibilityLabel = nativeString(input.buttonText);
        if (!input.buttonTooltip.isEmpty()) {
          row.button.toolTip = nativeString(input.buttonTooltip);
        }
        [row.button setContentHuggingPriority:NSLayoutPriorityDefaultHigh
                               forOrientation:NSLayoutConstraintOrientationHorizontal];
        [controls addArrangedSubview:row.button];
      }
      field = controls;
      break;
    }
    case InputDef::Type::Check:
    case InputDef::Type::Flip: {
      NSStackView* controls = line();
      controls.distribution = NSStackViewDistributionFill;
      row.check = button(QString(), QStringLiteral("check|") + key);
      if (input.disclosure) {
        // Never initialise this cell as a switch: AppKit keeps the switch's
        // checkbox artwork when only its type and bezel style change later.
        [row.check setButtonType:NSButtonTypePushOnPushOff];
        row.check.bezelStyle = NSBezelStyleDisclosure;
        row.check.image = nil;
        row.check.alternateImage = nil;
        row.check.title = @"";
        row.check.toolTip = nativeString(input.tooltip);
        [controls addArrangedSubview:row.check];
        [controls addArrangedSubview:label(input.label)];
      } else if (input.type == InputDef::Type::Flip) {
        // A compact toggle with the shared flip artwork, as the shared
        // panel's, not a text button (mitcad#815).
        [row.check setButtonType:NSButtonTypePushOnPushOff];
        row.check.bezelStyle = NSBezelStyleSmallSquare;
        NSImage* image = iconImage(themeIcon(QStringLiteral("flip")), 20, input.label);
        row.check.image = image != nil ? image : symbolImage(@"arrow.up.arrow.down", input.label);
        row.check.imagePosition = NSImageOnly;
        row.check.toolTip = nativeString(input.tooltip.isEmpty() ? QObject::tr("Flip the direction") : input.tooltip);
        [row.check.widthAnchor constraintGreaterThanOrEqualToConstant:32].active = YES;
        [row.check.heightAnchor constraintGreaterThanOrEqualToConstant:28].active = YES;
        [controls addArrangedSubview:row.check];
      } else {
        // The caption names it; the box alone takes the click, as the
        // shared panel's check box.
        [row.check setButtonType:NSButtonTypeSwitch];
        row.check.title = @"";
        row.check.toolTip = nativeString(input.tooltip);
        [controls addArrangedSubview:row.check];
      }
      row.check.accessibilityIdentifier = nativeString(QStringLiteral("input_") + key);
      row.check.accessibilityLabel = nativeString(input.label);
      [row.check setContentHuggingPriority:NSLayoutPriorityDefaultHigh
                            forOrientation:NSLayoutConstraintOrientationHorizontal];
      [controls addArrangedSubview:spring()];
      field = controls;
      break;
    }
    case InputDef::Type::Selection: {
      NSStackView* selection = line();
      selection.spacing = 2;
      selection.distribution = NSStackViewDistributionFill;
      row.select = button(QString(), QStringLiteral("select|") + key);
      [row.select setButtonType:NSButtonTypePushOnPushOff];
      row.select.image = swatchImage(colorOf(key));
      row.select.imagePosition = NSImageLeading;
      row.select.toolTip = nativeString(input.tooltip.isEmpty() ? QObject::tr("Click to pick %1").arg(input.label)
                                                               : input.tooltip);
      row.select.accessibilityIdentifier = nativeString(QStringLiteral("input_") + key);
      row.select.accessibilityLabel = nativeString(input.label);
      [row.select setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                           forOrientation:NSLayoutConstraintOrientationHorizontal];
      [row.select setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
      [selection addArrangedSubview:row.select];
      // A compact clear control, as the shared panel's ✕ (mitcad#815).
      row.clear = iconButton(symbolImage(@"xmark", QObject::tr("Clear")), QObject::tr("Clear the selection"),
                             QStringLiteral("clear|") + key);
      row.clear.accessibilityLabel = nativeString(QObject::tr("Clear %1").arg(input.label));
      [selection addArrangedSubview:row.clear];
      field = selection;
      if (input.ordered) {
        NSStackView* ordered = column();
        ordered.spacing = 4;
        addFullWidth(ordered, selection);
        row.order = column();
        row.order.spacing = 4;
        addFullWidth(ordered, row.order);
        field = ordered;
      }
      break;
    }
    case InputDef::Type::CutTargets: {
      row.targets = std::make_shared<CutTargetsPanel>();
      row.targets->setObjectName(QStringLiteral("input_") + key);
      row.targets->setScope(m_state.object(key));
      QObject::connect(row.targets.get(), &CutTargetsPanel::scopeChanged, row.targets.get(),
                       [this, key](const QJsonObject& value) {
        auto call = actions().objectChanged;
        if (call) { call(key, value); }
      });
      QObject::connect(row.targets.get(), &CutTargetsPanel::highlightChanged, row.targets.get(),
                       [this](const Selection& items) {
        auto call = actions().highlightRequested;
        if (call) { call(items); }
      });
      row.targetsView = createNativeWidgetView(row.targets.get());
      // Under its name in bold, as the shared panel's cut scope.
      if (row.heading == nil) {
        row.heading = label(input.label);
        row.heading.font = [NSFont boldSystemFontOfSize:[NSFont systemFontSize]];
        row.heading.toolTip = nativeString(input.tooltip);
        row.headingRow = spanningRow(grid, row.heading, 6, 2);
      }
      NSStackView* box = column();
      addFullWidth(box, (__bridge NSView*)row.targetsView->nativeView());
      field = box;
      spanning = true;
      break;
    }
    case InputDef::Type::List:
      // Declarative lists contain fields; nesting is not supported by the
      // shared CommandState row representation either.
      return;
    case InputDef::Type::Note:
      // A line of text and its buttons, made by refresh (mitcad#655).
      row.noteContent = column();
      row.noteContent.spacing = 2;
      field = row.noteContent;
      spanning = input.label.isEmpty() && !input.fieldColumn;
      break;
    }
    if (field == nil) {
      return;
    }
    row.box = field;
    row.box.accessibilityIdentifier = nativeString(QStringLiteral("row_") + key);
    if (spanning) {
      row.mainRow = spanningRow(grid, field, 3, 3);
    } else {
      // A disclosure carries its label itself; a note in the field column
      // (InputDef::fieldColumn) has none.
      if (!input.disclosure && !input.label.isEmpty()) {
        row.caption = captionLabel(input.label, input.tooltip);
        row.caption.accessibilityIdentifier = nativeString(QStringLiteral("caption_") + key);
      }
      row.mainRow = gridRow(grid, row.caption, field, 3, 3);
    }
    row.note = label(QString(), true);
    row.note.hidden = YES;
    row.noteRow = gridRow(grid, nil, row.note, 0, 3);
    row.noteRow.hidden = YES;
    m_rows.push_back(std::move(row));
  }

  void refreshChoices(Row& row, const CommandState& scope) {
    if (row.tile != nil) {
      refreshTile(row, scope);
      return;
    }
    if (row.choiceButtons != nil) {
      const QString value = scope.choice(row.input->id);
      for (NSButton* button in row.choiceButtons) {
        button.state = qtString(button.identifier).section(QLatin1Char('|'), 2) == value
                           ? NSControlStateValueOn : NSControlStateValueOff;
      }
      return;
    }
    QVector<QPair<QString, QString>> options = row.input->choicesIn(scope);
    const QString value = scope.choice(row.input->id);
    if (!value.isEmpty() && std::none_of(options.cbegin(), options.cend(), [&value](const auto& option) {
          return option.first == value;
        })) {
      options.append({value, value});
    }
    bool same = row.popup.numberOfItems == options.size();
    for (int index = 0; same && index < options.size(); ++index) {
      NSMenuItem* item = [row.popup itemAtIndex:static_cast<NSInteger>(index)];
      same = (item.isSeparatorItem || qtString(item.title) == options[index].second) &&
             qtString(static_cast<NSString*>(item.representedObject)) == options[index].first;
    }
    if (!same) {
      [row.popup removeAllItems];
      QStringList labels;
      for (const auto& option : options) {
        if (isChoiceSeparator(option.first)) {
          // A line between the options (mitcad#828), not choosable.
          NSMenuItem* line = [NSMenuItem separatorItem];
          line.representedObject = nativeString(option.first);
          [row.popup.menu addItem:line];
          labels.append(QStringLiteral("---"));
          continue;
        }
        NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:nativeString(option.second)
                                                   action:nil keyEquivalent:@""];
        item.representedObject = nativeString(option.first);
        [row.popup.menu addItem:item];
        labels.append(option.second);
      }
      qDebug().noquote() << QStringLiteral("Panel %1 options %2: %3")
                                .arg(m_def.name, row.key, labels.join(QStringLiteral(" | ")));
    }
    NSInteger selected = 0;
    for (int index = 0; index < options.size(); ++index) {
      if (options[index].first == value) {
        selected = static_cast<NSInteger>(index);
        break;
      }
    }
    if (!options.isEmpty()) {
      [row.popup selectItemAtIndex:selected];
    }
  }

  // The chosen option's picture, name and detail on its tile (TileButton).
  void refreshTile(Row& row, const CommandState& scope) {
    const ChoiceTile tile = row.input->choiceTile(scope);
    const QString text = tile.detail.isEmpty() ? tile.title : tile.title + QLatin1Char('\n') + tile.detail;
    if (text == row.tileText && tile.image.cacheKey() == row.tilePicture) {
      return;
    }
    row.tileText = text;
    row.tilePicture = tile.image.cacheKey();
    row.tile.image = tileImage(tile.image);
    row.tile.imagePosition = row.tile.image == nil ? NSNoImage : NSImageLeading;
    NSMutableAttributedString* title = [[NSMutableAttributedString alloc]
        initWithString:nativeString(tile.title)
            attributes:@{NSFontAttributeName : [NSFont systemFontOfSize:[NSFont systemFontSize]],
                         NSForegroundColorAttributeName : NSColor.labelColor}];
    if (!tile.detail.isEmpty()) {
      [title appendAttributedString:[[NSAttributedString alloc]
                                        initWithString:nativeString(QLatin1Char('\n') + tile.detail)
                                            attributes:@{NSFontAttributeName : [NSFont systemFontOfSize:[NSFont smallSystemFontSize]],
                                                         NSForegroundColorAttributeName : NSColor.secondaryLabelColor}]];
    }
    row.tile.attributedTitle = title;
    row.tile.accessibilityValue = nativeString(tile.title);
    qDebug().noquote() << QStringLiteral("Panel %1 tile %2: %3").arg(m_def.name, row.key, tile.title);
  }

  void refreshOrder(Row& row, const Selection& items) {
    QStringList descriptions;
    for (const SelectionItem& item : items) {
      descriptions.append(item.describe());
    }
    if (descriptions == row.orderDescriptions) {
      return;
    }
    row.orderDescriptions = descriptions;
    for (NSView* child in [row.order.arrangedSubviews copy]) {
      [row.order removeArrangedSubview:child];
      [child removeFromSuperview];
    }
    for (int index = 0; index < descriptions.size(); ++index) {
      NSStackView* item = line();
      NSTextField* text = label(QStringLiteral("%1. %2").arg(index + 1).arg(descriptions[index]), true);
      [item addArrangedSubview:text];
      for (int direction : {-1, 1}) {
        NSButton* move = button(direction < 0 ? QStringLiteral("↑") : QStringLiteral("↓"),
                                QStringLiteral("move|%1|%2|%3").arg(row.key).arg(index).arg(index + direction));
        move.enabled = index + direction >= 0 && index + direction < descriptions.size();
        move.accessibilityLabel = nativeString(direction < 0 ? QObject::tr("Move %1 up").arg(descriptions[index])
                                                           : QObject::tr("Move %1 down").arg(descriptions[index]));
        [item addArrangedSubview:move];
      }
      NSButton* remove = button(QStringLiteral("−"), QStringLiteral("removeItem|%1|%2").arg(row.key).arg(index));
      remove.accessibilityLabel = nativeString(QObject::tr("Remove %1").arg(descriptions[index]));
      [item addArrangedSubview:remove];
      addFullWidth(row.order, item);
    }
  }

  // A value field's mark (InputDef::markFor, mitcad#980): its icon, the
  // tooltip saying why and what a click does.
  void refreshMark(Row& row, const FieldMark& mark) {
    if (mark == row.shownMark) {
      return;
    }
    row.shownMark = mark;
    row.mark.hidden = mark.isEmpty();
    if (mark.isEmpty()) {
      return;
    }
    const QString tip =
        mark.action.isEmpty() ? mark.tooltip : QObject::tr("%1\nClick: %2").arg(mark.tooltip, mark.action);
    if (NSImage* image = iconImage(themeIcon(mark.icon), 16, tip)) {
      row.mark.image = image;
    }
    row.mark.toolTip = nativeString(tip);
    row.mark.accessibilityLabel = nativeString(mark.action.isEmpty() ? mark.tooltip : mark.action);
  }

  // A note row's text and buttons (NoteRow, mitcad#655): the text and the
  // icon buttons on a line, the text buttons under it, each link on a line
  // of its own. Made again only when the note changes.
  void refreshNote(Row& row, const InputNote& note) {
    if (row.noteContent == nil || (row.noteBuilt && sameNote(note, row.shownNote))) {
      return;
    }
    row.shownNote = note;
    row.noteBuilt = true;
    for (NSView* child in [row.noteContent.arrangedSubviews copy]) {
      [row.noteContent removeArrangedSubview:child];
      [child removeFromSuperview];
    }
    NSStackView* textLine = line();
    textLine.spacing = 4;
    textLine.distribution = NSStackViewDistributionFill;
    if (!note.text.isEmpty()) {
      NSTextField* text = label(note.text);
      text.accessibilityIdentifier = nativeString(QStringLiteral("note_") + row.key);
      if (note.oneLine) {
        // On one line, elided in the middle, the whole text its tooltip.
        text.maximumNumberOfLines = 1;
        text.lineBreakMode = NSLineBreakByTruncatingMiddle;
        text.toolTip = nativeString(note.text);
      }
      [text setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
      [textLine addArrangedSubview:text];
    } else {
      [textLine addArrangedSubview:spring()];
    }
    bool icons = false;
    NSStackView* buttons = line();
    NSMutableArray<NSButton*>* links = [NSMutableArray array];
    for (int i = 0; i < note.actions.size(); ++i) {
      const QString icon = note.icons.value(i);
      const QString text = note.actions.at(i);
      const QString kind = QStringLiteral("note|%1|%2").arg(row.key).arg(i);
      if (icon == linkActionIcon()) {
        [links addObject:linkButton(text, kind)];
      } else if (!icon.isEmpty()) {
        NSImage* image = iconImage(themeIcon(icon), 16, text);
        [textLine addArrangedSubview:image != nil ? iconButton(image, text, kind) : button(text, kind)];
        icons = true;
      } else {
        // Elided rather than wider than the card (Import from slicer…).
        NSButton* control = button(text, kind);
        [control setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                          forOrientation:NSLayoutConstraintOrientationHorizontal];
        [buttons addArrangedSubview:control];
      }
    }
    if (!note.text.isEmpty() || icons) {
      addFullWidth(row.noteContent, textLine);
    }
    if (buttons.arrangedSubviews.count > 0) {
      [row.noteContent addArrangedSubview:buttons];
    }
    for (NSButton* link in links) {
      addFullWidth(row.noteContent, link);
    }
  }

  // Shows or hides an input's grid rows and views together.
  void setRowShown(Row& row, bool shown) {
    setHidden(row.box, !shown);
    setHidden(row.caption, !shown);
    setHidden(row.heading, !shown);
    setHidden(row.headingRow, !shown);
    setHidden(row.mainRow, !shown);
  }

  void updateNote(Row& row) {
    if (row.note == nil) {
      return;
    }
    const auto texts = m_notes.value(row.key);
    const bool error = !texts.first.isEmpty();
    const QString text = error ? texts.first : texts.second;
    row.note.stringValue = nativeString(text);
    row.note.textColor = error ? NSColor.systemRedColor : NSColor.secondaryLabelColor;
    const bool noteShown = !text.isEmpty() && row.box != nil && !row.box.hidden;
    setHidden(row.note, !noteShown);
    setHidden(row.noteRow, !noteShown);
    row.box.accessibilityHelp = nativeString(text);
    if (row.edit != nil) {
      row.edit.accessibilityHelp = nativeString(text);
    }
  }

  bool shown(const Row& row) const {
    return !row.box.hidden && !row.box.hiddenOrHasHiddenAncestor;
  }

  Row* findRow(const QString& key) {
    const auto row = std::find_if(m_rows.begin(), m_rows.end(), [&key](const Row& candidate) {
      return candidate.key == key;
    });
    return row == m_rows.end() ? nullptr : &*row;
  }

  const Row* findRow(const QString& key) const {
    const auto row = std::find_if(m_rows.cbegin(), m_rows.cend(), [&key](const Row& candidate) {
      return candidate.key == key;
    });
    return row == m_rows.cend() ? nullptr : &*row;
  }

  QPointer<CommandSession> m_session;
  const CommandDef m_def;
  CommandState m_state;
  QPointer<QWidget> m_expressionHost; // the fx tools' window (nativeExpressionHost)
  MitcadCommandFormTarget* __strong m_target;
  NSStackView* __strong m_root;
  NSStackView* __strong m_header;
  NSStackView* __strong m_fields;
  NSStackView* __strong m_sections;
  NSGridView* __strong m_grid = nil;
  NSTextField* __strong m_result;
  NSTextField* __strong m_message;
  NSStackView* __strong m_footer;
  NSButton* __strong m_ok;
  std::vector<Row> m_rows;
  std::vector<HeaderAction> m_headerActions;
  // Declaration order keeps the presenter alive until its native adapter
  // has disconnected all callbacks, including queued model refreshes.
  struct Section {
    std::unique_ptr<QWidget> presenter;
    std::unique_ptr<NativeWidgetView> view;
  };
  std::unique_ptr<QWidget> m_sectionHost = std::make_unique<QWidget>();
  std::vector<Section> m_auxiliary;
  QHash<QString, int> m_counts;
  QHash<QString, QString> m_pendingStructure;
  QHash<QString, QColor> m_colors;
  QHash<QString, int> m_listColors;
  QHash<QString, QPair<QString, QString>> m_notes;
  QString m_active;
  QString m_editingKey; // the value field AppKit edits (mitcad#250)
  QTimer m_logTimer;
};

std::unique_ptr<NativeCommandForm> createNativeCommandForm(CommandSession& session, QWidget* parameterOwner) {
  return std::make_unique<AppKitCommandForm>(session, parameterOwner);
}

} // namespace mitcad

@implementation MitcadCommandFormTarget
- (void)performAction:(id)sender {
  if (form != nullptr && [sender isKindOfClass:[NSControl class]]) {
    form->action(static_cast<NSControl*>(sender));
  }
}
- (void)controlTextDidChange:(NSNotification*)notification {
  if (form != nullptr) {
    form->textChanged(static_cast<NSTextField*>(notification.object));
  }
}
- (void)controlTextDidEndEditing:(NSNotification*)notification {
  if (form != nullptr) {
    form->valueCommitted(static_cast<NSTextField*>(notification.object));
  }
}
- (BOOL)control:(NSControl*)control textView:(NSTextView*)textView doCommandBySelector:(SEL)command {
  (void)textView;
  return form != nullptr && form->textCommand(static_cast<NSTextField*>(control), command);
}
// AppKit's completion (F5 or Option-Esc) offers a value field's parameters.
- (NSArray<NSString*>*)control:(NSControl*)control
                      textView:(NSTextView*)textView
                   completions:(NSArray<NSString*>*)words
           forPartialWordRange:(NSRange)charRange
           indexOfSelectedItem:(NSInteger*)index {
  NSArray<NSString*>* names =
      form != nullptr && [control isKindOfClass:NSTextField.class]
          ? form->completions(static_cast<NSTextField*>(control), textView, charRange)
          : nil;
  if (names == nil) {
    return words;
  }
  if (index != nullptr) {
    *index = names.count > 0 ? 0 : -1;
  }
  return names;
}
@end
