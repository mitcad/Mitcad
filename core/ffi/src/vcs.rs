// SPDX-License-Identifier: MIT
//! The version history for C++ (P12b, `mitcad_vcs`): projects whose folder
//! is the root of a git repository, driven with the JSON commands of
//! `mitcad_vcs::api`, and older versions opened as documents.

use std::path::Path;

use mitcad_vcs::{ProjectRepo, VcsError, api};

use crate::Document;
use crate::kernel::OcctKernel;

/// A project with version history.
pub struct Project(pub(crate) ProjectRepo);

pub fn open_project(path: &str) -> Result<Box<Project>, VcsError> {
    ProjectRepo::open(Path::new(path)).map(|repo| Box::new(Project(repo)))
}

pub fn init_project_history(dir: &str, author: &str) -> Result<String, VcsError> {
    api::init(Path::new(dir), author).map(|(_, answer)| answer)
}

pub fn create_project_repository(dir: &str) -> Result<Box<Project>, VcsError> {
    ProjectRepo::create(Path::new(dir)).map(|repo| Box::new(Project(repo)))
}

pub fn git_repository_root(path: &str) -> String {
    mitcad_vcs::repository_root(Path::new(path))
        .map(|root| root.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn load_version(project: &Project, rev: &str, path: &str) -> Result<Box<Document>, VcsError> {
    project
        .0
        .load_version(rev, Path::new(path), OcctKernel)
        .map(|document| Box::new(Document(document)))
}

impl Project {
    pub fn command(&self, json: &str) -> Result<String, VcsError> {
        self.0.command(json)
    }

    pub fn command_text(&self, json: &str) -> Result<String, VcsError> {
        self.0.command_text(json)
    }
}
