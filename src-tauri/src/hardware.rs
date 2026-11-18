//! What the player's computer has: memory, CPU threads and the strongest graphics adapter.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Hardware {
    pub ram_mb: u64,
    pub threads: usize,
    pub cpu: String,
    pub gpu: String,
    pub vram_mb: u64,
    /// "weak", "mid" or "strong": a rough class from dedicated video memory.
    pub gpu_class: &'static str,
}

pub fn detect() -> Hardware {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    // if the system cannot be read, assume a modest 8 GB machine rather than fail
    let (ram_mb, cpu) = match sysinfo::System::new() {
        Ok(mut system) => {
            system.refresh_memory();
            system.refresh_cpu_all();
            let cpu = system.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
            (system.total_memory() / 1024 / 1024, cpu)
        }
        Err(_) => (8 * 1024, String::new()),
    };
    let (gpu, vram_mb) = gpu();
    // integrated graphics report little or no dedicated memory; a guess, not a benchmark
    let gpu_class = if vram_mb >= 6 * 1024 {
        "strong"
    } else if vram_mb >= 2 * 1024 {
        "mid"
    } else {
        "weak"
    };
    Hardware { ram_mb, threads, cpu, gpu, vram_mb, gpu_class }
}

#[cfg(windows)]
fn gpu() -> (String, u64) {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
    let mut best = (String::from("unknown"), 0u64);
    // SAFETY: plain DXGI enumeration; every returned interface is released when dropped
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return best;
        };
        let mut index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(index) {
            index += 1;
            let Ok(desc) = adapter.GetDesc1() else { continue };
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                continue;
            }
            let vram = desc.DedicatedVideoMemory as u64 / 1024 / 1024;
            let name = String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_string();
            if vram > best.1 || best.0 == "unknown" {
                best = (name, vram);
            }
        }
    }
    best
}

#[cfg(not(windows))]
fn gpu() -> (String, u64) {
    (String::from("unknown"), 0)
}
