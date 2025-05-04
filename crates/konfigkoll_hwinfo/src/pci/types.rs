use ahash::AHashMap;

#[derive(Debug, PartialEq, Eq)]
pub struct Class {
    pub name: String,
    pub subclasses: AHashMap<u8, Subclass>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Subclass {
    pub name: String,
    pub program_interfaces: AHashMap<u8, ProgrammingInterface>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProgrammingInterface {
    pub name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Vendor {
    pub name: String,
    pub devices: AHashMap<u16, Device>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub subsystems: AHashMap<(u16, u16), Subsystem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Subsystem {
    pub name: String,
}
