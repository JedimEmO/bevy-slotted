//! The script conformance suite: `conformance/*.lua` cases run against any
//! [`ScriptRuntime`]. Phase 4 contract section 1.7; ADR 0001 makes this the
//! gate for swapping adapters.
//!
//! A case is a Lua file whose leading `-- ` comment lines carry directives:
//!
//! ```lua
//! -- STAGE: data
//! -- EVENTS: [ (type: "data_stage", api_version: 1) ]
//! -- EXPECT: [ (type: "register_item", id: "test:apple", def: {"name": "test:apple"}) ]
//! slotted.register_item("apple", {})
//! ```
//!
//! `STAGE` defaults to `data`; `EVENTS` defaults to `[DataStage]` or
//! `[ControlStart]` by stage; exactly one of `EXPECT` (the flattened commands
//! of every event, `Subscribe` stripped) or `EXPECT_ERROR` (`budget`,
//! `memory`, `compile`, `runtime`, `sandbox`) is required.

use std::path::{Path, PathBuf};

use slotted_script::{ModId, ScriptCommand, ScriptError, ScriptEvent, ScriptRuntime, Stage};

/// Which error class a negative case expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedError {
    /// `ScriptError::BudgetExceeded`.
    Budget,
    /// `ScriptError::Memory`.
    Memory,
    /// `ScriptError::Compile`.
    Compile,
    /// `ScriptError::Runtime`.
    Runtime,
    /// `ScriptError::Sandbox`.
    Sandbox,
}

impl ExpectedError {
    /// Whether `err` is this class.
    pub fn matches(self, err: &ScriptError) -> bool {
        matches!(
            (self, err),
            (Self::Budget, ScriptError::BudgetExceeded { .. })
                | (Self::Memory, ScriptError::Memory { .. })
                | (Self::Compile, ScriptError::Compile { .. })
                | (Self::Runtime, ScriptError::Runtime { .. })
                | (Self::Sandbox, ScriptError::Sandbox { .. })
        )
    }
}

/// What a case expects.
#[derive(Debug, Clone, PartialEq)]
pub enum Expectation {
    /// These commands, in order, `Subscribe` excluded.
    Commands(Vec<ScriptCommand>),
    /// An error of this class from `load` or any `call`.
    Error(ExpectedError),
}

/// One parsed case.
#[derive(Debug, Clone, PartialEq)]
pub struct ConformanceCase {
    /// File stem.
    pub name: String,
    /// Which stage the chunk loads as.
    pub stage: Stage,
    /// Events delivered in order after the load.
    pub events: Vec<ScriptEvent>,
    /// What must come back.
    pub expect: Expectation,
    /// The Lua source, directives included (they are comments).
    pub source: String,
}

/// Why a case file is malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{name}: {message}")]
pub struct CaseError {
    /// File stem.
    pub name: String,
    /// What is wrong.
    pub message: String,
}

impl ConformanceCase {
    /// Parses the directive block of `source`.
    ///
    /// # Errors
    ///
    /// [`CaseError`] when a directive is missing, duplicated or not RON.
    pub fn parse(name: &str, source: &str) -> Result<Self, CaseError> {
        // PHASE4-IMPL: A -- scan leading `-- ` lines, join continuation lines
        // of one directive, parse EVENTS/EXPECT with ron.
        let _ = source;
        Err(CaseError {
            name: name.to_owned(),
            message: "not implemented".to_owned(),
        })
    }
}

/// The outcome of one case.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseResult {
    /// File stem.
    pub name: String,
    /// `None` when the case passed, otherwise the mismatch, rendered.
    pub failure: Option<String>,
}

impl CaseResult {
    /// Whether the case passed.
    pub const fn passed(&self) -> bool {
        self.failure.is_none()
    }
}

/// The directory the cases live in.
pub fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

/// Reads and parses every `*.lua` in [`cases_dir`], sorted by name.
///
/// # Errors
///
/// The first [`CaseError`]; an unreadable directory is an empty list.
pub fn load_cases() -> Result<Vec<ConformanceCase>, CaseError> {
    // PHASE4-IMPL: A
    Ok(Vec::new())
}

/// Runs one case: `load` as the case's stage, then every event, comparing the
/// flattened commands (or the error class) with the expectation.
pub fn run_case(runtime: &mut dyn ScriptRuntime, case: &ConformanceCase) -> CaseResult {
    // PHASE4-IMPL: A -- ModId "test", name "<case>.lua"; strip Subscribe.
    let _ = ModId::new("test");
    let _ = runtime;
    CaseResult {
        name: case.name.clone(),
        failure: Some("not implemented".to_owned()),
    }
}

/// Runs every case.
pub fn run_all(runtime: &mut dyn ScriptRuntime) -> Vec<CaseResult> {
    match load_cases() {
        Ok(cases) => cases.iter().map(|c| run_case(runtime, c)).collect(),
        Err(err) => vec![CaseResult {
            name: err.name.clone(),
            failure: Some(err.message),
        }],
    }
}

/// Runs every case and panics with every failure listed.
///
/// # Panics
///
/// When any case fails or the directory does not parse.
pub fn assert_conformance(runtime: &mut dyn ScriptRuntime) {
    let results = run_all(runtime);
    let failures: Vec<String> = results
        .iter()
        .filter_map(|r| r.failure.as_ref().map(|f| format!("{}: {f}", r.name)))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} conformance cases failed:\n{}",
        failures.len(),
        results.len(),
        failures.join("\n")
    );
}
