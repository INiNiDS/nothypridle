use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
};

pub struct ResourceMonitor {
    cpu: f32,
    gpu: f32,
    ram: u64,
    vram: u64,
    sys: System,
    gpu_monitor: GpuMonitor,
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceMonitor {
    pub fn new() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        sys.refresh_all();

        Self {
            cpu: 0.0,
            gpu: 0.0,
            ram: 0,
            vram: u64::MAX,
            sys,
            gpu_monitor: GpuMonitor::new(),
        }
    }

    pub fn update(&mut self) {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();

        let cpu_usage = self.sys.global_cpu_usage();
        let free_mem = self.sys.free_memory();

        let (gpu_busy, vram_free_mb) = self.gpu_monitor.get_metrics().unwrap_or((0, u64::MAX));

        self.cpu = cpu_usage;
        self.ram = free_mem;
        self.gpu = gpu_busy as f32;
        self.vram = vram_free_mb;
    }

    pub fn get_cpu(&self) -> f32 {
        self.cpu
    }
    pub fn get_gpu(&self) -> f32 {
        self.gpu
    }
    pub fn get_ram(&self) -> u64 {
        self.ram
    }
    /// Free VRAM in megabytes (`MB`).
    pub fn get_vram(&self) -> u64 {
        self.vram
    }

    pub fn check_any_process_running(&mut self, blacklist: &[&str]) -> bool {
        if blacklist.is_empty() {
            return false;
        }

        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );

        self.sys.processes().values().any(|process| {
            let process_name = process.name().to_string_lossy();
            blacklist
                .iter()
                .any(|&target| process_name.contains(target))
        })
    }
}

pub struct GpuMonitor {
    amd_monitor: AmdGpuMonitor,
    nvidia_monitor: NvidiaGpuMonitor,
    intel_monitor: IntelGpuMonitor,
}

impl Default for GpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuMonitor {
    pub fn new() -> Self {
        Self {
            amd_monitor: AmdGpuMonitor::new(),
            nvidia_monitor: NvidiaGpuMonitor::new(),
            intel_monitor: IntelGpuMonitor::new(),
        }
    }

    pub fn get_metrics(&self) -> Option<(u32, u64)> {
        let mut max_gpu_busy = 0;
        let mut vram_free: Option<u64> = None;

        if let Some((busy, vram)) = self.nvidia_monitor.get_metrics() {
            max_gpu_busy = max_gpu_busy.max(busy);
            vram_free = Some(vram);
        }
        if let Some((busy, vram)) = self.amd_monitor.get_metrics() {
            max_gpu_busy = max_gpu_busy.max(busy);
            if vram_free.is_none() {
                vram_free = Some(vram);
            }
        }
        if let Some((busy, vram)) = self.intel_monitor.get_metrics() {
            max_gpu_busy = max_gpu_busy.max(busy);
            if vram_free.is_none() {
                vram_free = Some(vram);
            }
        }

        Some((max_gpu_busy, vram_free.unwrap_or(u64::MAX)))
    }
}

pub struct AmdGpuMonitor {
    gpu_paths: Vec<PathBuf>,
}

impl Default for AmdGpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl AmdGpuMonitor {
    pub fn new() -> Self {
        let all_paths = find_amd_gpu_paths();
        let connected_gpus = get_connected_gpus();

        let connected_amd_paths: Vec<PathBuf> = all_paths
            .iter()
            .filter(|path| {
                if let Some(card_name) = path.file_name().and_then(|n| n.to_str()) {
                    connected_gpus.contains(card_name)
                } else {
                    false
                }
            })
            .cloned()
            .collect();

        let gpu_paths = if !connected_amd_paths.is_empty() {
            for path in &connected_amd_paths {
                tracing::info!("Found AMD GPU with connected display: {:?}", path);
            }
            connected_amd_paths
        } else if !all_paths.is_empty() {
            tracing::info!(
                "No connected monitors found on AMD GPUs. Fallback to monitoring all AMD GPUs: {:?}",
                all_paths
            );
            all_paths
        } else {
            tracing::info!("AMD GPUs not found. No AMD GPUs will be monitored.");
            all_paths
        };

        Self { gpu_paths }
    }

    pub fn get_metrics(&self) -> Option<(u32, u64)> {
        if self.gpu_paths.is_empty() {
            return None;
        }

        let mut max_gpu_busy = 0;
        let mut min_vram_free = u64::MAX;

        for card_path in &self.gpu_paths {
            let device_path = card_path.join("device");
            let busy_path = device_path.join("gpu_busy_percent");
            let used_path = device_path.join("mem_info_vram_used");
            let total_path = device_path.join("mem_info_vram_total");

            if let Ok(busy_str) = fs::read_to_string(busy_path)
                && let Ok(busy) = busy_str.trim().parse::<u32>()
            {
                max_gpu_busy = max_gpu_busy.max(busy);
            }

            let vram_used: Option<u64> = fs::read_to_string(used_path)
                .ok()
                .and_then(|s| s.trim().parse().ok());
            let vram_total: Option<u64> = fs::read_to_string(total_path)
                .ok()
                .and_then(|s| s.trim().parse().ok());

            if let (Some(total), Some(used)) = (vram_total, vram_used)
                && total > 0
            {
                let free = total.saturating_sub(used);
                let free_mb = free / (1024 * 1024);
                min_vram_free = min_vram_free.min(free_mb);
            }
        }

        Some((max_gpu_busy, min_vram_free))
    }
}

/// Returns a set of GPU names that are connected to the system.
pub fn get_connected_gpus() -> HashSet<String> {
    let mut connected_cards = HashSet::new();
    let drm_path = Path::new("/sys/class/drm");

    let entries = match fs::read_dir(drm_path) {
        Ok(e) => e,
        Err(_) => return connected_cards,
    };

    for entry in entries.flatten() {
        if let Some(card_name) = connected_card_name(&entry) {
            connected_cards.insert(card_name);
        }
    }

    connected_cards
}

/// Returns the base card name (e.g. `card0`) if `entry` is a connected DRM
/// connector(sysfs `cardN-M` with `status == "connected"`).
fn connected_card_name(entry: &fs::DirEntry) -> Option<String> {
    let name = entry.file_name();
    let name_str = name.to_string_lossy();

    if !name_str.starts_with("card") || !name_str.contains('-') {
        return None;
    }

    let status = fs::read_to_string(entry.path().join("status")).ok()?;
    if status.trim() != "connected" {
        return None;
    }

    name_str.split('-').next().map(|s| s.to_string())
}

/// A predicate that decides whether a enumerated DRM `cardN` device belongs to
/// a given vendor (by sysfs `vendor` id or `driver` symlink name).
type VendorPredicate = fn(&Path) -> bool;

/// Enumerates `/sys/class/drm/cardN` devices for which `matches` returns true.
fn find_gpu_paths_by_vendor(matches: VendorPredicate) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let drm_dir = Path::new("/sys/class/drm");

    let entries = match fs::read_dir(drm_dir) {
        Ok(e) => e,
        Err(_) => return paths,
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with("card")
            && name_str["card".len()..].chars().all(|c| c.is_ascii_digit())
            && matches(&entry.path())
        {
            paths.push(entry.path());
        }
    }

    paths.sort();
    paths
}

/// True if the sysfs `vendor` file matches or the `driver` symlink contains
/// the given vendor id and driver name.
fn matches_vendor_or_driver(card_path: &Path, vendor_id: &str, driver: &str) -> bool {
    let device_path = card_path.join("device");

    let vendor_file = device_path.join("vendor");
    if let Ok(vendor) = fs::read_to_string(vendor_file)
        && vendor.trim().to_lowercase().contains(vendor_id)
    {
        return true;
    }

    let driver_symlink = device_path.join("driver");
    if let Ok(target) = fs::read_link(driver_symlink)
        && target.to_string_lossy().contains(driver)
    {
        return true;
    }

    false
}

fn is_amd_gpu(card_path: &Path) -> bool {
    matches_vendor_or_driver(card_path, "0x1002", "amdgpu")
}

pub fn find_amd_gpu_paths() -> Vec<PathBuf> {
    find_gpu_paths_by_vendor(is_amd_gpu)
}

pub struct NvidiaGpuMonitor {
    nvml: Option<nvml_wrapper::Nvml>,
}

impl Default for NvidiaGpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl NvidiaGpuMonitor {
    pub fn new() -> Self {
        let nvml = match nvml_wrapper::Nvml::init() {
            Ok(n) => {
                tracing::info!("NVIDIA Management Library (NVML) successfully initialized.");
                Some(n)
            }
            Err(e) => {
                tracing::info!("NVIDIA GPU/NVML not found or failed to initialize: {:?}", e);
                None
            }
        };
        Self { nvml }
    }

    pub fn get_metrics(&self) -> Option<(u32, u64)> {
        let nvml = self.nvml.as_ref()?;

        let device_count = nvml.device_count().ok()?;
        if device_count == 0 {
            return None;
        }

        let mut max_gpu_busy = 0;
        let mut min_vram_free = u64::MAX;

        for i in 0..device_count {
            if let Ok(device) = nvml.device_by_index(i) {
                if let Ok(utilization) = device.utilization_rates() {
                    max_gpu_busy = max_gpu_busy.max(utilization.gpu);
                }
                if let Ok(memory_info) = device.memory_info()
                    && memory_info.total > 0
                {
                    let vram_free_mb = memory_info.free / (1024 * 1024);
                    min_vram_free = min_vram_free.min(vram_free_mb);
                }
            }
        }

        Some((max_gpu_busy, min_vram_free))
    }
}

pub struct IntelGpuMonitor {
    gpu_paths: Vec<PathBuf>,
}

impl Default for IntelGpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl IntelGpuMonitor {
    pub fn new() -> Self {
        let all_paths = find_intel_gpu_paths();
        let connected_gpus = get_connected_gpus();

        let connected_intel_paths: Vec<PathBuf> = all_paths
            .iter()
            .filter(|path| {
                if let Some(card_name) = path.file_name().and_then(|n| n.to_str()) {
                    connected_gpus.contains(card_name)
                } else {
                    false
                }
            })
            .cloned()
            .collect();

        let gpu_paths = if !connected_intel_paths.is_empty() {
            for path in &connected_intel_paths {
                tracing::info!("Found Intel GPU with connected display: {:?}", path);
            }
            connected_intel_paths
        } else if !all_paths.is_empty() {
            tracing::info!(
                "No connected monitors found on Intel GPUs. Monitoring all Intel GPUs: {:?}",
                all_paths
            );
            all_paths
        } else {
            tracing::info!("Intel GPUs not found. No Intel GPUs will be monitored.");
            all_paths
        };

        Self { gpu_paths }
    }

    pub fn get_metrics(&self) -> Option<(u32, u64)> {
        if self.gpu_paths.is_empty() {
            return None;
        }

        let mut max_gpu_busy = 0;
        let mut min_vram_free = u64::MAX;

        for card_path in &self.gpu_paths {
            if is_gpu_suspended(card_path) {
                continue;
            }
            max_gpu_busy = max_gpu_busy.max(read_intel_busy_pct(card_path));
            if let Some(free_mb) = read_intel_vram_free_mb(card_path) {
                min_vram_free = min_vram_free.min(free_mb);
            }
        }

        Some((max_gpu_busy, min_vram_free))
    }
}

/// Returns true if the GPU's `power/runtime_status` sysfs file reports
/// "suspended".
fn is_gpu_suspended(card_path: &Path) -> bool {
    let status_path = card_path.join("device/power/runtime_status");
    fs::read_to_string(status_path)
        .map(|s| s.trim() == "suspended")
        .unwrap_or(false)
}

/// Computes Intel GPU busy percentage from `rps_cur_freq_mhz / rps_max_freq_mhz`,
/// trying the standard `gt/gt0` path first, then a per-card DRM fallback.
fn read_intel_busy_pct(card_path: &Path) -> u32 {
    if let Some(pct) = read_freq_ratio(
        card_path.join("gt/gt0/rps_cur_freq_mhz"),
        card_path.join("gt/gt0/rps_max_freq_mhz"),
    ) {
        return pct;
    }

    let Some(card_name) = card_path.file_name() else {
        return 0;
    };
    let drm_dir = card_path.join("device/drm").join(card_name);
    read_freq_ratio(
        drm_dir.join("gt_cur_freq_mhz"),
        drm_dir.join("gt_max_freq_mhz"),
    )
    .unwrap_or(0)
}

fn read_freq_ratio(cur_path: PathBuf, max_path: PathBuf) -> Option<u32> {
    let cur_str = fs::read_to_string(&cur_path).ok()?;
    let max_str = fs::read_to_string(&max_path).ok()?;
    let cur = cur_str.trim().parse::<u32>().ok()?;
    let max = max_str.trim().parse::<u32>().ok()?;
    if max == 0 {
        return None;
    }
    Some(((cur as f32 / max as f32) * 100.0) as u32)
}

/// Reads Intel discrete-GPU free VRAM (in MB) from `tile0` sysfs files when
/// present.
fn read_intel_vram_free_mb(card_path: &Path) -> Option<u64> {
    let tile0_path = card_path.join("device/tile0");
    let total_vram_path = tile0_path.join("memory_all_bytes");
    let free_vram_path = tile0_path.join("memory_free_bytes");

    if !total_vram_path.exists() || !free_vram_path.exists() {
        return None;
    }

    let total: u64 = fs::read_to_string(total_vram_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())?;
    let free: u64 = fs::read_to_string(free_vram_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())?;
    if total == 0 {
        return None;
    }
    Some(free / (1024 * 1024))
}

fn is_intel_gpu(card_path: &Path) -> bool {
    let device_path = card_path.join("device");

    let vendor_file = device_path.join("vendor");
    if let Ok(vendor) = fs::read_to_string(vendor_file)
        && vendor.trim().to_lowercase().contains("0x8086")
    {
        return true;
    }

    let driver_symlink = device_path.join("driver");
    if let Ok(target) = fs::read_link(driver_symlink) {
        let target_str = target.to_string_lossy();
        if target_str.contains("i915") || target_str.contains("xe") {
            return true;
        }
    }

    false
}

pub fn find_intel_gpu_paths() -> Vec<PathBuf> {
    find_gpu_paths_by_vendor(is_intel_gpu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Creates an empty unique temp directory under `std::env::temp_dir()` for
    /// a single test. The directory is cleaned up (best-effort) when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path =
                std::env::temp_dir().join(format!("nh-hal-test-{}-{}", std::process::id(), id));
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn symlink(&self, src: &str, link: &str) {
            let link_path = self.0.join(link);
            fs::create_dir_all(link_path.parent().unwrap()).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(src, link_path).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resource_monitor_update_then_get_returns_refreshed_values() {
        // `ResourceMonitor::new` queries the live system; we only assert the
        // accessors are consistent right after `update()` and do not panic on
        // machines without GPUs (VRAM stays at `u64::MAX`).
        let mut monitor = ResourceMonitor::new();
        monitor.update();
        let _ = monitor.get_cpu();
        let _ = monitor.get_gpu();
        let _ = monitor.get_ram();
        let _ = monitor.get_vram();
    }

    #[test]
    fn check_any_process_running_returns_false_for_empty_blacklist() {
        let mut monitor = ResourceMonitor::new();
        assert!(!monitor.check_any_process_running(&[]));
    }

    #[test]
    fn connected_card_name_rejects_non_connector_entries() {
        let dir = TempDir::new();
        let plain_card = dir.path().join("card0");
        fs::create_dir_all(&plain_card).unwrap();
        // `card0` has no `-`, so it isn't a connector entry.
        let entry = fs::read_dir(dir.path())
            .unwrap()
            .find(|e| e.as_ref().unwrap().file_name() == *"card0")
            .unwrap()
            .unwrap();
        assert_eq!(connected_card_name(&entry), None);
    }

    #[test]
    fn connected_card_name_recognizes_connected_connector() {
        let dir = TempDir::new();
        let connector = dir.path().join("card0-HDMI-A-1");
        fs::create_dir_all(&connector).unwrap();
        fs::write(connector.join("status"), "connected\n").unwrap();
        let entry = fs::read_dir(dir.path())
            .unwrap()
            .find(|e| e.as_ref().unwrap().file_name().to_string_lossy() == "card0-HDMI-A-1")
            .unwrap()
            .unwrap();
        assert_eq!(connected_card_name(&entry), Some("card0".to_string()));
    }

    #[test]
    fn connected_card_name_ignores_disconnected_connector() {
        let dir = TempDir::new();
        let connector = dir.path().join("card1-DP-1");
        fs::create_dir_all(&connector).unwrap();
        fs::write(connector.join("status"), "disconnected\n").unwrap();
        let entry = fs::read_dir(dir.path())
            .unwrap()
            .find(|e| e.as_ref().unwrap().file_name().to_string_lossy() == "card1-DP-1")
            .unwrap()
            .unwrap();
        assert_eq!(connected_card_name(&entry), None);
    }

    #[test]
    fn matches_vendor_or_driver_uses_vendor_id() {
        let dir = TempDir::new();
        let card = dir.path().join("card0");
        let device = card.join("device");
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("vendor"), "0x1002\n").unwrap();

        assert!(matches_vendor_or_driver(&card, "0x1002", "amdgpu"));
        assert!(!matches_vendor_or_driver(&card, "0x8086", "i915"));
    }

    #[test]
    fn matches_vendor_or_driver_uses_driver_symlink() {
        let dir = TempDir::new();
        let card = dir.path().join("card0");
        let device = card.join("device");
        fs::create_dir_all(&device).unwrap();
        // No vendor file; only the driver symlink is set.
        dir.symlink("/sys/bus/pci/drivers/amdgpu", "card0/device/driver");

        assert!(matches_vendor_or_driver(&card, "0x1002", "amdgpu"));
        assert!(!matches_vendor_or_driver(&card, "0x8086", "i915"));
    }

    #[test]
    fn is_amd_gpu_matches_amd_vendor() {
        let dir = TempDir::new();
        let card = dir.path().join("card0");
        let device = card.join("device");
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("vendor"), "0x1002\n").unwrap();
        assert!(is_amd_gpu(&card));
    }

    #[test]
    fn is_intel_gpu_matches_intel_vendor() {
        let dir = TempDir::new();
        let card = dir.path().join("card0");
        let device = card.join("device");
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("vendor"), "0x8086\n").unwrap();
        assert!(is_intel_gpu(&card));
    }

    #[test]
    fn is_intel_gpu_matches_i915_driver() {
        let dir = TempDir::new();
        let card = dir.path().join("card0");
        let device = card.join("device");
        fs::create_dir_all(&device).unwrap();
        dir.symlink("/sys/bus/pci/drivers/i915", "card0/device/driver");
        assert!(is_intel_gpu(&card));
    }

    #[test]
    fn read_freq_ratio_computes_percentage() {
        let dir = TempDir::new();
        fs::write(dir.path().join("cur"), "500\n").unwrap();
        fs::write(dir.path().join("max"), "1000\n").unwrap();
        let pct = read_freq_ratio(dir.path().join("cur"), dir.path().join("max"));
        assert_eq!(pct, Some(50));
    }

    #[test]
    fn read_freq_ratio_returns_none_when_max_is_zero() {
        let dir = TempDir::new();
        fs::write(dir.path().join("cur"), "500\n").unwrap();
        fs::write(dir.path().join("max"), "0\n").unwrap();
        let pct = read_freq_ratio(dir.path().join("cur"), dir.path().join("max"));
        assert_eq!(pct, None);
    }

    #[test]
    fn read_freq_ratio_returns_none_on_missing_files() {
        let dir = TempDir::new();
        let pct = read_freq_ratio(dir.path().join("cur"), dir.path().join("max"));
        assert_eq!(pct, None);
    }

    #[test]
    fn read_intel_vram_free_mb_returns_mb_when_files_present() {
        let dir = TempDir::new();
        let tile0 = dir.path().join("device/tile0");
        fs::create_dir_all(&tile0).unwrap();
        // 16 GB total, 4 GB free -> 4096 MB.
        fs::write(
            tile0.join("memory_all_bytes"),
            format!("{}", 16u64 * 1024 * 1024 * 1024),
        )
        .unwrap();
        fs::write(
            tile0.join("memory_free_bytes"),
            format!("{}", 4u64 * 1024 * 1024 * 1024),
        )
        .unwrap();
        let free = read_intel_vram_free_mb(dir.path());
        assert_eq!(free, Some(4096));
    }

    #[test]
    fn read_intel_vram_free_mb_returns_none_when_files_absent() {
        let dir = TempDir::new();
        assert_eq!(read_intel_vram_free_mb(dir.path()), None);
    }

    #[test]
    fn read_intel_vram_free_mb_returns_none_when_total_is_zero() {
        let dir = TempDir::new();
        let tile0 = dir.path().join("device/tile0");
        fs::create_dir_all(&tile0).unwrap();
        fs::write(tile0.join("memory_all_bytes"), "0\n").unwrap();
        fs::write(tile0.join("memory_free_bytes"), "0\n").unwrap();
        assert_eq!(read_intel_vram_free_mb(dir.path()), None);
    }

    #[test]
    fn is_gpu_suspended_reports_true_for_suspended_runtime_status() {
        let dir = TempDir::new();
        let device = dir.path().join("device/power");
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("runtime_status"), "suspended\n").unwrap();
        assert!(is_gpu_suspended(dir.path()));
    }

    #[test]
    fn is_gpu_suspended_reports_false_for_active_runtime_status() {
        let dir = TempDir::new();
        let device = dir.path().join("device/power");
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("runtime_status"), "active\n").unwrap();
        assert!(!is_gpu_suspended(dir.path()));
    }

    #[test]
    fn is_gpu_suspended_reports_false_when_status_file_missing() {
        let dir = TempDir::new();
        assert!(!is_gpu_suspended(dir.path()));
    }

    #[test]
    fn read_intel_busy_pct_uses_gt0_path_when_present() {
        let dir = TempDir::new();
        let gt0 = dir.path().join("gt/gt0");
        fs::create_dir_all(&gt0).unwrap();
        fs::write(gt0.join("rps_cur_freq_mhz"), "250\n").unwrap();
        fs::write(gt0.join("rps_max_freq_mhz"), "1000\n").unwrap();
        assert_eq!(read_intel_busy_pct(dir.path()), 25);
    }

    #[test]
    fn gpu_monitor_get_metrics_returns_some_even_without_gpus() {
        // On a CI/host machine without AMD/NVIDIA/Intel GPUs, `get_metrics`
        // still returns `Some((0, u64::MAX))` rather than `None`.
        let monitor = GpuMonitor::new();
        let m = monitor.get_metrics();
        assert!(m.is_some());
    }
}
