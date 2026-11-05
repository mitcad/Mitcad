// SPDX-License-Identifier: MIT
//! The rolled-back features evaluated in the background (mitcad#1267):
//! the application's side of `mitcad_model::RolledBackRun`
//! (`core/model/src/document/rolled_back.rs`).
//!
//! When the window's session is idle with the marker before the end of the
//! timeline, the application makes a [`BackgroundRun`] of its document on
//! its own thread (the document is idle there), runs it on a thread of its
//! own at a low priority (`app/framework/DocumentSession.cpp`), and gives
//! it back to the document to adopt once it is done; it cancels it through
//! the run's [`JobControl`] as soon as a job starts or a command or a
//! sketch opens, and waits for it to stop before a job takes the document.

use crate::Document;
use crate::jobs::JobControl;
use crate::kernel::OcctKernel;

/// A fork of a document that evaluates the features after its marker, or
/// nothing (no feature after the marker to evaluate).
///
/// SAFETY (threads): as [`Document`], a run crosses threads only as a
/// handover through the worker's mutex (`app/framework/ModelWorker.hpp`):
/// made on the document's thread, run on the background thread, given
/// back to the document's thread when the run ended. It shares no mutable
/// state with the document (`RolledBackRun`); the shapes both hold are
/// only read by the run's evaluations, as by the forks of the `.f3d`
/// import and the recompute's workers (`kernel/shape.rs`). The view never
/// triangulates them while it runs: the jobs mesh the bodies the view
/// draws, and the application starts no run after a job whose meshing was
/// asked to stop (the view meshes what such a job left).
pub struct BackgroundRun(Option<mitcad_model::RolledBackRun<OcctKernel>>);

impl Document {
    /// A run of the features after the marker, reporting to `control` (and
    /// cancelled through it); empty when there is nothing to evaluate.
    pub(crate) fn background_run(&self, control: &JobControl) -> Box<BackgroundRun> {
        Box::new(BackgroundRun(
            self.0.rolled_back_run(Some(control.monitor())),
        ))
    }

    /// Takes what the run evaluated into the document's cache, when the
    /// document's definition is still the one the run was made from;
    /// returns how many results were taken.
    pub(crate) fn adopt_background_run(&mut self, run: Box<BackgroundRun>) -> usize {
        let BackgroundRun(run) = *run;
        run.map_or(0, |run| self.0.adopt_rolled_back(run))
    }
}

impl BackgroundRun {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// The features after the marker it evaluates.
    pub(crate) fn rolled_back(&self) -> usize {
        self.0.as_ref().map_or(0, |run| run.rolled_back().len())
    }

    /// Runs it to the end of the timeline; false when cancelled (or empty).
    pub(crate) fn run(&mut self) -> bool {
        self.0.as_mut().is_some_and(|run| run.run())
    }

    /// The features it evaluated, also those of a cancelled run.
    pub(crate) fn evaluated(&self) -> usize {
        self.0.as_ref().map_or(0, |run| run.evaluated())
    }

    /// The revision of the document's state it was made from.
    pub(crate) fn base_revision(&self) -> u64 {
        self.0.as_ref().map_or(0, |run| run.base_revision())
    }
}
