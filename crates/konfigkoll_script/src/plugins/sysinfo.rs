//! System information gathering
use rune::Any;
use rune::ContextError;
use rune::Module;
use sysinfo::CpuRefreshKind;
use sysinfo::MemoryRefreshKind;

use konfigkoll_hwinfo::pci::PciDevice;
use konfigkoll_hwinfo::pci::PciIdDb;

use super::error::KResult;

/// System info access
#[derive(Debug, Any)]
#[rune(item = ::sysinfo)]
struct SysInfo {
    inner: sysinfo::System,
    cpu_loaded: bool,
    pci_devices: Option<Vec<PciDevice>>,
    // TODO: Needed for future functionality
    #[allow(dead_code)]
    pci_db: Option<PciIdDb>,
}

impl SysInfo {
    /// Create a new system info object
    #[rune::function(path = Self::new)]
    fn new() -> Self {
        Self {
            inner: sysinfo::System::new(),
            cpu_loaded: false,
            pci_devices: None,
            pci_db: None,
        }
    }

    /// Total amount of memory in kB
    #[rune::function]
    fn total_memory(&mut self) -> u64 {
        self.inner
            .refresh_memory_specifics(MemoryRefreshKind::new().with_ram());
        self.inner.total_memory() / 1024
    }

    /// The system architecture
    #[rune::function]
    fn architecture(&self) -> Option<String> {
        sysinfo::System::cpu_arch()
    }

    /// The kernel version
    #[rune::function]
    fn kernel(&self) -> Option<String> {
        sysinfo::System::kernel_version()
    }

    /// The DNS hostname
    #[rune::function]
    fn host_name(&self) -> Option<String> {
        sysinfo::System::host_name()
    }

    /// The OS ID
    ///
    /// On Linux this corresponds to the `ID` field in `/etc/os-release`.
    #[rune::function]
    fn os_id(&self) -> String {
        sysinfo::System::distribution_id()
    }

    /// The OS version
    ///
    /// On Linux this corresponds to the `VERSION_ID` field in `/etc/os-release`
    /// or `DISTRIB_RELEASE` in `/etc/lsb-release`.
    #[rune::function]
    fn os_version(&self) -> Option<String> {
        sysinfo::System::os_version()
    }

    /// Number of physical CPU cores
    #[rune::function]
    fn cpu_cores(&self) -> Option<usize> {
        self.inner.physical_core_count()
    }

    /// Get the CPU vendor
    #[rune::function]
    fn cpu_vendor_id(&mut self) -> Option<String> {
        if !self.cpu_loaded {
            self.inner.refresh_cpu_specifics(CpuRefreshKind::default());
            self.cpu_loaded = true;
        }
        self.inner
            .cpus()
            .first()
            .map(|cpu| cpu.vendor_id().to_string())
    }

    /// Get the CPU vendor
    #[rune::function]
    fn cpu_brand(&mut self) -> Option<String> {
        if !self.cpu_loaded {
            self.inner.refresh_cpu_specifics(CpuRefreshKind::default());
            self.cpu_loaded = true;
        }
        self.inner.cpus().first().map(|cpu| cpu.brand().to_string())
    }

    #[rune::function]
    /// Get the PCI devices
    fn pci_devices(&mut self) -> KResult<Vec<PciDevice>> {
        if self.pci_devices.is_none() {
            let devices = konfigkoll_hwinfo::pci::load_pci_devices()?;
            self.pci_devices = Some(devices.collect());
        }

        self.pci_devices
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Failed to load PCI devices").into())
    }
}

#[rune::module(::sysinfo)]
/// Various functions to get system information
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<SysInfo>()?;
    m.function_meta(SysInfo::new)?;
    m.function_meta(SysInfo::architecture)?;
    m.function_meta(SysInfo::kernel)?;
    m.function_meta(SysInfo::host_name)?;
    m.function_meta(SysInfo::os_id)?;
    m.function_meta(SysInfo::os_version)?;
    m.function_meta(SysInfo::cpu_cores)?;
    m.function_meta(SysInfo::total_memory)?;
    m.function_meta(SysInfo::cpu_vendor_id)?;
    m.function_meta(SysInfo::cpu_brand)?;
    m.function_meta(SysInfo::pci_devices)?;
    m.ty::<PciDevice>()?;
    m.ty::<PciIdDb>()?;
    Ok(m)
}
