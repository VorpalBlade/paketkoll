//! Low level parser of the file format

use crate::Id;
use crate::Mode;
use compact_str::CompactString;
use winnow::ModalResult;
use winnow::Parser;
use winnow::ascii::digit1;
use winnow::ascii::escaped;
use winnow::ascii::newline;
use winnow::ascii::space1;
use winnow::combinator::alt;
use winnow::combinator::delimited;
use winnow::combinator::opt;
use winnow::combinator::separated;
use winnow::combinator::trace;
use winnow::error::StrContext;
use winnow::stream::Accumulate;
use winnow::token::take_till;
use winnow::token::take_while;

/// Low level parser that doesn't make a difference between entry types
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Line {
    pub(super) entry_type: CompactString,
    pub(super) path: CompactString,
    pub(super) mode: Option<Mode>,
    pub(super) user: Id,
    pub(super) group: Id,
    pub(super) age: Option<CompactString>,
    pub(super) argument: Option<CompactString>,
}

/// Top level parser
pub(super) fn parse_file(i: &mut &str) -> ModalResult<Vec<Option<Line>>> {
    let alternatives = (
        comment.map(|()| None).context(StrContext::Label("comment")),
        directive.map(Some).context(StrContext::Label("directive")),
        "".map(|_| None).context(StrContext::Label("whitespace")), // Blank lines
    );
    (separated(0.., alt(alternatives), newline), opt(newline))
        .map(|(val, _)| val)
        .parse_next(i)
}

/// A comment
fn comment(i: &mut &str) -> ModalResult<()> {
    ('#', take_till(0.., ['\n', '\r'])).void().parse_next(i)
}

/// Helper to `directive` to flatten the optional tuple
fn flattener<T>(e: Option<(&str, Option<T>)>) -> Option<T> {
    e.and_then(|(_, arg)| arg)
}

fn directive(i: &mut &str) -> ModalResult<Line> {
    let entry_type = any_string.context(StrContext::Label("entry type"));
    let path = any_string.context(StrContext::Label("path"));
    let mode = mode_parser.context(StrContext::Label("mode"));
    let user = id_parser.context(StrContext::Label("user"));
    let group = id_parser.context(StrContext::Label("group"));
    let age = optional_string.context(StrContext::Label("age"));
    let argument = optional_string.context(StrContext::Label("argument"));

    let mut parser = (
        entry_type,
        space1,
        path,
        opt((space1, mode)).map(flattener),
        opt((space1, user))
            .map(flattener)
            .map(Option::unwrap_or_default),
        opt((space1, group))
            .map(flattener)
            .map(Option::unwrap_or_default),
        opt((space1, age)).map(flattener),
        opt((space1, argument)).map(flattener),
    )
        .map(
            |(entry_type, _, path, mode, user, group, age, argument)| Line {
                entry_type,
                path,
                mode,
                user,
                group,
                age,
                argument,
            },
        );
    parser.parse_next(i)
}

fn mode_parser(i: &mut &str) -> ModalResult<Option<Mode>> {
    // - for not set, numeric otherwise
    // A prefix of : indicates new only
    // A prefix of ~ indicates a mask
    let inner =
        (take_while(0.., (':', '~')), octal).map(|(prefixes, mode): (&str, libc::mode_t)| {
            Some(Mode::Set {
                mode,
                new_only: prefixes.contains(':'),
                masked: prefixes.contains('~'),
            })
        });

    alt((
        '-'.value(None),
        // Numeric
        // TODO: Can these also be quoted?
        inner,
    ))
    .parse_next(i)
}

/// Parse an octal number
fn octal(i: &mut &str) -> ModalResult<u32> {
    digit1.try_map(|e| u32::from_str_radix(e, 8)).parse_next(i)
}

/// Parse an octal number
fn decimal(i: &mut &str) -> ModalResult<u32> {
    digit1
        .try_map(|e| libc::mode_t::from_str_radix(e, 10))
        .parse_next(i)
}

fn id_parser(i: &mut &str) -> ModalResult<Option<Id>> {
    // - for not set, numeric or name
    // A prefix of : indicates new only
    alt((
        '-'.value(None),
        // Numeric
        // TODO: Can these also be quoted?
        (':', decimal).map(|(_, id): (_, u32)| Some(Id::Numeric { id, new_only: true })),
        decimal.map(|id: u32| {
            Some(Id::Numeric {
                id,
                new_only: false,
            })
        }),
        // Name
        (':', any_string).map(|(_, name)| {
            Some(Id::Name {
                name,
                new_only: true,
            })
        }),
        any_string.map(|name| {
            Some(Id::Name {
                name,
                new_only: false,
            })
        }),
    ))
    .parse_next(i)
}

fn optional_string(i: &mut &str) -> ModalResult<Option<CompactString>> {
    // - is None, otherwise string
    alt(('-'.value(None), any_string.map(Some))).parse_next(i)
}

fn any_string(i: &mut &str) -> ModalResult<CompactString> {
    trace(
        "any_string",
        alt((quoted_string, unquoted_string_with_escapes)),
    )
    .parse_next(i)
}

/// Quoted string value
fn quoted_string(i: &mut &str) -> ModalResult<CompactString> {
    delimited(
        '"',
        escaped(take_till(1.., ['"', '\\']), '\\', escapes),
        '"',
    )
    .map(|s: CompactStringWrapper| s.0)
    .parse_next(i)
}

/// Unquoted string value
fn unquoted_string_with_escapes(i: &mut &str) -> ModalResult<CompactString> {
    escaped(take_till(1.., [' ', '\t', '\n', '\r', '\\']), '\\', escapes)
        .map(|s: CompactStringWrapper| s.0)
        .parse_next(i)
}

fn escapes<'input>(i: &mut &'input str) -> ModalResult<&'input str> {
    alt((
        "n".value("\n"),
        "r".value("\r"),
        "t".value("\t"),
        " ".value(" "),
        "\"".value("\""),
        "\\".value("\\"),
    ))
    .parse_next(i)
}

/// Wrapper to get around coherence issues
#[repr(transparent)]
struct CompactStringWrapper(CompactString);

impl<'i> Accumulate<&'i str> for CompactStringWrapper {
    fn initial(capacity: Option<usize>) -> Self {
        match capacity {
            Some(capacity) => Self(CompactString::with_capacity(capacity)),
            None => Self(CompactString::new("")),
        }
    }

    fn accumulate(&mut self, acc: &'i str) {
        self.0.push_str(acc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_comment() {
        let input = "# This is a comment\n";
        let (rem, ()) = comment.parse_peek(input).unwrap();
        assert_eq!(rem, "\n");
    }

    #[test]
    fn test_any_string() {
        let (rem, out) = any_string.parse_peek("foo bar\n").unwrap();
        assert_eq!(rem, " bar\n");
        assert_eq!(out, "foo".to_string());

        let (rem, out) = any_string.parse_peek("foo\\ bar\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(out, "foo bar".to_string());

        let (rem, out) = any_string.parse_peek("\"foo bar\"\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(out, "foo bar".to_string());
    }

    #[test]
    fn test_quoted_string() {
        let (rem, out) = quoted_string.parse_peek("\"foo bar\"\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(out, "foo bar".to_string());

        let (rem, out) = quoted_string.parse_peek("\"foo\\\"bar\"\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(out, "foo\"bar".to_string());
    }

    #[test]
    fn test_unquoted_string() {
        let (rem, out) = unquoted_string_with_escapes
            .parse_peek("foo bar\n")
            .unwrap();
        assert_eq!(rem, " bar\n");
        assert_eq!(out, "foo".to_string());

        let (rem, out) = unquoted_string_with_escapes
            .parse_peek("foo\\ bar\n")
            .unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(out, "foo bar".to_string());
    }

    #[test]
    fn test_mode() {
        // Not set
        let (rem, out) = mode_parser.parse_peek("-\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(out, None);

        // Numeric
        let (rem, out) = mode_parser.parse_peek("0644\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(
            out,
            Some(Mode::Set {
                mode: 0o644,
                new_only: false,
                masked: false
            })
        );

        // Numeric with new only
        let (rem, out) = mode_parser.parse_peek(":0644\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(
            out,
            Some(Mode::Set {
                mode: 0o644,
                new_only: true,
                masked: false
            })
        );

        // Numeric with masked
        let (rem, out) = mode_parser.parse_peek("~0644\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(
            out,
            Some(Mode::Set {
                mode: 0o644,
                new_only: false,
                masked: true
            })
        );

        // Numeric with new only and masked
        let (rem, out) = mode_parser.parse_peek(":~0644\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(
            out,
            Some(Mode::Set {
                mode: 0o644,
                new_only: true,
                masked: true
            })
        );
    }

    #[test]
    fn test_id() {
        // Not set
        let (rem, out) = id_parser.parse_peek("-\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(out, None);

        // Numeric
        let (rem, out) = id_parser.parse_peek("1000\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(
            out,
            Some(Id::Numeric {
                id: 1000,
                new_only: false
            })
        );

        // Numeric with new only
        let (rem, out) = id_parser.parse_peek(":1000\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(
            out,
            Some(Id::Numeric {
                id: 1000,
                new_only: true
            })
        );

        // Name
        let (rem, out) = id_parser.parse_peek("foo\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(
            out,
            Some(Id::Name {
                name: "foo".into(),
                new_only: false
            })
        );

        // Name with new only
        let (rem, out) = id_parser.parse_peek(":foo\n").unwrap();
        assert_eq!(rem, "\n");
        assert_eq!(
            out,
            Some(Id::Name {
                name: "foo".into(),
                new_only: true
            })
        );
    }

    #[test]
    fn test_directive() {
        // Full line
        let (rest, line) = directive
            .parse_peek("L /tmp/foo 0644 - - - /tmp/target\n")
            .unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(
            line,
            Line {
                entry_type: "L".into(),
                path: "/tmp/foo".into(),
                mode: Some(Mode::Set {
                    mode: 0o644,
                    new_only: false,
                    masked: false
                }),
                user: Id::Caller { new_only: false },
                group: Id::Caller { new_only: false },
                age: None,
                argument: Some("/tmp/target".into())
            }
        );

        // Last field missing
        let (rest, line) = directive.parse_peek("L /tmp/foo 0644 - - -\n").unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(
            line,
            Line {
                entry_type: "L".into(),
                path: "/tmp/foo".into(),
                mode: Some(Mode::Set {
                    mode: 0o644,
                    new_only: false,
                    masked: false
                }),
                user: Id::Caller { new_only: false },
                group: Id::Caller { new_only: false },
                age: None,
                argument: None
            }
        );

        // Two fields missing
        let (rest, line) = directive.parse_peek("L /tmp/foo 0644 - -\n").unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(
            line,
            Line {
                entry_type: "L".into(),
                path: "/tmp/foo".into(),
                mode: Some(Mode::Set {
                    mode: 0o644,
                    new_only: false,
                    masked: false
                }),
                user: Id::Caller { new_only: false },
                group: Id::Caller { new_only: false },
                age: None,
                argument: None
            }
        );

        // Three fields missing
        let (rest, line) = directive.parse_peek("L /tmp/foo 0644 -\n").unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(
            line,
            Line {
                entry_type: "L".into(),
                path: "/tmp/foo".into(),
                mode: Some(Mode::Set {
                    mode: 0o644,
                    new_only: false,
                    masked: false
                }),
                user: Id::Caller { new_only: false },
                group: Id::Caller { new_only: false },
                age: None,
                argument: None
            }
        );

        // Four fields missing
        let (rest, line) = directive.parse_peek("L /tmp/foo 0644\n").unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(
            line,
            Line {
                entry_type: "L".into(),
                path: "/tmp/foo".into(),
                mode: Some(Mode::Set {
                    mode: 0o644,
                    new_only: false,
                    masked: false
                }),
                user: Id::Caller { new_only: false },
                group: Id::Caller { new_only: false },
                age: None,
                argument: None
            }
        );

        // Five fields missing
        let (rest, line) = directive.parse_peek("L /tmp/foo\n").unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(
            line,
            Line {
                entry_type: "L".into(),
                path: "/tmp/foo".into(),
                mode: None,
                user: Id::Caller { new_only: false },
                group: Id::Caller { new_only: false },
                age: None,
                argument: None
            }
        );

        // Partial line
        let (rest, line) = directive.parse_peek("C /var\n").unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(
            line,
            Line {
                entry_type: "C".into(),
                path: "/var".into(),
                mode: None,
                user: Id::Caller { new_only: false },
                group: Id::Caller { new_only: false },
                age: None,
                argument: None
            }
        );
    }
}
