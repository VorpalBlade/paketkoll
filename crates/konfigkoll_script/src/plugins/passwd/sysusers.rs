//! Parser for systemd sysusers.d files.

use compact_str::CompactString;
use winnow::ModalResult;
use winnow::Parser;
use winnow::ascii::dec_uint;
use winnow::ascii::escaped;
use winnow::ascii::newline;
use winnow::ascii::space1;
use winnow::combinator::alt;
use winnow::combinator::delimited;
use winnow::combinator::opt;
use winnow::combinator::separated;
use winnow::combinator::trace;
use winnow::error::ContextError;
use winnow::error::StrContext;
use winnow::stream::Accumulate;
use winnow::token::take_till;

/// Sub-error type for the first splitting layer
#[derive(Debug, PartialEq)]
pub(super) struct SysusersParseError {
    message: String,
    span: std::ops::Range<usize>,
    input: String,
}

impl SysusersParseError {
    pub(super) fn from_parse<'input>(
        error: &winnow::error::ParseError<&'input str, ContextError>,
        input: &'input str,
    ) -> Self {
        let message = error.inner().to_string();
        let input = input.to_owned();
        let start = error.offset();
        let end = (start + 1..)
            .find(|e| input.is_char_boundary(*e))
            .unwrap_or(start);
        Self {
            message,
            span: start..end,
            input,
        }
    }
}

impl std::fmt::Display for SysusersParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = annotate_snippets::Level::Error
            .title(&self.message)
            .snippet(
                annotate_snippets::Snippet::source(&self.input)
                    .fold(true)
                    .annotation(annotate_snippets::Level::Error.span(self.span.clone())),
            );
        let renderer = annotate_snippets::Renderer::plain();
        let rendered = renderer.render(message);
        rendered.fmt(f)
    }
}

impl std::error::Error for SysusersParseError {}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Directive {
    Comment,
    User(User),
    Group(Group),
    AddUserToGroup {
        user: CompactString,
        group: CompactString,
    },
    SetRange(u32, u32),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct User {
    pub name: CompactString,
    pub id: Option<UserId>,
    pub gecos: Option<CompactString>,
    pub home: Option<CompactString>,
    pub shell: Option<CompactString>,
    pub locked: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum UserId {
    Uid(u32),
    UidGid(u32, u32),
    UidGroup(u32, CompactString),
    FromPath(CompactString),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum GroupId {
    Gid(u32),
    FromPath(CompactString),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Group {
    pub name: CompactString,
    pub id: Option<GroupId>,
}

/// Top level parser
pub(super) fn parse_file(i: &mut &str) -> ModalResult<Vec<Directive>> {
    let alternatives = (
        comment
            .map(|()| Directive::Comment)
            .context(StrContext::Label("comment")),
        user.context(StrContext::Label("user")),
        group.context(StrContext::Label("group")),
        add_to_group.context(StrContext::Label("add_to_group")),
        set_range.context(StrContext::Label("set_range")),
        "".map(|_| Directive::Comment)
            .context(StrContext::Label("whitespace")), // Blank lines
    );
    (separated(0.., alt(alternatives), newline), opt(newline))
        .map(|(val, _)| val)
        .parse_next(i)
}

/// Helper to `directive` to flatten the optional tuple
fn flattener<T>(e: Option<(&str, Option<T>)>) -> Option<T> {
    e.and_then(|(_, arg)| arg)
}

fn user(i: &mut &str) -> ModalResult<Directive> {
    let entry_type = 'u';
    let locked = opt('!').context(StrContext::Label("locked"));
    let user_name = any_string.context(StrContext::Label("user_name"));
    let id = user_id_parser.context(StrContext::Label("id"));
    let gecos = optional_string.context(StrContext::Label("gecos"));
    let home_dir = optional_string.context(StrContext::Label("home"));
    let shell = optional_string.context(StrContext::Label("shell"));

    let mut parser = (
        entry_type,
        locked,
        space1,
        user_name,
        opt((space1, id)).map(flattener),
        opt((space1, gecos)).map(flattener),
        opt((space1, home_dir)).map(flattener),
        opt((space1, shell)).map(flattener),
    )
        .map(|(_, maybe_locked, _, name, id, gecos, home, shell)| {
            Directive::User(User {
                name,
                id,
                gecos,
                home,
                shell,
                locked: maybe_locked.is_some(),
            })
        });
    parser.parse_next(i)
}

fn group(i: &mut &str) -> ModalResult<Directive> {
    let entry_type = 'g';
    let path = any_string.context(StrContext::Label("group_name"));
    let id = group_id_parser.context(StrContext::Label("id"));

    let mut parser = (
        entry_type,
        space1,
        path,
        opt((space1, id)).map(flattener),
        opt((space1, '-')),
        opt((space1, '-')),
    )
        .map(|(_, _, name, id, _, _)| Directive::Group(Group { name, id }));
    parser.parse_next(i)
}

fn add_to_group(i: &mut &str) -> ModalResult<Directive> {
    let entry_type = 'm';
    let user = any_string.context(StrContext::Label("user_name"));
    let group = any_string.context(StrContext::Label("group_name"));

    let mut parser = (entry_type, space1, user, space1, group)
        .map(|(_, _, user, _, group)| Directive::AddUserToGroup { user, group });
    parser.parse_next(i)
}

fn set_range(i: &mut &str) -> ModalResult<Directive> {
    let entry_type = 'r';
    let name = '-';
    let range = range_parser.context(StrContext::Label("range"));

    let mut parser = (entry_type, space1, name, space1, range)
        .map(|(_, _, _, _, range)| Directive::SetRange(range.0, range.1));
    parser.parse_next(i)
}

fn user_id_parser(i: &mut &str) -> ModalResult<Option<UserId>> {
    let mut parser = alt((
        ('-').map(|_| None),
        (dec_uint, ':', dec_uint).map(|(uid, _, gid)| Some(UserId::UidGid(uid, gid))),
        (dec_uint, ':', name).map(|(uid, _, group)| Some(UserId::UidGroup(uid, group))),
        (dec_uint).map(|uid| Some(UserId::Uid(uid))),
        name.map(|path| Some(UserId::FromPath(path))),
    ));
    parser.parse_next(i)
}
fn group_id_parser(i: &mut &str) -> ModalResult<Option<GroupId>> {
    let mut parser = alt((
        ('-').map(|_| None),
        (dec_uint).map(|id| Some(GroupId::Gid(id))),
        name.map(|path| Some(GroupId::FromPath(path))),
    ));
    parser.parse_next(i)
}

fn range_parser(i: &mut &str) -> ModalResult<(u32, u32)> {
    let mut parser = (dec_uint, '-', dec_uint).map(|(start, _, end)| (start, end));
    parser.parse_next(i)
}

/// A comment
fn comment(i: &mut &str) -> ModalResult<()> {
    ('#', take_till(0.., ['\n', '\r'])).void().parse_next(i)
}

fn optional_string(i: &mut &str) -> ModalResult<Option<CompactString>> {
    // - is None, otherwise string
    alt(('-'.value(None), any_string.map(Some))).parse_next(i)
}

fn any_string(i: &mut &str) -> ModalResult<CompactString> {
    trace(
        "any_string",
        alt((
            quoted_string,
            single_quoted_string,
            unquoted_string_with_escapes,
        )),
    )
    .parse_next(i)
}

/// Quoted string value
fn single_quoted_string(i: &mut &str) -> ModalResult<CompactString> {
    delimited(
        '\'',
        escaped(take_till(1.., ['\'', '\\']), '\\', escapes),
        '\'',
    )
    .map(|s: CompactStringWrapper| s.0)
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

/// A valid name
fn name(i: &mut &str) -> ModalResult<CompactString> {
    take_till(1.., [' ', '\t', '\n', '\r'])
        .map(CompactString::from)
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
        let input = "# This is a comment\nblah";
        let (rest, ()) = comment.parse_peek(input).unwrap();
        assert_eq!(rest, "\nblah");
    }

    #[test]
    fn test_user() {
        let input = "u user 1000:2000 \"GECOS quux\" /home/user /bin/bash\n";
        let expected = Directive::User(User {
            name: "user".into(),
            id: Some(UserId::UidGid(1000, 2000)),
            gecos: Some("GECOS quux".into()),
            home: Some("/home/user".into()),
            shell: Some("/bin/bash".into()),
            locked: false,
        });
        let (rest, result) = user.parse_peek(input).unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_locked_user() {
        let input = "u! user 1000:2000 \"GECOS quux\" /home/user /bin/bash\n";
        let expected = Directive::User(User {
            name: "user".into(),
            id: Some(UserId::UidGid(1000, 2000)),
            gecos: Some("GECOS quux".into()),
            home: Some("/home/user".into()),
            shell: Some("/bin/bash".into()),
            locked: true,
        });
        let (rest, result) = user.parse_peek(input).unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_group() {
        let input = "g group 1000 - -\n";
        let expected = Directive::Group(Group {
            name: "group".into(),
            id: Some(GroupId::Gid(1000)),
        });
        let (rest, result) = group.parse_peek(input).unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(result, expected);

        let input = "g group -\n";
        let expected = Directive::Group(Group {
            name: "group".into(),
            id: None,
        });
        let (rest, result) = group.parse_peek(input).unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(result, expected);

        let input = "g group /path/to/group\n";
        let expected = Directive::Group(Group {
            name: "group".into(),
            id: Some(GroupId::FromPath("/path/to/group".into())),
        });
        let (rest, result) = group.parse_peek(input).unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_add_to_group() {
        let input = "m user group\n";
        let expected = Directive::AddUserToGroup {
            user: "user".into(),
            group: "group".into(),
        };
        let (rest, result) = add_to_group.parse_peek(input).unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_set_range() {
        let input = "r - 500-999\n";
        let expected = Directive::SetRange(500, 999);
        let (rest, result) = set_range.parse_peek(input).unwrap();
        assert_eq!(rest, "\n");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_file() {
        let input = indoc::indoc!(
            r#"# This is a comment
            u user 1000:2000 "GECOS quux" /home/user /bin/bash
            u user 1001 "GECOS bar"
            g group 1000
            m user group
            r - 500-999
            "#
        );
        let expected = vec![
            Directive::Comment,
            Directive::User(User {
                name: "user".into(),
                id: Some(UserId::UidGid(1000, 2000)),
                gecos: Some("GECOS quux".into()),
                home: Some("/home/user".into()),
                shell: Some("/bin/bash".into()),
                locked: false,
            }),
            Directive::User(User {
                name: "user".into(),
                id: Some(UserId::Uid(1001)),
                gecos: Some("GECOS bar".into()),
                home: None,
                shell: None,
                locked: false,
            }),
            Directive::Group(Group {
                name: "group".into(),
                id: Some(GroupId::Gid(1000)),
            }),
            Directive::AddUserToGroup {
                user: "user".into(),
                group: "group".into(),
            },
            Directive::SetRange(500, 999),
            Directive::Comment,
        ];
        let (rest, result) = parse_file.parse_peek(input).unwrap();
        assert_eq!(rest, "");
        assert_eq!(result, expected);
    }
}
