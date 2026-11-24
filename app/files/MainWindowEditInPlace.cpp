// SPDX-License-Identifier: MIT
// Edit in Place (mitcad#2321, GitHub mitcad/Mitcad#598; app/COMMANDS.md
// "Linked designs"): a linked component edited inside the assembly that
// holds it, the changes going to the linked design's own file.
//
// The window takes the linked design's own document: the one its tab has
// (lent while the mode lasts, so that one file is never edited in two
// documents; the tab shows the design as it was and takes nothing in) or
// the file read without a tab. The holder's document waits meanwhile
// (m_inPlaceHolder). The model shows the holder's other bodies around the
// design in a read-only component of its own (begin_edit_in_place), so the
// timeline, the browser, the commands, Undo and Redo are the design's and
// the assembly is drawn, picked and measured; what a command picks on it is
// frozen where it is now, said in the message band (`in_place_notices`).
// Finish Edit in Place (the message band's Finish, the command, the
// browser's root row, Esc on an idle view) gives the holder back, which
// takes the design's bodies as one undo step (finish_edit_in_place). Save
// in the mode writes the design's file; the holder's Save writes unsaved
// in-place edits first, and closing asks about them.
#include "../MainWindow.hpp"

#include <cstdint>
#include <exception>
#include <optional>
#include <utility>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QMessageBox>
#include <QMetaObject>
#include <QPushButton>
#include <QtLogging>

#include "../WindowManager.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "LinkedFiles.hpp"

namespace mitcad {
namespace {

QString fileNameOf(const QString& path) { return QFileInfo(path).fileName(); }

// The document is its design file's, as saved or read (P8).
void markSaved(Document& document) {
  document.command(rustStr(compactJson({{QStringLiteral("cmd"), QStringLiteral("mark_saved")}})));
}

// The document's `modified`, read on the UI thread while it is idle.
bool documentModified(const Document& document) {
  const QByteArray query = compactJson({{QStringLiteral("query"), QStringLiteral("document")}});
  return parseObject(document.query(rustStr(query))).value(QStringLiteral("modified")).toBool();
}

} // namespace

void MainWindow::registerEditInPlaceCommands() {
  CommandDef finish;
  finish.id = QStringLiteral("assemble.finish_edit_in_place");
  finish.name = tr("Finish Edit in Place");
  finish.icon = QStringLiteral("finish-sketch");
  finish.kind = CommandDef::Kind::Action;
  finish.mode = CommandDef::Mode::Model;
  finish.tooltip = tr("Returns to the assembly, which takes the linked design's changes");
  finish.run = [this] { finishEditInPlace(); };
  finish.enabled = [this] { return m_inPlaceHolder.has_value(); };
  m_registry->add(finish);
}

void MainWindow::editInPlace(const QString& component) {
  if (m_inPlaceHolder) {
    showHint(tr("Finish Edit in Place of %1 first.").arg(m_inPlaceName));
    return;
  }
  if (!canChangeModel()) {
    showError(changeBlockedReason());
    return;
  }
  if (m_filePath.isEmpty()) {
    showWarning(tr("Save the design first: its links are followed from its folder."));
    return;
  }
  QString link;
  QString name;
  for (const DocumentSnapshot::Component& c : std::as_const(m_snapshot.components)) {
    if (c.uid == component && c.linked && !c.library) {
      link = c.link;
      name = m_snapshot.componentName(component);
    }
  }
  if (link.isEmpty()) {
    showError(tr("%1 is not a linked design.").arg(m_snapshot.componentName(component)));
    return;
  }
  const QString file = QFileInfo(linkedFilePath(m_filePath, link)).absoluteFilePath();
  if (file.isEmpty() || !QFileInfo::exists(file)) {
    showWarning(tr("%1 is not there (%2).").arg(fileNameOf(link), QDir::toNativeSeparators(file)));
    return;
  }
  // The design's own document: its tab's, lent while the mode lasts, else
  // the file read without a tab.
  QPointer<MainWindow> tab = m_windowManager != nullptr ? m_windowManager->findByPath(file) : nullptr;
  if (tab == this) {
    tab = nullptr;
  }
  std::optional<rust::Box<Document>> linked;
  QByteArray json;
  // Unsaved edits made in place before: the same design again, or written
  // before another is edited.
  bool fromKept = false;
  if (m_inPlaceKept && !tab && sameFile(m_inPlaceKeptFile, file)) {
    linked.emplace(std::move(*m_inPlaceKept));
    m_inPlaceKept.reset();
    fromKept = true;
  } else if ((m_inPlaceKept || m_inPlaceKeptTab) && !saveInPlaceEdits()) {
    return;
  }
  if (linked) {
    // Taken from what the last Edit in Place left.
  } else if (tab) {
    if (tab->modelBusy() || tab->m_mode != Mode::Idle || tab->m_inPlaceHolder || tab->m_inPlaceBorrower) {
      showWarning(tr("%1 is busy in its tab: finish there first.").arg(fileNameOf(file)));
      return;
    }
    if (tab->windowReadOnly()) {
      showWarning(tr("%1 is open read-only: it cannot be edited in place.").arg(fileNameOf(file)));
      return;
    }
    // The tab keeps a copy it does not compute or show until Finish.
    const rust::String text = tab->m_document->to_json();
    linked.emplace(load_document(text));
    std::swap(*linked, tab->m_document);
    tab->lendForInPlace(this);
  } else {
    QFile read(file);
    if (!read.open(QIODevice::ReadOnly)) {
      showError(tr("Could not open %1: %2").arg(fileNameOf(file), read.errorString()));
      return;
    }
    json = read.readAll();
  }
  const QByteArray location = file.toUtf8();
  const QByteArray links = compactJson({{QStringLiteral("cmd"), QStringLiteral("update_links")},
                                        {QStringLiteral("base"), QFileInfo(file).absolutePath()}});
  const QByteArray recompute = compactJson({{QStringLiteral("cmd"), QStringLiteral("recompute")}});
  const QByteArray request =
      compactJson({{QStringLiteral("component"), component}, {QStringLiteral("holder"), documentName()}});
  QJsonObject begun;
  QString failure;
  bool done = false;
  try {
    done = runJob(tr("Edit in Place"), tr("Reading %1").arg(fileNameOf(file)), [&](ModelJob& job) {
      if (!linked) {
        rust::Box<Document> document = load_document_at(rustStr(json), rustStr(location));
        {
          const AttachedJob attached(*document, job);
          document->command(rustStr(links));
          document->command(rustStr(recompute));
        }
        markSaved(*document);
        linked.emplace(std::move(document));
      }
      job.setStage(tr("Showing the assembly around %1").arg(name));
      const AttachedJob attached(**linked, job);
      begun = parseObject(begin_edit_in_place(**linked, *m_document, rustStr(request)));
    });
  } catch (const std::exception& e) {
    failure = errorText(e);
  }
  if (!done || !failure.isEmpty()) {
    if (tab && linked) {
      tab->returnFromInPlace(std::move(*linked));
    } else if (fromKept && linked) {
      m_inPlaceKept.emplace(std::move(*linked)); // its unsaved edits stay
    }
    if (!failure.isEmpty()) {
      showError(tr("%1 cannot be edited in place: %2").arg(name, failure));
    }
    return;
  }
  // The window's document is the design's now; the holder's waits.
  resetCommandState();
  std::swap(m_document, *linked);
  m_inPlaceHolder.emplace(std::move(*linked));
  m_inPlaceComponent = component;
  m_inPlaceName = name;
  m_inPlaceFile = file;
  m_inPlaceTab = tab;
  m_measureHistory.clear();
  m_selection.clear();
  refreshScene();
  m_viewer->fitAll();
  updateWindowTitle();
  updateActions();
  qInfo().noquote() << QStringLiteral("Edit in Place %1: %2 %3, %4 bodies of the assembly")
                           .arg(component, fileNameOf(file), tab ? QStringLiteral("from its tab")
                                                                 : QStringLiteral("read"))
                           .arg(begun.value(QStringLiteral("bodies")).toArray().size());
  showActionStatus(tr("Editing %1 in place in this assembly. What you pick on the assembly is fixed where it is "
                      "now.")
                       .arg(name),
                   NoticeLevel::Information, tr("Finish"), [this] { finishEditInPlace(); }, QString(),
                   tr("Finish Edit in Place"));
}

void MainWindow::finishEditInPlace() {
  if (!m_inPlaceHolder) {
    return;
  }
  if (!canChangeModel()) {
    showError(changeBlockedReason());
    return;
  }
  const QByteArray request = compactJson({{QStringLiteral("component"), m_inPlaceComponent},
                                          {QStringLiteral("base"), QFileInfo(m_filePath).absolutePath()}});
  QJsonObject answer;
  QString failure;
  bool done = false;
  try {
    done = runJob(tr("Finish Edit in Place"), tr("Taking %1").arg(m_inPlaceName), [&](ModelJob& job) {
      const AttachedJob attached(**m_inPlaceHolder, job);
      answer = parseObject(finish_edit_in_place(**m_inPlaceHolder, *m_document, rustStr(request)));
    });
  } catch (const std::exception& e) {
    // The design leaves the mode before the holder takes it: the holder is
    // given back as it was, with why.
    failure = errorText(e);
  }
  // Canceled, or refused, before the design left the mode: it stays.
  const bool stillInPlace = !query({{QStringLiteral("query"), QStringLiteral("document")}})
                                 .toObject()
                                 .value(QStringLiteral("in_place"))
                                 .isUndefined();
  if (stillInPlace && (!done || !failure.isEmpty())) {
    if (failure.isEmpty()) {
      showHint(tr("Finish Edit in Place canceled."));
    } else {
      showError(tr("Edit in Place of %1 could not finish: %2").arg(m_inPlaceName, failure));
    }
    return;
  }
  // The holder's document is the window's again.
  resetCommandState();
  std::swap(m_document, *m_inPlaceHolder);
  rust::Box<Document> linked = std::move(*m_inPlaceHolder);
  m_inPlaceHolder.reset();
  const bool unsaved = documentModified(*linked);
  const QPointer<MainWindow> tab = m_inPlaceTab;
  m_inPlaceTab = nullptr;
  if (tab) {
    tab->returnFromInPlace(std::move(linked));
    m_inPlaceKeptTab = unsaved ? tab : nullptr;
    m_inPlaceKept.reset();
  } else if (unsaved) {
    // Its unsaved edits wait for the holder's Save (or its close question).
    m_inPlaceKept.emplace(std::move(linked));
    m_inPlaceKeptTab = nullptr;
  } else {
    m_inPlaceKept.reset();
    m_inPlaceKeptTab = nullptr;
  }
  if (unsaved) {
    m_inPlaceKeptComponent = m_inPlaceComponent;
    m_inPlaceKeptFile = m_inPlaceFile;
  }
  m_measureHistory.clear();
  m_selection.clear();
  refreshScene();
  m_viewer->fitAll();
  updateWindowTitle();
  updateActions();
  const QString label = answer.value(QStringLiteral("label")).toString();
  qInfo().noquote() << QStringLiteral("Finish Edit in Place %1: %2, %3; %4")
                           .arg(m_inPlaceComponent,
                                answer.value(QStringLiteral("changed")).toBool() ? QStringLiteral("changed")
                                                                                  : QStringLiteral("unchanged"),
                                unsaved ? QStringLiteral("unsaved") : QStringLiteral("saved"),
                                label.isEmpty() ? QStringLiteral("no undo step") : label);
  if (!failure.isEmpty()) {
    showError(tr("%1 did not take the changes made in place: %2").arg(m_inPlaceName, failure));
  } else if (answer.value(QStringLiteral("changed")).toBool()) {
    showLinkMessages(answer.value(QStringLiteral("messages")).toArray());
    if (unsaved) {
      showStatus(tr("%1 has changes made in place that are not saved yet; Save writes them to %2.")
                     .arg(m_inPlaceName, fileNameOf(m_inPlaceFile)),
                 NoticeLevel::Information);
    }
  } else {
    showHint(tr("Edit in Place of %1 finished: nothing changed.").arg(m_inPlaceName));
  }
  m_inPlaceComponent.clear();
  m_inPlaceName.clear();
  m_inPlaceFile.clear();
}

bool MainWindow::saveInPlaceDesign() {
  if (!m_inPlaceHolder) {
    return false;
  }
  if (m_session && (m_nativeUi || modelBusy())) {
    showHint(tr("Finish the current command before saving."));
    return false;
  }
  // No picture of the view, which shows the assembly around it: the
  // design keeps the preview it has.
  static const std::uint8_t none = 0;
  QString error;
  bool written = false;
  try {
    idleDocument().save_design_stamped(rustStr(m_inPlaceFile.toUtf8()), "auto",
                                       rust::Slice<const std::uint8_t>(&none, 0));
    written = true;
  } catch (const std::exception& e) {
    error = errorText(e);
  }
  if (!written) {
    showError(tr("Could not save %1:\n%2").arg(QDir::toNativeSeparators(m_inPlaceFile), error));
    return false;
  }
  markSaved(*m_document);
  updateWindowTitle();
  qInfo().noquote() << QStringLiteral("Edit in Place: saved %1").arg(m_inPlaceFile);
  showHint(tr("Saved %1").arg(QDir::toNativeSeparators(m_inPlaceFile)));
  // The other tabs that link the design offer to take it (mitcad#1416).
  if (m_windowManager != nullptr) {
    m_windowManager->designSaved(this, m_inPlaceFile);
  }
  return true;
}

bool MainWindow::saveInPlaceEdits() {
  // Its save is this window's own: the file is not offered as changed here
  // (linkedDesignSaved), the link records it below.
  m_inPlaceSaving = m_inPlaceKeptFile;
  if (m_inPlaceKeptTab) {
    // The design's tab has the edits: its own Save writes them.
    const QPointer<MainWindow> tab = m_inPlaceKeptTab;
    if (tab->isModified() && !tab->save()) {
      m_inPlaceSaving.clear();
      return false;
    }
  } else if (m_inPlaceKept) {
    // No picture of the view: the design keeps the preview it has.
    static const std::uint8_t none = 0;
    QString error;
    bool written = false;
    try {
      (*m_inPlaceKept)
          ->save_design_stamped(rustStr(m_inPlaceKeptFile.toUtf8()), "auto",
                                rust::Slice<const std::uint8_t>(&none, 0));
      written = true;
    } catch (const std::exception& e) {
      error = errorText(e);
    }
    if (!written) {
      m_inPlaceSaving.clear();
      showError(tr("Could not save %1:\n%2").arg(QDir::toNativeSeparators(m_inPlaceKeptFile), error));
      return false;
    }
    markSaved(**m_inPlaceKept);
    qInfo().noquote() << QStringLiteral("Edit in Place: saved %1 with the assembly").arg(m_inPlaceKeptFile);
    if (m_windowManager != nullptr) {
      m_windowManager->designSaved(this, m_inPlaceKeptFile);
    }
  } else {
    m_inPlaceSaving.clear();
    return true;
  }
  // The link records the file now: it is not offered as changed.
  const QByteArray request = compactJson({{QStringLiteral("component"), m_inPlaceKeptComponent},
                                          {QStringLiteral("base"), QFileInfo(m_filePath).absolutePath()}});
  try {
    linked_saved_in_place(*m_document, rustStr(request));
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Edit in Place: the link did not record %1: %2")
                                .arg(m_inPlaceKeptFile, errorText(e));
  }
  m_inPlaceSaving.clear();
  m_inPlaceKept.reset();
  m_inPlaceKeptTab = nullptr;
  return true;
}

bool MainWindow::settleInPlaceEdits() {
  // In the mode: the design's unsaved edits asked about, then the holder
  // given back. A design from a tab goes back to it with its edits, which
  // that tab asks about when it closes.
  if (m_inPlaceHolder) {
    if (!m_inPlaceTab && documentModified(idleDocument())) {
      qInfo().noquote() << QStringLiteral("Save question: %1 (in place)").arg(fileNameOf(m_inPlaceFile));
      QMessageBox box(QMessageBox::Warning, tr("MitCad"),
                      tr("Save the changes made in place to %1?").arg(fileNameOf(m_inPlaceFile)),
                      QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel, this);
      box.setInformativeText(tr("%1 is edited in place in %2. Your changes to it will be lost if you don't save "
                                "them.")
                                 .arg(m_inPlaceName, documentName()));
      box.button(QMessageBox::Save)->setText(tr("&Save"));
      box.button(QMessageBox::Discard)->setText(tr("&Don't Save"));
      box.setDefaultButton(QMessageBox::Save);
      prepareModal(&box);
      switch (execDialog(&box)) {
      case QMessageBox::Save:
        if (!saveInPlaceDesign()) {
          return false;
        }
        break;
      case QMessageBox::Discard:
        break;
      default:
        return false;
      }
    }
    finishEditInPlace();
    if (m_inPlaceHolder) {
      return false; // it could not finish (a command is open)
    }
    // Not saved and not from a tab: dropped, as asked.
    m_inPlaceKept.reset();
    return true;
  }
  if (m_inPlaceKept && documentModified(**m_inPlaceKept)) {
    qInfo().noquote() << QStringLiteral("Save question: %1 (in place)").arg(fileNameOf(m_inPlaceKeptFile));
    QMessageBox box(QMessageBox::Warning, tr("MitCad"),
                    tr("Save the changes made in place to %1?").arg(fileNameOf(m_inPlaceKeptFile)),
                    QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel, this);
    box.setInformativeText(tr("They were made in %1. Your changes will be lost if you don't save them.")
                               .arg(documentName()));
    box.button(QMessageBox::Save)->setText(tr("&Save"));
    box.button(QMessageBox::Discard)->setText(tr("&Don't Save"));
    box.setDefaultButton(QMessageBox::Save);
    prepareModal(&box);
    switch (execDialog(&box)) {
    case QMessageBox::Save:
      return saveInPlaceEdits();
    case QMessageBox::Discard:
      m_inPlaceKept.reset();
      return true;
    default:
      return false;
    }
  }
  return true;
}

void MainWindow::showInPlaceNotices(const QJsonObject& answer) {
  const QJsonArray notices = answer.value(QStringLiteral("in_place_notices")).toArray();
  if (notices.isEmpty()) {
    return;
  }
  QPointer<MainWindow> self(this);
  // After the command's own messages.
  QMetaObject::invokeMethod(
      this,
      [self, notices] {
        if (!self) {
          return;
        }
        for (const QJsonValue& notice : notices) {
          qInfo().noquote() << QStringLiteral("Edit in Place notice: %1").arg(notice.toString());
          self->showStatus(notice.toString(), NoticeLevel::Information);
        }
      },
      Qt::QueuedConnection);
}

void MainWindow::lendForInPlace(MainWindow* borrower) {
  m_inPlaceBorrower = borrower;
  setEnabled(false);
  qInfo().noquote() << QStringLiteral("Edit in Place: %1 lent to %2").arg(documentName(), borrower->documentName());
  showStatus(tr("%1 is edited in place in %2. Finish there to edit it here again.")
                 .arg(documentName(), borrower->documentName()),
             NoticeLevel::Information);
}

void MainWindow::returnFromInPlace(rust::Box<Document> document) {
  // The stand-in goes with `document` at the end of this scope.
  std::swap(m_document, document);
  m_inPlaceBorrower = nullptr;
  setEnabled(true);
  refreshScene();
  updateWindowTitle();
  updateActions();
  qInfo().noquote() << QStringLiteral("Edit in Place: %1 back in its tab").arg(documentName());
}

QString MainWindow::lentReason() const {
  if (!m_inPlaceBorrower) {
    return QString();
  }
  return tr("%1 is edited in place in %2: finish there first.")
      .arg(documentName(), m_inPlaceBorrower->documentName());
}

} // namespace mitcad
