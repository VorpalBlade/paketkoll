//! Parser for pci.ids

use ahash::AHashMap;
use winnow::{
    ascii::{hex_uint, newline, space1},
    combinator::{alt, opt, separated, trace},
    error::{ContextError, StrContext},
    stream::AsChar,
    token::{take, take_until},
    PResult, Parser,
};

use super::{Class, ProgrammingInterface, Subclass};

#[derive(Debug, PartialEq, Eq)]
enum Line<'input> {
    Class(ClassLine<'input>),
    Subclass(SubclassLine<'input>),
    ProgrammingInterface(ProgrammingInterfaceLine<'input>),

    Vendor(VendorLine<'input>),
    Device(DeviceLine<'input>),
    Subsystem(SubsystemLine<'input>),
}

#[derive(Debug, PartialEq, Eq)]
struct VendorLine<'input> {
    id: u16,
    name: &'input str,
}

#[derive(Debug, PartialEq, Eq)]
struct DeviceLine<'input> {
    id: u16,
    name: &'input str,
}

#[derive(Debug, PartialEq, Eq)]
struct SubsystemLine<'input> {
    subvendor: u16,
    subdevice: u16,
    name: &'input str,
}

#[derive(Debug, PartialEq, Eq)]
struct ClassLine<'input> {
    id: u8,
    name: &'input str,
}

#[derive(Debug, PartialEq, Eq)]
struct SubclassLine<'input> {
    id: u8,
    name: &'input str,
}

#[derive(Debug, PartialEq, Eq)]
struct ProgrammingInterfaceLine<'input> {
    id: u8,
    name: &'input str,
}

/// Sub-error type for the first splitting layer
#[derive(Debug, PartialEq)]
pub struct ParsePciError {
    message: String,
    pos: usize,
    input: String,
}

impl ParsePciError {
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

impl std::fmt::Display for ParsePciError {
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

impl std::error::Error for ParsePciError {}

pub(super) fn parse_pcidatabase(input: &str) -> anyhow::Result<super::PciIdDb> {
    let lines = parse_file
        .parse(input)
        .map_err(|error| ParsePciError::from_parse(&error, input))?;
    build_hierarchy(&lines)
}

/// This function takes the line-by-line parsed data and builds a hierarchical
/// structure from it.
///
/// We either need to keep a cursor into the structure we are building (ouch in
/// Rust), or we need a lookahead of 1 line to determine when to go up a level.
/// We do the latter, using [`itertools::put_back`].
fn build_hierarchy(lines: &[Line<'_>]) -> anyhow::Result<super::PciIdDb> {
    let mut db = super::PciIdDb {
        classes: Default::default(),
        vendors: Default::default(),
    };

    let mut lines = itertools::put_back(lines.iter());

    while let Some(line) = lines.next() {
        match line {
            Line::Class(class) => {
                let mut subclasses = AHashMap::new();
                while let Some(line) = lines.next() {
                    match line {
                        Line::Subclass(subclass) => {
                            let mut prog_ifs = AHashMap::new();
                            while let Some(line) = lines.next() {
                                match line {
                                    Line::ProgrammingInterface(prog_if) => {
                                        prog_ifs.insert(
                                            prog_if.id,
                                            ProgrammingInterface {
                                                name: prog_if.name.to_string(),
                                            },
                                        );
                                    }
                                    _ => {
                                        lines.put_back(line);
                                        break;
                                    }
                                }
                            }
                            subclasses.insert(
                                subclass.id,
                                Subclass {
                                    name: subclass.name.to_string(),
                                    program_interfaces: prog_ifs,
                                },
                            );
                        }
                        _ => {
                            lines.put_back(line);
                            break;
                        }
                    }
                }
                db.classes.insert(
                    class.id,
                    Class {
                        name: class.name.to_string(),
                        subclasses,
                    },
                );
            }
            Line::Vendor(vendor) => {
                let mut devices = AHashMap::new();
                while let Some(line) = lines.next() {
                    match line {
                        Line::Device(device) => {
                            let mut subsystems = AHashMap::new();
                            while let Some(line) = lines.next() {
                                match line {
                                    Line::Subsystem(subsystem) => {
                                        subsystems.insert(
                                            (subsystem.subvendor, subsystem.subdevice),
                                            super::Subsystem {
                                                name: subsystem.name.to_string(),
                                            },
                                        );
                                    }
                                    _ => {
                                        lines.put_back(line);
                                        break;
                                    }
                                }
                            }
                            devices.insert(
                                device.id,
                                super::Device {
                                    name: device.name.to_string(),
                                    subsystems,
                                },
                            );
                        }
                        _ => {
                            lines.put_back(line);
                            break;
                        }
                    }
                }
                db.vendors.insert(
                    vendor.id,
                    super::Vendor {
                        name: vendor.name.to_string(),
                        devices,
                    },
                );
            }
            Line::Subclass(_)
            | Line::ProgrammingInterface(_)
            | Line::Device(_)
            | Line::Subsystem(_) => anyhow::bail!("Unexpected line at top level: {line:?}"),
        }
    }

    Ok(db)
}

fn parse_file<'input>(i: &mut &'input str) -> PResult<Vec<Line<'input>>> {
    let alternatives = (
        comment.map(|_| None).context(StrContext::Label("comment")),
        // Vendor hierarchy
        vendor
            .map(|v| Some(Line::Vendor(v)))
            .context(StrContext::Label("vendor")),
        device
            .map(|d| Some(Line::Device(d)))
            .context(StrContext::Label("device")),
        subsystem
            .map(|s| Some(Line::Subsystem(s)))
            .context(StrContext::Label("subsystem")),
        // Class hierarchy
        class
            .map(|c| Some(Line::Class(c)))
            .context(StrContext::Label("class")),
        sub_class
            .map(|c| Some(Line::Subclass(c)))
            .context(StrContext::Label("subclass")),
        prog_if
            .map(|c| Some(Line::ProgrammingInterface(c)))
            .context(StrContext::Label("prog_if")),
        "".map(|_| None).context(StrContext::Label("whitespace")), // Blank lines, must be last
    );
    (separated(0.., alt(alternatives), newline), opt(newline))
        .map(|(val, _): (Vec<_>, _)| {
            // Filter
            val.into_iter().flatten().collect()
        })
        .parse_next(i)
}

/// A comment
fn comment(i: &mut &str) -> PResult<()> {
    ('#', take_until(0.., '\n')).void().parse_next(i)
}

fn device<'input>(i: &mut &'input str) -> PResult<DeviceLine<'input>> {
    let parser = ('\t', hex4, space1, string).map(|(_, id, _, name)| DeviceLine { id, name });
    trace("device", parser).parse_next(i)
}

fn vendor<'input>(i: &mut &'input str) -> PResult<VendorLine<'input>> {
    let parser = (hex4, space1, string).map(|(id, _, name)| VendorLine { id, name });
    trace("vendor", parser).parse_next(i)
}

fn subsystem<'input>(i: &mut &'input str) -> PResult<SubsystemLine<'input>> {
    let parser = ("\t\t", hex4, space1, hex4, space1, string).map(
        |(_, subvendor, _, subdevice, _, name)| SubsystemLine {
            subvendor,
            subdevice,
            name,
        },
    );
    trace("subsystem", parser).parse_next(i)
}

fn prog_if<'input>(i: &mut &'input str) -> PResult<ProgrammingInterfaceLine<'input>> {
    let parser = ("\t\t", hex2, space1, string)
        .map(|(_, id, _, name)| ProgrammingInterfaceLine { id, name });
    trace("prog_if", parser).parse_next(i)
}

fn sub_class<'input>(i: &mut &'input str) -> PResult<SubclassLine<'input>> {
    let parser = ('\t', hex2, space1, string).map(|(_, id, _, name)| SubclassLine { id, name });
    trace("sub_class", parser).parse_next(i)
}

fn class<'input>(i: &mut &'input str) -> PResult<ClassLine<'input>> {
    let parser =
        ('C', space1, hex2, space1, string).map(|(_, _, id, _, name)| ClassLine { id, name });
    trace("class", parser).parse_next(i)
}

/// A string until the end of the line
fn string<'input>(i: &mut &'input str) -> PResult<&'input str> {
    let parser = take_until(0.., '\n');

    trace("string", parser).parse_next(i)
}

pub fn hex2(i: &mut &str) -> PResult<u8> {
    trace("hex2", take(2usize).verify(is_hex))
        .and_then(hex_uint::<_, u8, _>)
        .parse_next(i)
}

pub fn hex4(i: &mut &str) -> PResult<u16> {
    trace("hex4", take(4usize).verify(is_hex))
        .and_then(hex_uint::<_, u16, _>)
        .parse_next(i)
}

fn is_hex(s: &str) -> bool {
    for c in s.bytes() {
        if !AsChar::is_hex_digit(c) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {

    use crate::pci::{Device, PciIdDb, Subsystem, Vendor};

    use super::*;
    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use winnow::combinator::terminated;

    #[test]
    fn test_build_hierarchy() {
        let test_data = vec![
            Line::Vendor(VendorLine {
                id: 0x0001,
                name: "Some ID",
            }),
            Line::Vendor(VendorLine {
                id: 0x0010,
                name: "Some other ID",
            }),
            Line::Device(DeviceLine {
                id: 0x8139,
                name: "A device",
            }),
            Line::Vendor(VendorLine {
                id: 0x0014,
                name: "Another ID",
            }),
            Line::Device(DeviceLine {
                id: 0x0001,
                name: "ID ID ID",
            }),
            Line::Subsystem(SubsystemLine {
                subvendor: 0x001c,
                subdevice: 0x0004,
                name: "Sub device",
            }),
            // Classes
            Line::Class(ClassLine {
                id: 0x00,
                name: "CA",
            }),
            Line::Subclass(SubclassLine {
                id: 0x00,
                name: "CA 0",
            }),
            Line::Subclass(SubclassLine {
                id: 0x01,
                name: "CA 1",
            }),
            Line::Subclass(SubclassLine {
                id: 0x05,
                name: "CA 5",
            }),
            Line::Class(ClassLine {
                id: 0x06,
                name: "CB",
            }),
            Line::Subclass(SubclassLine {
                id: 0x00,
                name: "CB 0",
            }),
            Line::Subclass(SubclassLine {
                id: 0x01,
                name: "CB 1",
            }),
            Line::ProgrammingInterface(ProgrammingInterfaceLine {
                id: 0x00,
                name: "CB 1 0",
            }),
            Line::ProgrammingInterface(ProgrammingInterfaceLine {
                id: 0x05,
                name: "CB 1 5",
            }),
            Line::Subclass(SubclassLine {
                id: 0x02,
                name: "CC",
            }),
        ];

        let db = build_hierarchy(&test_data).unwrap();

        assert_eq!(
            db,
            PciIdDb {
                classes: AHashMap::from([
                    (
                        0,
                        Class {
                            name: "CA".into(),
                            subclasses: AHashMap::from([
                                (
                                    0,
                                    Subclass {
                                        name: "CA 0".into(),
                                        program_interfaces: AHashMap::from([])
                                    }
                                ),
                                (
                                    1,
                                    Subclass {
                                        name: "CA 1".into(),
                                        program_interfaces: AHashMap::from([])
                                    }
                                ),
                                (
                                    5,
                                    Subclass {
                                        name: "CA 5".into(),
                                        program_interfaces: AHashMap::from([])
                                    }
                                ),
                            ])
                        }
                    ),
                    (
                        6,
                        Class {
                            name: "CB".into(),
                            subclasses: AHashMap::from([
                                (
                                    0,
                                    Subclass {
                                        name: "CB 0".into(),
                                        program_interfaces: AHashMap::from([])
                                    }
                                ),
                                (
                                    1,
                                    Subclass {
                                        name: "CB 1".into(),
                                        program_interfaces: AHashMap::from([
                                            (
                                                0,
                                                ProgrammingInterface {
                                                    name: "CB 1 0".into()
                                                }
                                            ),
                                            (
                                                5,
                                                ProgrammingInterface {
                                                    name: "CB 1 5".into()
                                                }
                                            ),
                                        ])
                                    }
                                ),
                                (
                                    2,
                                    Subclass {
                                        name: "CC".into(),
                                        program_interfaces: AHashMap::from([])
                                    }
                                ),
                            ])
                        }
                    ),
                ]),
                vendors: AHashMap::from([
                    (
                        0x0001,
                        Vendor {
                            name: "Some ID".into(),
                            devices: AHashMap::from([])
                        }
                    ),
                    (
                        0x0010,
                        Vendor {
                            name: "Some other ID".into(),
                            devices: AHashMap::from([(
                                0x8139,
                                Device {
                                    name: "A device".into(),
                                    subsystems: AHashMap::from([])
                                }
                            )])
                        }
                    ),
                    (
                        0x0014,
                        Vendor {
                            name: "Another ID".into(),
                            devices: AHashMap::from([(
                                0x0001,
                                Device {
                                    name: "ID ID ID".into(),
                                    subsystems: AHashMap::from([(
                                        (0x001c, 0x0004),
                                        Subsystem {
                                            name: "Sub device".into()
                                        }
                                    )])
                                }
                            )])
                        }
                    ),
                ])
            }
        );
    }

    const TEST_DATA: &str = indoc! {
"0001  Some ID
0010  Some other ID
# A Comment
\t8139  A device
0014  Another ID
\t0001  ID ID ID
\t\t001c 0004  Sub device

# A comment

C 00  CA
\t00  CA 0
\t01  CA 1
\t05  CA 5
C 01  CB
\t00  CB 0
\t01  CB 1
\t\t00  CB 1 0
\t\t05  CB 1 5
\t02  CC\n"};

    #[test]
    fn test_parse_file() {
        let parsed = parse_file.parse(TEST_DATA).unwrap();

        assert_eq!(
            parsed,
            vec![
                Line::Vendor(VendorLine {
                    id: 0x0001,
                    name: "Some ID"
                }),
                Line::Vendor(VendorLine {
                    id: 0x0010,
                    name: "Some other ID"
                }),
                Line::Device(DeviceLine {
                    id: 0x8139,
                    name: "A device"
                }),
                Line::Vendor(VendorLine {
                    id: 0x0014,
                    name: "Another ID"
                }),
                Line::Device(DeviceLine {
                    id: 0x0001,
                    name: "ID ID ID"
                }),
                Line::Subsystem(SubsystemLine {
                    subvendor: 0x001c,
                    subdevice: 0x0004,
                    name: "Sub device"
                }),
                Line::Class(ClassLine {
                    id: 0x00,
                    name: "CA"
                }),
                Line::Subclass(SubclassLine {
                    id: 0x00,
                    name: "CA 0"
                }),
                Line::Subclass(SubclassLine {
                    id: 0x01,
                    name: "CA 1"
                }),
                Line::Subclass(SubclassLine {
                    id: 0x05,
                    name: "CA 5"
                }),
                Line::Class(ClassLine {
                    id: 0x01,
                    name: "CB"
                }),
                Line::Subclass(SubclassLine {
                    id: 0x00,
                    name: "CB 0"
                }),
                Line::Subclass(SubclassLine {
                    id: 0x01,
                    name: "CB 1"
                }),
                Line::ProgrammingInterface(ProgrammingInterfaceLine {
                    id: 0x00,
                    name: "CB 1 0"
                }),
                Line::ProgrammingInterface(ProgrammingInterfaceLine {
                    id: 0x05,
                    name: "CB 1 5"
                }),
                Line::Subclass(SubclassLine {
                    id: 0x02,
                    name: "CC"
                }),
            ]
        );
    }

    #[test]
    fn test_class() {
        let parsed = terminated(class, newline)
            .parse("C 00  Something\n")
            .unwrap();

        assert_eq!(
            parsed,
            ClassLine {
                id: 0,
                name: "Something"
            }
        );
    }

    #[test]
    fn test_sub_class() {
        let parsed = terminated(sub_class, newline)
            .parse("\t0f  Some string\n")
            .unwrap();
        assert_eq!(
            parsed,
            SubclassLine {
                id: 0x0f,
                name: "Some string"
            }
        );
    }
}
