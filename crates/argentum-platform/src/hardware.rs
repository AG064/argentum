// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! The memory and GPUs of this computer, for the model marketplace's
//! "Recommended for your hardware" marks.
//!
//! Everything here is read locally from the operating system and stays on the
//! machine. Nothing is sent, stored or logged.

/// One GPU, as the operating system reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    pub name: String,
    /// Dedicated video memory. Integrated GPUs report a small reserved amount.
    pub dedicated_memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HardwareInfo {
    /// Installed system memory, or 0 when it could not be read.
    pub system_memory_bytes: u64,
    /// Hardware GPUs, largest dedicated memory first. Software renderers are left out.
    pub gpus: Vec<GpuInfo>,
}

impl HardwareInfo {
    /// The GPU llama.cpp would mostly use: the one with the most dedicated memory.
    pub fn main_gpu(&self) -> Option<&GpuInfo> {
        self.gpus.first()
    }
}

/// Read this computer's memory and GPUs. Fast, and safe to call from the UI thread.
pub fn detect_hardware() -> HardwareInfo {
    let mut gpus = detect_gpus();
    gpus.sort_by(|left, right| {
        right
            .dedicated_memory_bytes
            .cmp(&left.dedicated_memory_bytes)
    });
    HardwareInfo {
        system_memory_bytes: detect_system_memory().unwrap_or(0),
        gpus,
    }
}

#[cfg(windows)]
fn detect_system_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `status` is a valid MEMORYSTATUSEX with its length set, as the
    // call requires.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    (ok != 0).then_some(status.ullTotalPhys)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn detect_system_memory() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_meminfo_total(&text)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
fn detect_system_memory() -> Option<u64> {
    None
}

/// `MemTotal` from `/proc/meminfo`, in bytes.
#[cfg_attr(
    not(any(target_os = "linux", target_os = "android", test)),
    allow(dead_code)
)]
fn parse_meminfo_total(text: &str) -> Option<u64> {
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    kib.checked_mul(1024)
}

#[cfg(windows)]
fn detect_gpus() -> Vec<GpuInfo> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };

    // SAFETY: CreateDXGIFactory1 has no preconditions. It returns an owned COM
    // interface that the `windows` crate releases on drop.
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut gpus = Vec::new();
    for index in 0..16 {
        // SAFETY: `factory` is a live interface. The call returns an error once
        // `index` is past the last adapter.
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else {
            break;
        };
        // SAFETY: `adapter` is a live interface returned above.
        let Ok(description) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        if description.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
            continue;
        }
        let end = description
            .Description
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(description.Description.len());
        let name = String::from_utf16_lossy(&description.Description[..end])
            .trim()
            .to_owned();
        // Microsoft Basic Render Driver, the software adapter, has vendor 0x1414.
        if description.VendorId == 0x1414 && description.DedicatedVideoMemory == 0 {
            continue;
        }
        gpus.push(GpuInfo {
            name,
            dedicated_memory_bytes: description.DedicatedVideoMemory as u64,
        });
    }
    gpus
}

#[cfg(not(windows))]
fn detect_gpus() -> Vec<GpuInfo> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_total_is_read_in_bytes() {
        let text = "MemTotal:       32768000 kB\nMemFree:         1000 kB\n";
        assert_eq!(parse_meminfo_total(text), Some(32_768_000 * 1024));
        assert_eq!(parse_meminfo_total("MemFree: 1 kB\n"), None);
    }

    #[test]
    fn detection_reads_this_machine_without_failing() {
        let info = detect_hardware();
        if cfg!(any(windows, target_os = "linux")) {
            assert!(info.system_memory_bytes > 0, "{info:?}");
        }
        assert!(info
            .gpus
            .windows(2)
            .all(|pair| pair[0].dedicated_memory_bytes >= pair[1].dedicated_memory_bytes));
    }
}
