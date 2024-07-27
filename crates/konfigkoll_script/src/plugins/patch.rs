//! Facilities to patch a file compared to the default package provided one.

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Context;
use regex::Regex;
use rune::runtime::Shared;
use rune::runtime::VmResult;
use rune::Any;
use rune::ContextError;
use rune::Module;

/// A simple line editor, like sed
#[derive(Debug, Default, Any)]
#[rune(item = ::patch)]
struct LineEditor {
    inner: Rc<RefCell<konfigkoll_utils::line_edit::EditProgram>>,
}

impl LineEditor {
    /// Create a new empty line editor
    #[rune::function(path = Self::new)]
    fn new() -> Self {
        Default::default()
    }

    /// Add a new rule to the line editor.
    ///
    /// Returns a Result<()>, where the error variant can happen on invalid regexes.
    #[rune::function]
    pub fn add(&mut self, selector: &Selector, action: &Action) -> anyhow::Result<()> {
        self.inner
            .borrow_mut()
            .add(selector.try_into()?, false, action.try_into()?);
        Ok(())
    }

    /// Add a new rule where the selector condition has been inverted to the line editor
    ///
    /// Returns a Result<()>, where the error variant can happen on invalid regexes.
    #[rune::function]
    pub fn add_inverted(&mut self, selector: &Selector, action: &Action) -> anyhow::Result<()> {
        self.inner
            .borrow_mut()
            .add(selector.try_into()?, true, action.try_into()?);
        Ok(())
    }

    /// Apply the line editor to a string
    #[rune::function]
    fn apply(&self, text: &str) -> String {
        self.inner.borrow().apply(text)
    }

    /// Clone the line editor, allowing "forking it" into two different related variants
    #[rune::function]
    fn clone(&self) -> Self {
        Self {
            inner: Rc::new(RefCell::new(self.inner.borrow().clone())),
        }
    }
}

/// Selects if a line should be edited by [`LineEditor`] or not
#[derive(Debug, Any)]
#[rune(item = ::patch)]
enum Selector {
    /// Match all lines
    #[rune(constructor)]
    All,
    /// End of file
    #[rune(constructor)]
    Eof,
    /// Match a specific line number (1-indexed)
    #[rune(constructor)]
    Line(#[rune(get)] usize),
    /// A range of line numbers (1-indexed, inclusive)
    #[rune(constructor)]
    Range(#[rune(get)] usize, #[rune(get)] usize),
    /// A regex to match the line
    #[rune(constructor)]
    Regex(#[rune(get)] String),
    /// A custom function, passed the line number and current line, returning a bool
    #[rune(constructor)]
    Function(#[rune(get)] Shared<rune::runtime::Function>),
}

impl TryFrom<&Selector> for konfigkoll_utils::line_edit::Selector {
    type Error = anyhow::Error;

    fn try_from(value: &Selector) -> Result<Self, Self::Error> {
        match value {
            Selector::All => Ok(Self::All),
            Selector::Eof => Ok(Self::Eof),
            Selector::Line(n) => Ok(Self::Line(*n)),
            Selector::Range(a, b) => Ok(Self::Range(*a, *b)),
            Selector::Regex(r) => Ok(Self::Regex(Regex::new(r).context("invalid regex")?)),
            Selector::Function(ref f) => {
                let f = f.clone();
                Ok(Self::Function(Rc::new(move |lineno, s| {
                    let guard = f.borrow_mut().expect("Failed to borrow function object");
                    match guard.call::<_, bool>((lineno, s)) {
                        VmResult::Ok(v) => v,
                        VmResult::Err(e) => {
                            tracing::error!(
                                "Error in custom selector function {:?}: {:?}",
                                *guard,
                                e
                            );
                            false
                        }
                    }
                })))
            }
        }
    }
}

/// Action to perform on a line when matched by a [`Selector`]
#[derive(Debug, Any)]
#[rune(item = ::patch)]
enum Action {
    /// Copy the current line to the output. Only needed when auto-print is disabled.
    #[rune(constructor)]
    Print,
    /// Delete the current line and short circuit the rest of the program (immediately go to the next line)
    #[rune(constructor)]
    Delete,
    /// Replace pattern space with next line (will print unless auto-print is disabled)
    #[rune(constructor)]
    NextLine,
    /// Stop processing the input and program and terminate early (do not print rest of file)
    #[rune(constructor)]
    Stop,
    /// Stop processing the input and program and terminate early (auto-print rest of file)
    #[rune(constructor)]
    StopAndPrint,
    /// Insert a new line *before* the current line
    #[rune(constructor)]
    InsertBefore(#[rune(get)] String),
    /// Insert a new line *after* the current line
    #[rune(constructor)]
    InsertAfter(#[rune(get)] String),
    /// Replace the entire current string with the given string
    #[rune(constructor)]
    Replace(#[rune(get)] String),
    /// Do a regex search and replace in the current line.
    ///
    /// Only the first match is replaced in any given line.
    ///
    /// Capture groups in the replacement string works as with `::regex::Regex`.
    #[rune(constructor)]
    RegexReplace(#[rune(get)] String, #[rune(get)] String),
    /// Like `RegexReplace` but replaces all matches on the line.
    #[rune(constructor)]
    RegexReplaceAll(#[rune(get)] String, #[rune(get)] String),
    /// A sub-program that is executed. Will share pattern space with parent program
    Subprogram(LineEditor),
    /// A custom function passed the current pattern buffer, returning a new pattern buffer
    #[rune(constructor)]
    Function(#[rune(get)] Shared<rune::runtime::Function>),
}

impl Action {
    /// Create an action for a nested sub-program
    #[rune::function(path = Self::sub_program)]
    fn sub_program(sub: LineEditor) -> Self {
        Self::Subprogram(sub)
    }
}

impl TryFrom<&Action> for konfigkoll_utils::line_edit::Action {
    type Error = anyhow::Error;

    fn try_from(value: &Action) -> Result<Self, Self::Error> {
        match value {
            Action::Print => Ok(Self::Print),
            Action::Delete => Ok(Self::Delete),
            Action::Stop => Ok(Self::Stop),
            Action::StopAndPrint => Ok(Self::StopAndPrint),
            Action::InsertBefore(s) => Ok(Self::InsertBefore(s.into())),
            Action::InsertAfter(s) => Ok(Self::InsertAfter(s.into())),
            Action::Replace(s) => Ok(Self::Replace(s.into())),
            Action::RegexReplace(a, b) => Ok(Self::RegexReplace {
                regex: Regex::new(a)?,
                replacement: b.into(),
                replace_all: false,
            }),
            Action::RegexReplaceAll(a, b) => Ok(Self::RegexReplace {
                regex: Regex::new(a)?,
                replacement: b.into(),
                replace_all: true,
            }),
            Action::Function(ref f) => {
                let f = f.clone();
                Ok(Self::Function(Rc::new(move |s| {
                    let guard = f.borrow_mut().expect("Failed to borrow function object");
                    match guard.call::<_, String>((s,)) {
                        VmResult::Ok(v) => Cow::Owned(v),
                        VmResult::Err(e) => {
                            tracing::error!(
                                "Error in custom action function {:?}: {:?}",
                                *guard,
                                e
                            );
                            Cow::Borrowed(s)
                        }
                    }
                })))
            }
            Action::NextLine => Ok(Self::NextLine),
            Action::Subprogram(sub) => Ok(Self::Subprogram(sub.inner.clone())),
        }
    }
}

#[rune::module(::patch)]
/// Utilities for patching file contents conveniently.
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<LineEditor>()?;
    m.function_meta(LineEditor::new)?;
    m.function_meta(LineEditor::apply)?;
    m.function_meta(LineEditor::add)?;
    m.function_meta(LineEditor::add_inverted)?;
    m.function_meta(LineEditor::clone)?;
    m.ty::<Selector>()?;
    m.ty::<Action>()?;
    m.function_meta(Action::sub_program)?;

    Ok(m)
}
