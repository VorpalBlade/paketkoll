//! A simple streaming line editor (inspired by sed, but simplified)

use compact_str::CompactString;
use regex::Regex;
use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;
use std::str::Lines;

/// A program consists of a bunch of commands and can be applied to a string
/// line by line.
///
/// Like sed the basic algorithm is to repeatedly (until the input is consumed):
/// 1. Read a line into a "pattern space" buffer
/// 2. For each instruction in the program:
///    1. Check if selector matches the current line number and/or line contents
///    2. Apply action on the pattern space
/// 3. Append the pattern space to the output buffer
/// 4. Clear the pattern space
///
/// This means that the instructions will operate on the pattern space
/// *as changed by any previous instructions* in the program.
#[derive(Debug, Clone)]
pub struct EditProgram {
    instructions: Vec<Instruction>,
    print_default: bool,
}

impl Default for EditProgram {
    fn default() -> Self {
        Self {
            instructions: Default::default(),
            print_default: true,
        }
    }
}

impl EditProgram {
    /// Create a new empty program.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new instruction to the program.
    pub fn add(&mut self, selector: Selector, selector_invert: bool, action: Action) -> &mut Self {
        self.instructions.push(Instruction {
            selector,
            selector_invert,
            action,
        });
        self
    }

    /// Disable the default implicit action of putting the current pattern space
    /// into the output.
    pub fn disable_default_printing(&mut self) -> &mut Self {
        self.print_default = false;
        self
    }

    /// Helper to implement `NextLine` command
    fn advance_line<'lines>(
        &self,
        pattern_space: &mut String,
        output: &mut String,
        line: &mut &'lines str,
        lines: &mut Lines<'lines>,
        line_number: &mut usize,
    ) -> bool {
        if self.print_default {
            output.push_str(pattern_space);
            output.push('\n');
        }
        pattern_space.clear();
        if let Some(line_) = lines.next() {
            *line = line_;
            *line_number += 1;
            pattern_space.push_str(line);
            true
        } else {
            false
        }
    }

    /// Apply this program to the given input string.
    pub fn apply(&self, input: &str) -> String {
        let mut output = String::new();
        let mut line_number = 0;
        let mut pattern_space = String::new();
        let mut lines = input.lines();
        'input: while let Some(line) = lines.next() {
            line_number += 1;
            pattern_space.push_str(line);

            let prog_action = self.execute_program(
                &mut pattern_space,
                &mut line_number,
                line,
                &mut lines,
                &mut output,
            );
            match prog_action {
                ProgramAction::Done => (),
                ProgramAction::Stop => break 'input,
                ProgramAction::StopAndPrint => {
                    print_rest_of_input(&mut output, &mut pattern_space, &mut lines);
                    break 'input;
                }
                ProgramAction::ShortCircuit => continue 'input,
            }
            if self.print_default {
                output.push_str(&pattern_space);
                output.push('\n');
            }
            pattern_space.clear();
        }
        // Run end of file match:
        pattern_space.clear();
        for instr in &self.instructions {
            if let Selector::Eof = instr.selector {
                match instr.action.apply(&mut pattern_space, &mut output) {
                    ActionResult::Continue => (),
                    ActionResult::ShortCircuit => break,
                    ActionResult::Stop => break,
                    ActionResult::StopAndPrint => {
                        print_rest_of_input(&mut output, &mut pattern_space, &mut lines);
                        break;
                    }
                    ActionResult::NextLine => {
                        tracing::error!("NextLine not allowed in EOF selector");
                    }
                    ActionResult::Subprogram(_) => todo!(),
                }
            }
        }
        if !pattern_space.is_empty() {
            let pattern_space = if let Some(stripped) = pattern_space.strip_prefix('\n') {
                stripped
            } else {
                &pattern_space
            };
            output.push_str(pattern_space);
            if !pattern_space.ends_with('\n') {
                output.push('\n');
            }
        }
        output
    }

    fn execute_program<'lines>(
        &self,
        pattern_space: &mut String,
        line_number: &mut usize,
        mut line: &'lines str,
        lines: &mut Lines<'lines>,
        output: &mut String,
    ) -> ProgramAction {
        for instr in &self.instructions {
            if instr.matches(LineNo::Line(*line_number), line) {
                match instr.action.apply(pattern_space, output) {
                    ActionResult::Continue => (),
                    ActionResult::ShortCircuit => return ProgramAction::ShortCircuit,
                    ActionResult::Stop => return ProgramAction::Stop,
                    ActionResult::StopAndPrint => return ProgramAction::StopAndPrint,
                    ActionResult::NextLine => {
                        self.advance_line(pattern_space, output, &mut line, lines, line_number);
                    }
                    ActionResult::Subprogram(sub) => {
                        let result = sub.borrow().execute_program(
                            pattern_space,
                            line_number,
                            line,
                            lines,
                            output,
                        );
                        match result {
                            ProgramAction::Done => (),
                            ProgramAction::Stop => return ProgramAction::Stop,
                            ProgramAction::StopAndPrint => return ProgramAction::StopAndPrint,
                            // TODO: Is this the sensible semantics?
                            ProgramAction::ShortCircuit => return ProgramAction::ShortCircuit,
                        }
                    }
                }
            }
        }
        ProgramAction::Done
    }
}

fn print_rest_of_input(output: &mut String, pattern_space: &mut String, lines: &mut Lines<'_>) {
    output.push_str(&*pattern_space);
    pattern_space.clear();
    output.push('\n');
    for line in lines.by_ref() {
        output.push_str(line);
        output.push('\n');
    }
}

#[derive(Debug)]
enum ProgramAction {
    Done,
    Stop,
    StopAndPrint,
    ShortCircuit,
}

/// An instruction consists of a selector and an action.
#[derive(Debug, Clone)]
struct Instruction {
    selector: Selector,
    selector_invert: bool,
    action: Action,
}

impl Instruction {
    fn matches(&self, line_no: LineNo, line: &str) -> bool {
        let matches = self.selector.matches(line_no, line);
        if self.selector_invert {
            !matches
        } else {
            matches
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineNo {
    Line(usize),
    Eof,
}

#[derive(Clone)]
#[non_exhaustive]
pub enum Selector {
    /// Match all lines
    All,
    /// End of file (useful to insert lines at the very end)
    Eof,
    /// Match a specific line number (1-indexed)
    Line(usize),
    /// A range of line numbers (1-indexed, inclusive)
    Range(usize, usize),
    /// A regex to match the line
    Regex(Regex),
    /// A custom function, passed the line number and current line
    #[allow(clippy::type_complexity)]
    Function(Rc<dyn Fn(usize, &str) -> bool>),
}

impl Selector {
    fn matches(&self, line_no: LineNo, line: &str) -> bool {
        match self {
            Selector::All => true,
            Selector::Eof => line_no == LineNo::Eof,
            Selector::Line(v) => line_no == LineNo::Line(*v),
            Selector::Range(l, u) => match line_no {
                LineNo::Line(line_no) => line_no >= *l && line_no <= *u,
                _ => false,
            },
            Selector::Regex(re) => re.is_match(line),
            Selector::Function(func) => match line_no {
                LineNo::Line(line_no) => func(line_no, line),
                _ => false,
            },
        }
    }
}

impl Debug for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => write!(f, "All"),
            Self::Eof => write!(f, "Eof"),
            Self::Line(arg0) => f.debug_tuple("Line").field(arg0).finish(),
            Self::Range(arg0, arg1) => f.debug_tuple("Range").field(arg0).field(arg1).finish(),
            Self::Regex(arg0) => f.debug_tuple("Regex").field(arg0).finish(),
            Self::Function(_) => f.debug_tuple("Function").finish(),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub enum Action {
    /// Copy the current line to the output. Only needed when auto-print is
    /// disabled.
    Print,
    /// Delete the current line and short circuit the rest of the program
    /// (immediately go to the next line)
    Delete,
    /// Replace pattern space with next line (will print unless auto-print is
    /// disabled)
    NextLine,
    /// Stop processing the input and program and terminate early (do not print
    /// rest of file)
    Stop,
    /// Stop processing the input and program and terminate early (auto-print
    /// rest of file)
    StopAndPrint,
    /// Insert a new line *before* the current line
    InsertBefore(CompactString),
    /// Insert a new line *after* the current line
    InsertAfter(CompactString),
    /// Replace the entire current string with the given string
    Replace(CompactString),
    /// Do a regex search and replace in the current line
    ///
    /// Capture groups in the replacement string works as with
    /// [`Regex::replace`].
    RegexReplace {
        regex: Regex,
        replacement: CompactString,
        replace_all: bool,
    },
    /// A sub-program that is executed. Will share pattern space with parent
    /// program
    Subprogram(Rc<RefCell<EditProgram>>),
    /// Call a custom function to determine the new line
    #[allow(clippy::type_complexity)]
    Function(Rc<dyn Fn(&str) -> Cow<'_, str>>),
}

impl Action {
    fn apply(&self, pattern_space: &mut String, output: &mut String) -> ActionResult {
        match self {
            Action::Print => {
                output.push_str(pattern_space);
                output.push('\n');
            }
            Action::Delete => {
                pattern_space.clear();
                return ActionResult::ShortCircuit;
            }
            Action::Stop => return ActionResult::Stop,
            Action::StopAndPrint => return ActionResult::StopAndPrint,
            Action::InsertBefore(s) => {
                let old_pattern_space = std::mem::take(pattern_space);
                *pattern_space = s.to_string();
                pattern_space.push('\n');
                pattern_space.push_str(&old_pattern_space);
            }
            Action::InsertAfter(s) => {
                pattern_space.push('\n');
                pattern_space.push_str(s);
            }
            Action::Replace(s) => {
                *pattern_space = s.to_string();
            }
            Action::RegexReplace {
                regex,
                replacement,
                replace_all,
            } => {
                let ret = if *replace_all {
                    regex.replace_all(pattern_space, replacement.as_str())
                } else {
                    regex.replace(pattern_space, replacement.as_str())
                };
                match ret {
                    Cow::Borrowed(_) => (),
                    Cow::Owned(new_val) => *pattern_space = new_val,
                };
            }
            Action::Function(func) => {
                let new_val = func(pattern_space);
                *pattern_space = new_val.into_owned();
            }
            Action::NextLine => return ActionResult::NextLine,
            Action::Subprogram(prog) => return ActionResult::Subprogram(prog.clone()),
        }
        ActionResult::Continue
    }
}

impl Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Print => write!(f, "Print"),
            Self::Delete => write!(f, "Delete"),
            Self::Stop => write!(f, "Stop"),
            Self::StopAndPrint => write!(f, "StopAndPrint"),
            Self::InsertBefore(arg0) => f.debug_tuple("InsertBefore").field(arg0).finish(),
            Self::InsertAfter(arg0) => f.debug_tuple("InsertAfter").field(arg0).finish(),
            Self::Replace(arg0) => f.debug_tuple("Replace").field(arg0).finish(),
            Self::RegexReplace {
                regex,
                replacement,
                replace_all,
            } => f
                .debug_struct("RegexReplace")
                .field("regex", regex)
                .field("replacement", replacement)
                .field("replace_all", &replace_all)
                .finish(),
            Self::Function(_) => f.debug_tuple("Function").finish(),
            Self::NextLine => write!(f, "LoadNextLine"),
            Self::Subprogram(arg0) => f.debug_tuple("Subprogram").field(arg0).finish(),
        }
    }
}

#[derive(Debug, Clone)]
enum ActionResult {
    Continue,
    NextLine,
    Subprogram(Rc<RefCell<EditProgram>>),
    ShortCircuit,
    Stop,
    StopAndPrint,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_replace() {
        let mut program = EditProgram::new();
        program.add(
            Selector::All,
            false,
            Action::RegexReplace {
                regex: Regex::new("^foo$").unwrap(),
                replacement: "bar".into(),
                replace_all: false,
            },
        );
        let input = "foo\nbar\nbaz";
        let output = program.apply(input);
        assert_eq!(output, "bar\nbar\nbaz\n");
    }

    #[test]
    fn test_regex_replace_no_anchors() {
        let mut program = EditProgram::new();
        program.add(
            Selector::All,
            false,
            Action::RegexReplace {
                regex: Regex::new("foo").unwrap(),
                replacement: "bar".into(),
                replace_all: false,
            },
        );
        let input = "foo foo\nbar\nbaz";
        let output = program.apply(input);
        assert_eq!(output, "bar foo\nbar\nbaz\n");

        let mut program = EditProgram::new();
        program.add(
            Selector::All,
            false,
            Action::RegexReplace {
                regex: Regex::new("foo").unwrap(),
                replacement: "bar".into(),
                replace_all: true,
            },
        );
        let input = "foo foo\nbar\nbaz";
        let output = program.apply(input);
        assert_eq!(output, "bar bar\nbar\nbaz\n");
    }

    #[test]
    fn test_regex_replace_capture_groups() {
        let mut program = EditProgram::new();
        program.add(
            Selector::All,
            false,
            Action::RegexReplace {
                regex: Regex::new("f(a|o)o").unwrap(),
                replacement: "b${1}r".into(),
                replace_all: true,
            },
        );
        let input = "foo\nfao foo fee\nbar\nbaz";
        let output = program.apply(input);
        assert_eq!(output, "bor\nbar bor fee\nbar\nbaz\n");

        let mut program = EditProgram::new();
        program.add(
            Selector::All,
            false,
            Action::RegexReplace {
                regex: Regex::new("f(a|o)o").unwrap(),
                replacement: "b${1}r".into(),
                replace_all: false,
            },
        );
        let input = "foo\nfoo\nfao foo fee\nbar\nbaz";
        let output = program.apply(input);
        assert_eq!(output, "bor\nbor\nbar foo fee\nbar\nbaz\n");
    }

    #[test]
    fn test_insert_before() {
        let mut program = EditProgram::new();
        program.add(Selector::Line(2), false, Action::InsertBefore("foo".into()));
        let input = "bar\nbaz\nquux";
        let output = program.apply(input);
        assert_eq!(output, "bar\nfoo\nbaz\nquux\n");
    }

    #[test]
    fn test_insert_after() {
        let mut program = EditProgram::new();
        program.add(
            Selector::Regex(Regex::new("^q").unwrap()),
            false,
            Action::InsertAfter("foo".into()),
        );
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "bar\nbaz\nquux\nfoo\nquack\nfoo\n");
    }

    #[test]
    fn test_replace() {
        let mut program = EditProgram::new();
        program.add(Selector::Range(2, 3), false, Action::Replace("foo".into()));
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "bar\nfoo\nfoo\nquack\n");

        // Test inverted selector
        let mut program = EditProgram::new();
        program.add(Selector::Range(2, 3), true, Action::Replace("foo".into()));
        let output = program.apply(input);
        assert_eq!(output, "foo\nbaz\nquux\nfoo\n");
    }

    #[test]
    fn test_function() {
        let mut program = EditProgram::new();
        program.add(
            Selector::All,
            false,
            Action::Function(Rc::new(|line| {
                if line == "bar" {
                    Cow::Borrowed("baz")
                } else {
                    Cow::Borrowed(line)
                }
            })),
        );
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "baz\nbaz\nquux\nquack\n");
    }

    #[test]
    fn test_selector_function() {
        let mut program = EditProgram::new();
        program.disable_default_printing();
        program.add(
            Selector::Function(Rc::new(|line_no, _line| line_no % 2 == 0)),
            false,
            Action::Print,
        );
        let input = "bar\nbaz\nquux\nquack\nhuzza\nbar";
        let output = program.apply(input);
        assert_eq!(output, "baz\nquack\nbar\n");

        let mut program = EditProgram::new();
        program.add(
            Selector::Function(Rc::new(|line_no, _line| line_no % 2 == 0)),
            false,
            Action::Delete,
        );
        let input = "bar\nbaz\nquux\nquack\nhuzza\nbar";
        let output = program.apply(input);
        assert_eq!(output, "bar\nquux\nhuzza\n");
    }

    #[test]
    fn test_delete() {
        let mut program = EditProgram::new();
        program.add(Selector::Line(2), false, Action::Delete);
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "bar\nquux\nquack\n");

        // Test inverted selector
        let mut program = EditProgram::new();
        program.add(
            Selector::Regex(Regex::new("x$").unwrap()),
            true,
            Action::Delete,
        );
        let output = program.apply(input);
        assert_eq!(output, "quux\n");
    }

    #[test]
    fn test_stop() {
        let mut program = EditProgram::new();
        program.add(
            Selector::Regex(Regex::new("x").unwrap()),
            false,
            Action::Stop,
        );
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "bar\nbaz\n");

        let mut program = EditProgram::new();
        program.add(Selector::All, false, Action::Replace("foo".into()));
        program.add(
            Selector::Regex(Regex::new("x").unwrap()),
            false,
            Action::Stop,
        );
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "foo\nfoo\n");
    }

    #[test]
    fn test_stop_and_print() {
        let mut program = EditProgram::new();
        program.add(
            Selector::Regex(Regex::new("x").unwrap()),
            false,
            Action::StopAndPrint,
        );
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "bar\nbaz\nquux\nquack\n");

        let mut program = EditProgram::new();
        program.add(Selector::All, false, Action::Replace("foo".into()));
        program.add(
            Selector::Regex(Regex::new("x").unwrap()),
            false,
            Action::StopAndPrint,
        );
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "foo\nfoo\nfoo\nquack\n");
    }

    #[test]
    fn test_print() {
        let mut program = EditProgram::new();
        program.disable_default_printing();
        program.add(Selector::Range(2, 3), false, Action::Print);
        program.add(Selector::Range(3, 4), false, Action::Print);
        let input = "bar\nbaz\nquux\nquack\nhuzza";
        let output = program.apply(input);
        assert_eq!(output, "baz\nquux\nquux\nquack\n");
    }

    #[test]
    fn test_eof() {
        let mut program = EditProgram::new();
        program.add(Selector::Eof, false, Action::InsertBefore("foo".into()));
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "bar\nbaz\nquux\nquack\nfoo\n");

        let mut program = EditProgram::new();
        program.add(Selector::Eof, false, Action::InsertAfter("foo".into()));
        program.add(Selector::Eof, false, Action::InsertAfter("bar".into()));
        let input = "bar\nbaz\nquux\nquack";
        let output = program.apply(input);
        assert_eq!(output, "bar\nbaz\nquux\nquack\nfoo\nbar\n");
    }

    #[test]
    fn test_subprogram() {
        let mut subprogram = EditProgram::new();
        subprogram.add(Selector::All, false, Action::Replace("foo".into()));
        subprogram.add(Selector::All, false, Action::NextLine);
        subprogram.add(Selector::All, false, Action::Replace("bar".into()));
        let mut program = EditProgram::new();
        program.add(
            Selector::Regex(Regex::new("quux").unwrap()),
            false,
            Action::Subprogram(Rc::new(RefCell::new(subprogram))),
        );
        let input = "bar\nquux\nquack\nx\ny";
        let output = program.apply(input);
        assert_eq!(output, "bar\nfoo\nbar\nx\ny\n");
    }
}
