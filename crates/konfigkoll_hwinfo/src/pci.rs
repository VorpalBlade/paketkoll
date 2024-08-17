//! Utilities similar to pciutils to read PCI devices on Linux

use ahash::AHashMap;

mod parser;

/// A database of PCI devices IDs
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "rune", derive(rune::Any))]
#[cfg_attr(feature = "rune", rune(item = ::sysinfo))]
pub struct PciIdDb {
    pub classes: AHashMap<u8, Class>,
    pub vendors: AHashMap<u16, Vendor>,
}

impl PciIdDb {
    /// Create from a string containing `pci.ids`
    pub fn parse(s: &str) -> eyre::Result<Self> {
        parser::parse_pcidatabase(s)
    }

    /// Create from a file containing `pci.ids`
    pub fn parse_file(path: &std::path::Path) -> eyre::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        Self::parse(&s)
    }
}

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

/// Data about a PCI device
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "rune", derive(rune::Any))]
#[cfg_attr(feature = "rune", rune(item = ::sysinfo))]
pub struct PciDevice {
    #[cfg_attr(feature = "rune", rune(get))]
    pub class: u32,
    #[cfg_attr(feature = "rune", rune(get))]
    pub vendor: u16,
    #[cfg_attr(feature = "rune", rune(get))]
    pub device: u16,
    #[cfg_attr(feature = "rune", rune(get))]
    pub revision: u8,
    #[cfg_attr(feature = "rune", rune(get))]
    pub subsystem_vendor: u16,
    #[cfg_attr(feature = "rune", rune(get))]
    pub subsystem_device: u16,
}

impl PciDevice {
    /// Load data from /sys
    fn from_directory(path: &std::path::Path) -> eyre::Result<Self> {
        let class = std::fs::read_to_string(path.join("class"))?;
        let vendor = std::fs::read_to_string(path.join("vendor"))?;
        let device = std::fs::read_to_string(path.join("device"))?;
        let revision = std::fs::read_to_string(path.join("revision"))?;
        let subsystem_vendor = std::fs::read_to_string(path.join("subsystem_vendor"))?;
        let subsystem_device = std::fs::read_to_string(path.join("subsystem_device"))?;
        Ok(Self {
            class: u32::from_str_radix(&class, 16)?,
            vendor: u16::from_str_radix(&vendor, 16)?,
            device: u16::from_str_radix(&device, 16)?,
            revision: u8::from_str_radix(&revision, 16)?,
            subsystem_vendor: u16::from_str_radix(&subsystem_vendor, 16)?,
            subsystem_device: u16::from_str_radix(&subsystem_device, 16)?,
        })
    }

    /// Get the vendor, device and possibly subsystem names
    pub fn vendor_names<'db>(&self, db: &'db PciIdDb) -> PciVendorLookup<&'db str> {
        // Resolve vendor
        let vendor = db.vendors.get(&self.vendor);
        let device = vendor.and_then(|v| v.devices.get(&self.device));
        let subsystem = device.and_then(|d| {
            d.subsystems
                .get(&(self.subsystem_vendor, self.subsystem_device))
        });
        // The subvendor can be different from the main vendor
        // See https://admin.pci-ids.ucw.cz/mods/PC/?action=help?help=pci
        let subvendor = db.vendors.get(&self.subsystem_vendor);

        // Extract strings
        PciVendorLookup {
            vendor: vendor.map(|v| v.name.as_str()),
            device: device.map(|d| d.name.as_str()),
            subvendor: subvendor.map(|v| v.name.as_str()),
            subdevice: subsystem.map(|s| s.name.as_str()),
        }
    }

    /// Get the class, subclass and program interface names
    pub fn class_strings<'db>(&self, db: &'db PciIdDb) -> PciClassLookup<&'db str> {
        // Split up class 0xccsspp
        let class = (self.class >> 16) as u8;
        let subclass = (self.class >> 8) as u8;
        let program_interface = self.class as u8;

        // Resolve hierarchy
        let class = db.classes.get(&class);
        let subclass = class.and_then(|c| c.subclasses.get(&subclass));
        let program_interface = subclass.and_then(|s| s.program_interfaces.get(&program_interface));

        // Extract strings
        PciClassLookup {
            class: class.map(|c| c.name.as_str()),
            subclass: subclass.map(|s| s.name.as_str()),
            program_interface: program_interface.map(|p| p.name.as_str()),
        }
    }
}

/// Result from [`PciDevice::vendor_names`]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PciVendorLookup<S> {
    pub vendor: Option<S>,
    pub device: Option<S>,
    pub subvendor: Option<S>,
    pub subdevice: Option<S>,
}

impl<S> PciVendorLookup<S>
where
    S: ToOwned<Owned = String>,
{
    pub fn to_owned(&self) -> PciVendorLookup<S::Owned> {
        PciVendorLookup {
            vendor: self.vendor.as_ref().map(ToOwned::to_owned),
            device: self.device.as_ref().map(ToOwned::to_owned),
            subvendor: self.subvendor.as_ref().map(ToOwned::to_owned),
            subdevice: self.subdevice.as_ref().map(ToOwned::to_owned),
        }
    }
}

/// Result from [`PciDevice::class_strings`]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PciClassLookup<S> {
    pub class: Option<S>,
    pub subclass: Option<S>,
    pub program_interface: Option<S>,
}

impl<S> PciClassLookup<S>
where
    S: ToOwned<Owned = String>,
{
    pub fn to_owned(&self) -> PciClassLookup<S::Owned> {
        PciClassLookup {
            class: self.class.as_ref().map(ToOwned::to_owned),
            subclass: self.subclass.as_ref().map(ToOwned::to_owned),
            program_interface: self.program_interface.as_ref().map(ToOwned::to_owned),
        }
    }
}

/// Read PCI device info from `/sys`
pub fn load_pci_devices() -> eyre::Result<impl Iterator<Item = PciDevice>> {
    let path = std::path::Path::new("/sys/bus/pci/devices");
    let mut devices = vec![];
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        devices.push(PciDevice::from_directory(&path)?);
    }
    Ok(devices.into_iter())
}
