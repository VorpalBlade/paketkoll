//! Actual parser for file format

use crate::types::DeviceNode;
use crate::types::Directive;
use crate::types::Entry;
use compact_str::CompactString;
use compact_str::ToCompactString;
use file::Line;
use smallvec::SmallVec;
use winnow::Parser;
use winnow::error::ContextError;

mod file;

/// Overall parser error
#[derive(Debug, thiserror::Error, PartialEq)]
#[non_exhaustive]
pub enum ParseError {
    #[error("Error parsing file: {0}")]
    SplitterError(#[from] SplitterError),
    #[error("Invalid directive: {1} when parsing {0}")]
    InvalidDirective(CompactString, CompactString),
    #[error("Missing required field {1} when parsing {0}")]
    MissingField(CompactString, i32),
    #[error("Invalid format for field {1} when parsing {0}")]
    InvalidField(CompactString, i32),
    #[error("Error parsing base64: {0}")]
    Base64Error(CompactString),
}

/// Sub-error type for the first splitting layer
#[derive(Debug, PartialEq, Eq)]
pub struct SplitterError {
    message: String,
    pos: usize,
    input: String,
}

impl SplitterError {
    fn from_parse<'input>(
        error: &winnow::error::ParseError<&'input str, ContextError>,
        input: &'input str,
    ) -> Self {
        let message = error.inner().to_string();
        let input = input.to_owned();
        Self {
            message,
            pos: error.offset(),
            input,
        }
    }
}

impl std::fmt::Display for SplitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pos = self.pos;
        let input = &self.input;
        let message = &self.message;
        write!(
            f,
            "Error at position {}: {}\n{}\n{}^",
            pos,
            message,
            &input[..pos],
            " ".repeat(pos)
        )
    }
}

impl std::error::Error for SplitterError {}

/// Parse the tmpfiles.d format from a string
pub fn parse_str(input: &str) -> Result<Vec<Entry>, ParseError> {
    let first_layer = file::parse_file
        .parse(input)
        .map_err(|e| SplitterError::from_parse(&e, input))?;

    let first_layer = first_layer.into_iter().flatten();
    let second_layer: Result<Vec<_>, _> = first_layer.map(parse_directive).collect();
    second_layer
}

fn parse_directive(line: Line) -> Result<Entry, ParseError> {
    let entry_type = &line.entry_type;
    let (type_, plus, mut flags) = parse_entry_type(entry_type.as_str())
        .ok_or_else(|| ParseError::InvalidDirective(line.path.clone(), entry_type.clone()))?;

    // Decode base64 (if applicable)
    let argument = if flags.contains(super::EntryFlags::ARG_BASE64)
        && !flags.contains(super::EntryFlags::ARG_CREDENTIAL)
    {
        // We decode it already, only leave it in when the credential is encoded
        flags.remove(super::EntryFlags::ARG_BASE64);
        line.argument
            .map(|a| {
                let mut a = a.into_bytes();
                match base64_simd::STANDARD.decode_inplace(a.as_mut_slice()) {
                    Ok(_) => Ok(a),
                    Err(e) => Err(ParseError::Base64Error(e.to_compact_string())),
                }
            })
            .transpose()?
    } else {
        line.argument.map(CompactString::into_bytes)
    }
    .map(SmallVec::into_boxed_slice);

    let cleanup_age = line.age.map(|a| super::Age { specifier: a });

    let directive = match (type_, plus) {
        ('f', plus) => Directive::CreateFile {
            truncate_if_exists: plus,
            mode: line.mode,
            user: line.user,
            group: line.group,
            contents: argument,
        },
        // This is a deprecated alias for f+
        ('F', _) => Directive::CreateFile {
            truncate_if_exists: true,
            mode: line.mode,
            user: line.user,
            group: line.group,
            contents: argument,
        },
        ('w', plus) => Directive::WriteToFile {
            append: plus,
            contents: argument.ok_or_else(|| ParseError::MissingField(entry_type.clone(), 7))?,
        },
        ('d', false) => Directive::CreateDirectory {
            remove_if_exists: false,
            mode: line.mode,
            user: line.user,
            group: line.group,
            cleanup_age,
        },
        ('D', false) => Directive::CreateDirectory {
            remove_if_exists: true,
            mode: line.mode,
            user: line.user,
            group: line.group,
            cleanup_age,
        },
        ('e', false) => Directive::AdjustPermissionsAndTmpFiles {
            mode: line.mode,
            user: line.user,
            group: line.group,
            cleanup_age,
        },
        ('v', false) => Directive::CreateSubvolume {
            quota: None,
            mode: line.mode,
            user: line.user,
            group: line.group,
            cleanup_age,
        },
        ('q', false) => Directive::CreateSubvolume {
            quota: Some(crate::SubvolumeQuota::Inherit),
            mode: line.mode,
            user: line.user,
            group: line.group,
            cleanup_age,
        },
        ('Q', false) => Directive::CreateSubvolume {
            quota: Some(crate::SubvolumeQuota::New),
            mode: line.mode,
            user: line.user,
            group: line.group,
            cleanup_age,
        },
        ('p', plus) => Directive::CreateFifo {
            replace_if_exists: plus,
            mode: line.mode,
            user: line.user,
            group: line.group,
        },
        ('L', plus) => Directive::CreateSymlink {
            replace_if_exists: plus,
            target: argument,
        },
        ('c', plus) => Directive::CreateCharDeviceNode {
            replace_if_exists: plus,
            mode: line.mode,
            user: line.user,
            group: line.group,
            device_specifier: argument
                .and_then(|e| DeviceNode::try_from_bytes(&e))
                .ok_or_else(|| ParseError::InvalidField(entry_type.clone(), 7))?,
        },
        ('b', plus) => Directive::CreateBlockDeviceNode {
            replace_if_exists: plus,
            mode: line.mode,
            user: line.user,
            group: line.group,
            device_specifier: argument
                .and_then(|e| DeviceNode::try_from_bytes(&e))
                .ok_or_else(|| ParseError::InvalidField(entry_type.clone(), 7))?,
        },
        ('C', plus) => Directive::RecursiveCopy {
            recursive_if_exists: plus,
            cleanup_age,
            source: argument
                .map(CompactString::from_utf8)
                .transpose()
                .map_err(|_| ParseError::InvalidField(entry_type.clone(), 7))?,
        },
        ('x', false) => Directive::IgnorePathDuringCleaning { cleanup_age },
        ('X', false) => Directive::IgnoreDirectoryDuringCleaning { cleanup_age },
        ('r', false) => Directive::RemoveFile { recursive: false },
        ('R', false) => Directive::RemoveFile { recursive: true },
        // m is a legacy alias for z
        ('z' | 'm', false) => Directive::AdjustAccess {
            recursive: false,
            mode: line.mode,
            user: line.user,
            group: line.group,
        },
        ('Z', false) => Directive::AdjustAccess {
            recursive: true,
            mode: line.mode,
            user: line.user,
            group: line.group,
        },
        ('t', false) => Directive::SetExtendedAttributes {
            recursive: false,
            attributes: argument.ok_or_else(|| ParseError::MissingField(entry_type.clone(), 7))?,
        },
        ('T', false) => Directive::SetExtendedAttributes {
            recursive: true,
            attributes: argument.ok_or_else(|| ParseError::MissingField(entry_type.clone(), 7))?,
        },
        ('h', false) => Directive::SetAttributes {
            recursive: false,
            attributes: argument.ok_or_else(|| ParseError::MissingField(entry_type.clone(), 7))?,
        },
        ('H', false) => Directive::SetAttributes {
            recursive: true,
            attributes: argument.ok_or_else(|| ParseError::MissingField(entry_type.clone(), 7))?,
        },
        ('a', plus) => Directive::SetAcl {
            recursive: false,
            append: plus,
            acls: argument.ok_or_else(|| ParseError::MissingField(entry_type.clone(), 7))?,
        },
        ('A', plus) => Directive::SetAcl {
            recursive: true,
            append: plus,
            acls: argument.ok_or_else(|| ParseError::MissingField(entry_type.clone(), 7))?,
        },
        _ => {
            return Err(ParseError::InvalidDirective(
                line.path.clone(),
                entry_type.clone(),
            ))?;
        }
    };
    Ok(Entry {
        path: line.path,
        directive,
        flags,
    })
}

/// Split into type and modifiers
fn parse_entry_type(entry: &str) -> Option<(char, bool, super::EntryFlags)> {
    let type_idx = entry.find(|e: char| e.is_alphabetic())?;
    let type_chr = entry.chars().nth(type_idx)?;
    let mut flags = if entry.contains('-') {
        super::EntryFlags::ERRORS_OK_ON_CREATE
    } else {
        super::EntryFlags::empty()
    };
    flags |= if entry.contains('!') {
        super::EntryFlags::BOOT_ONLY
    } else {
        super::EntryFlags::empty()
    };
    flags |= if entry.contains('=') {
        super::EntryFlags::REMOVE_NONMATCHING
    } else {
        super::EntryFlags::empty()
    };
    flags |= if entry.contains('~') {
        super::EntryFlags::ARG_BASE64
    } else {
        super::EntryFlags::empty()
    };
    flags |= if entry.contains('^') {
        super::EntryFlags::ARG_CREDENTIAL
    } else {
        super::EntryFlags::empty()
    };

    Some((type_chr, entry.contains('+'), flags))
}

#[cfg(test)]
mod tests {
    use crate::Age;
    use crate::Entry;
    use crate::Id;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse_entry_type() {
        let entry = "d+";
        let (entry_type, plus, flags) = super::parse_entry_type(entry).unwrap();
        assert_eq!(entry_type, 'd');
        assert_eq!(plus, true);
        assert_eq!(flags, super::super::EntryFlags::empty());

        let entry = "f-=";
        let (entry_type, plus, flags) = super::parse_entry_type(entry).unwrap();
        assert_eq!(entry_type, 'f');
        assert_eq!(plus, false);
        assert_eq!(
            flags,
            super::super::EntryFlags::ERRORS_OK_ON_CREATE
                | super::super::EntryFlags::REMOVE_NONMATCHING
        );

        let entry = "!~c^";
        let (entry_type, plus, flags) = super::parse_entry_type(entry).unwrap();
        assert_eq!(entry_type, 'c');
        assert_eq!(plus, false);
        assert_eq!(
            flags,
            super::super::EntryFlags::BOOT_ONLY
                | super::super::EntryFlags::ARG_BASE64
                | super::super::EntryFlags::ARG_CREDENTIAL
        );
    }

    #[test]
    fn test_parse() {
        // Some test lines from a real system
        let input = indoc::indoc! {r"
            # Comment
            D! /tmp/.X11-unix 1777 root root 10d
            d /var/cache 0755 - - -
            q /var/tmp 1777 root root 30d
            Q /var/lib/machines 0700 - - -
            C /run/systemd/tpm2-pcr-signature.json 0444 root root - /.extra/tpm2-pcr-signature.json
            Z /run/log/journal/%m ~2750 root systemd-journal - -
            f^ /etc/motd.d/50-provision.conf - - - - login.motd
            z /dev/snd/seq      0660 - audio -
            a+ /var/log/journal    - - - - d:group::r-x,d:group:adm:r-x,d:group:wheel:r-x,group::r-x,group:adm:r-x,group:wheel:r-x
            L+  %t/docker.sock   -    -    -     -   %t/podman/podman.sock
        "};
        let parsed = super::parse_str(input).unwrap();
        assert_eq!(parsed.len(), 10);
        assert_eq!(
            parsed[0],
            Entry {
                path: "/tmp/.X11-unix".into(),
                directive: super::Directive::CreateDirectory {
                    remove_if_exists: true,
                    user: Id::Name {
                        name: "root".into(),
                        new_only: false
                    },
                    group: Id::Name {
                        name: "root".into(),
                        new_only: false
                    },
                    cleanup_age: Some(Age {
                        specifier: "10d".into()
                    }),
                    mode: Some(crate::Mode::Set {
                        mode: 0o1777,
                        new_only: false,
                        masked: false,
                    }),
                },
                flags: super::super::EntryFlags::BOOT_ONLY,
            }
        );
        assert_eq!(
            parsed[1],
            Entry {
                path: "/var/cache".into(),
                directive: super::Directive::CreateDirectory {
                    remove_if_exists: false,
                    user: Id::Caller { new_only: false },
                    group: Id::Caller { new_only: false },
                    cleanup_age: None,
                    mode: Some(crate::Mode::Set {
                        mode: 0o755,
                        new_only: false,
                        masked: false,
                    }),
                },
                flags: super::super::EntryFlags::empty(),
            }
        );
    }
}
