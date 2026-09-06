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
//!
//! `STAGE` and `EXPECT_ERROR` take the rest of their own line. `EVENTS` and
//! `EXPECT` carry RON, which may run over as many `-- ` lines as it needs: the
//! parser keeps appending comment lines until the accumulated text parses. Any
//! other leading comment line is prose and is ignored.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use slotted_script::{ModId, ScriptCommand, ScriptError, ScriptEvent, ScriptRuntime, Stage};

/// The mod id every case loads under.
const CASE_MOD_ID: &str = "test";

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

    /// The directive spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Budget => "budget",
            Self::Memory => "memory",
            Self::Compile => "compile",
            Self::Runtime => "runtime",
            Self::Sandbox => "sandbox",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        match word {
            "budget" => Some(Self::Budget),
            "memory" => Some(Self::Memory),
            "compile" => Some(Self::Compile),
            "runtime" => Some(Self::Runtime),
            "sandbox" => Some(Self::Sandbox),
            _ => None,
        }
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

/// One directive line, split into its keyword and the rest.
fn directive(line: &str) -> Option<(&'static str, &str)> {
    let body = line.trim_start().strip_prefix("--")?.trim_start();
    for key in ["STAGE", "EVENTS", "EXPECT_ERROR", "EXPECT"] {
        if let Some(rest) = body.strip_prefix(key)
            && let Some(rest) = rest.strip_prefix(':')
        {
            return Some((key, rest.trim()));
        }
    }
    None
}

/// The text of a comment line, `--` and one following space removed.
fn comment_body(line: &str) -> Option<&str> {
    let body = line.trim_start().strip_prefix("--")?;
    Some(body.strip_prefix(' ').unwrap_or(body))
}

impl ConformanceCase {
    /// Parses the directive block of `source`.
    ///
    /// # Errors
    ///
    /// [`CaseError`] when a directive is missing, duplicated or not RON.
    pub fn parse(name: &str, source: &str) -> Result<Self, CaseError> {
        let bad = |message: String| CaseError {
            name: name.to_owned(),
            message,
        };

        let lines: Vec<&str> = source
            .lines()
            .take_while(|l| l.trim_start().starts_with("--") || l.trim().is_empty())
            .collect();

        let mut stage: Option<Stage> = None;
        let mut events: Option<Vec<ScriptEvent>> = None;
        let mut expect: Option<Vec<ScriptCommand>> = None;
        let mut expect_error: Option<ExpectedError> = None;

        let mut index = 0;
        while index < lines.len() {
            let Some((key, rest)) = directive(lines[index]) else {
                index += 1;
                continue;
            };
            match key {
                "STAGE" => {
                    if stage.is_some() {
                        return Err(bad("STAGE given twice".to_owned()));
                    }
                    stage = Some(match rest {
                        "data" => Stage::Data,
                        "control" => Stage::Control,
                        "test" => Stage::Test,
                        other => {
                            return Err(bad(format!(
                                "STAGE must be data, control or test, not {other:?}"
                            )));
                        }
                    });
                    index += 1;
                }
                "EXPECT_ERROR" => {
                    if expect_error.is_some() {
                        return Err(bad("EXPECT_ERROR given twice".to_owned()));
                    }
                    expect_error = Some(ExpectedError::parse(rest).ok_or_else(|| {
                        bad(format!(
                            "EXPECT_ERROR must be budget, memory, compile, runtime or sandbox, not {rest:?}"
                        ))
                    })?);
                    index += 1;
                }
                "EVENTS" => {
                    if events.is_some() {
                        return Err(bad("EVENTS given twice".to_owned()));
                    }
                    let (parsed, next) = ron_block(name, "EVENTS", &lines, index, rest)?;
                    events = Some(parsed);
                    index = next;
                }
                "EXPECT" => {
                    if expect.is_some() {
                        return Err(bad("EXPECT given twice".to_owned()));
                    }
                    let (parsed, next) = ron_block(name, "EXPECT", &lines, index, rest)?;
                    expect = Some(parsed);
                    index = next;
                }
                _ => unreachable!("directive() only returns the four keys"),
            }
        }

        let stage = stage.unwrap_or(Stage::Data);
        let expect = match (expect, expect_error) {
            (Some(_), Some(_)) => {
                return Err(bad(
                    "EXPECT and EXPECT_ERROR are mutually exclusive".to_owned()
                ));
            }
            (Some(commands), None) => Expectation::Commands(commands),
            (None, Some(class)) => Expectation::Error(class),
            (None, None) => return Err(bad("neither EXPECT nor EXPECT_ERROR is given".to_owned())),
        };
        let events = events.unwrap_or_else(|| vec![default_event(stage)]);

        Ok(Self {
            name: name.to_owned(),
            stage,
            events,
            expect,
            source: source.to_owned(),
        })
    }
}

/// The event a case gets when it does not list any.
fn default_event(stage: Stage) -> ScriptEvent {
    match stage {
        Stage::Data => ScriptEvent::DataStage {
            api_version: slotted_script::API_VERSION,
        },
        Stage::Control => ScriptEvent::ControlStart {
            api_version: slotted_script::API_VERSION,
            mods: vec![CASE_MOD_ID.to_owned()],
        },
        // A test-stage case that lists no events asks for its test names, the
        // one event a test file always answers (Phase 6 contract 3.1).
        Stage::Test => ScriptEvent::TestList,
    }
}

/// Reads a RON directive that may span several comment lines. Returns the
/// parsed value and the index of the first line after the block.
fn ron_block<T: serde::de::DeserializeOwned>(
    name: &str,
    key: &str,
    lines: &[&str],
    start: usize,
    first: &str,
) -> Result<(T, usize), CaseError> {
    let mut text = first.to_owned();
    let mut index = start + 1;
    let mut last_error;
    loop {
        match ron::from_str::<T>(&text) {
            Ok(value) => return Ok((value, index)),
            Err(err) => last_error = err.to_string(),
        }
        // Not yet complete: pull in the next comment line, unless a new
        // directive or the end of the comment block says the RON is over.
        let over = index >= lines.len() || directive(lines[index]).is_some();
        let body = if over {
            None
        } else {
            comment_body(lines[index])
        };
        let Some(body) = body else {
            return Err(CaseError {
                name: name.to_owned(),
                message: format!("{key} is not RON: {last_error}\n  text: {text}"),
            });
        };
        text.push('\n');
        text.push_str(body);
        index += 1;
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

/// Every case's outcome, plus the counts a caller wants to print.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    /// One entry per case, in name order.
    pub results: Vec<CaseResult>,
}

impl Report {
    /// How many cases ran.
    pub fn total(&self) -> usize {
        self.results.len()
    }

    /// How many passed.
    pub fn passed(&self) -> usize {
        self.results.iter().filter(|r| r.passed()).count()
    }

    /// The rendered failures, one string per failing case.
    pub fn failures(&self) -> Vec<String> {
        self.results
            .iter()
            .filter_map(|r| r.failure.as_ref().map(|f| format!("{}: {f}", r.name)))
            .collect()
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
    load_cases_from(&cases_dir())
}

/// Reads and parses every `*.lua` in `dir`, sorted by name.
///
/// # Errors
///
/// The first [`CaseError`]; an unreadable directory is an empty list.
pub fn load_cases_from(dir: &Path) -> Result<Vec<ConformanceCase>, CaseError> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lua"))
        .collect();
    paths.sort();

    let mut cases = Vec::with_capacity(paths.len());
    for path in paths {
        let name = path.file_stem().map_or_else(
            || path.display().to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        let source = std::fs::read_to_string(&path).map_err(|err| CaseError {
            name: name.clone(),
            message: format!("unreadable: {err}"),
        })?;
        cases.push(ConformanceCase::parse(&name, &source)?);
    }
    Ok(cases)
}

/// Runs one case: `load` as the case's stage, then every event, comparing the
/// flattened commands (or the error class) with the expectation.
pub fn run_case(runtime: &mut dyn ScriptRuntime, case: &ConformanceCase) -> CaseResult {
    CaseResult {
        name: case.name.clone(),
        failure: run_case_inner(runtime, case).err(),
    }
}

fn run_case_inner(runtime: &mut dyn ScriptRuntime, case: &ConformanceCase) -> Result<(), String> {
    let mod_id = ModId::new(CASE_MOD_ID).map_err(|e| e.to_string())?;
    let file = format!("{}.lua", case.name);

    let mut actual: Vec<ScriptCommand> = Vec::new();
    let mut error: Option<ScriptError> = None;

    match runtime.load(&mod_id, &file, &case.source, case.stage) {
        Ok(id) => {
            for event in &case.events {
                match runtime.call(id, event) {
                    Ok(commands) => actual.extend(
                        commands
                            .into_iter()
                            .filter(|c| !matches!(c, ScriptCommand::Subscribe { .. })),
                    ),
                    Err(err) => {
                        error = Some(err);
                        break;
                    }
                }
            }
            runtime.unload(id);
        }
        Err(err) => error = Some(err),
    }

    match (&case.expect, error) {
        (Expectation::Error(class), Some(err)) => {
            if class.matches(&err) {
                Ok(())
            } else {
                Err(format!("expected a {} error, got {err:?}", class.as_str()))
            }
        }
        (Expectation::Error(class), None) => Err(format!(
            "expected a {} error, but the case succeeded with {} command(s):\n{}",
            class.as_str(),
            actual.len(),
            render(&actual)
        )),
        (Expectation::Commands(_), Some(err)) => Err(format!("unexpected error: {err:?}")),
        (Expectation::Commands(expected), None) => {
            if expected == &actual {
                Ok(())
            } else {
                Err(diff(expected, &actual))
            }
        }
    }
}

/// A readable, one-line-per-command comparison.
fn diff(expected: &[ScriptCommand], actual: &[ScriptCommand]) -> String {
    let mut out = format!(
        "commands differ ({} expected, {} produced)\n",
        expected.len(),
        actual.len()
    );
    for index in 0..expected.len().max(actual.len()) {
        let want = expected.get(index);
        let got = actual.get(index);
        let mark = if want == got { "  " } else { "!!" };
        let _ = writeln!(
            out,
            "{mark} #{index}\n{mark}   expected: {}\n{mark}   actual:   {}",
            want.map_or_else(|| "<none>".to_owned(), |c| format!("{c:?}")),
            got.map_or_else(|| "<none>".to_owned(), |c| format!("{c:?}")),
        );
    }
    out
}

fn render(commands: &[ScriptCommand]) -> String {
    commands
        .iter()
        .map(|c| format!("  {c:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs every case in [`cases_dir`].
pub fn run_all(runtime: &mut dyn ScriptRuntime) -> Vec<CaseResult> {
    run_conformance_dir(runtime, &cases_dir()).results
}

/// Runs every `*.lua` case in `dir` against `runtime`.
pub fn run_conformance_dir(runtime: &mut dyn ScriptRuntime, dir: &Path) -> Report {
    match load_cases_from(dir) {
        Ok(cases) => Report {
            results: cases.iter().map(|c| run_case(runtime, c)).collect(),
        },
        Err(err) => Report {
            results: vec![CaseResult {
                name: err.name.clone(),
                failure: Some(err.message),
            }],
        },
    }
}

/// Runs every case and panics with every failure listed.
///
/// # Panics
///
/// When any case fails, the directory does not parse, or fewer than the 20
/// cases the contract requires are present.
pub fn assert_conformance(runtime: &mut dyn ScriptRuntime) {
    let report = Report {
        results: run_all(runtime),
    };
    let failures = report.failures();
    assert!(
        failures.is_empty(),
        "{} of {} conformance cases failed:\n{}",
        failures.len(),
        report.total(),
        failures.join("\n")
    );
    assert!(
        report.total() >= 20,
        "the suite must carry at least 20 cases, found {}",
        report.total()
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_case() {
        let case = ConformanceCase::parse(
            "x",
            "-- EXPECT: [ (type: \"log\", level: \"info\", message: \"hi\") ]\nprint(\"hi\")\n",
        )
        .unwrap();
        assert_eq!(case.stage, Stage::Data);
        assert_eq!(case.events, vec![ScriptEvent::DataStage { api_version: 1 }]);
        let Expectation::Commands(commands) = &case.expect else {
            panic!("expected commands");
        };
        assert_eq!(commands.len(), 1);
    }

    #[test]
    fn a_multi_line_ron_block_and_a_trailing_comment() {
        let case = ConformanceCase::parse(
            "y",
            "-- STAGE: control\n\
             -- EXPECT: [\n\
             --   (type: \"log\", level: \"warn\", message: \"a\"),\n\
             --   (type: \"log\", level: \"warn\", message: \"b\"),\n\
             -- ]\n\
             -- a prose line the parser ignores\n\
             slotted.warn(\"a\")\n",
        )
        .unwrap();
        assert_eq!(case.stage, Stage::Control);
        let Expectation::Commands(commands) = &case.expect else {
            panic!("expected commands");
        };
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn expect_error_ignores_the_prose_after_it() {
        let case = ConformanceCase::parse(
            "z",
            "-- EXPECT_ERROR: budget\n-- because it spins\nwhile true do end\n",
        )
        .unwrap();
        assert_eq!(case.expect, Expectation::Error(ExpectedError::Budget));
    }

    #[test]
    fn a_case_without_an_expectation_is_rejected() {
        assert!(ConformanceCase::parse("w", "-- STAGE: data\nprint(1)\n").is_err());
    }

    #[test]
    fn the_shipped_suite_parses_and_is_large_enough() {
        let cases = load_cases().unwrap();
        assert!(cases.len() >= 20, "only {} cases", cases.len());
    }
}
