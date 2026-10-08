//! Hardware and CPU feature detection engine.
//! Inspects CPU instruction extensions, core counts, and selects optimal thread configurations.

use std::fmt;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HardwareProfile {
    pub os: String,
    pub arch: String,
    pub logical_cores: usize,
    pub recommended_threads: i32,
    pub has_avx2: bool,
    pub has_avx512: bool,
    pub has_vnni: bool,
    pub has_neon: bool,
    pub has_metal: bool,
    pub summary: String,
}

impl fmt::Display for HardwareProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.summary)
    }
}

/// Threads for Parakeet's ONNX session: the performance cores on Apple Silicon (efficiency cores
/// only slow a parallel matrix multiply down), otherwise the general recommendation.
pub fn parakeet_threads() -> usize {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn sysctlbyname(name: *const u8, oldp: *mut std::ffi::c_void, oldlenp: *mut usize, newp: *mut std::ffi::c_void, newlen: usize) -> i32;
        }
        let mut cores: i32 = 0;
        let mut len = std::mem::size_of::<i32>();
        let ok = unsafe {
            sysctlbyname(b"hw.perflevel0.physicalcpu\0".as_ptr(), &mut cores as *mut i32 as *mut _, &mut len, std::ptr::null_mut(), 0)
        } == 0;
        if ok && cores > 0 {
            return (cores as usize).clamp(4, 8);
        }
    }
    (detect_hardware_profile().recommended_threads.max(1) as usize).min(8)
}

pub fn detect_hardware_profile() -> HardwareProfile {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    let logical_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    // Compute optimal threads for speech recognition:
    // Avoid hyperthreading cache thrashing; sweet spot for Whisper/Parakeet is 4-6 threads.
    let recommended_threads = if logical_cores <= 4 {
        logical_cores as i32
    } else if logical_cores <= 8 {
        (logical_cores - 2).max(4) as i32
    } else {
        // High core count desktop/workstation (12+ cores): keep 6-8 threads to leave system responsive
        6
    };

    #[cfg(target_arch = "x86_64")]
    let (has_avx2, has_avx512, has_vnni, has_neon) = (
        is_x86_feature_detected!("avx2"),
        is_x86_feature_detected!("avx512f"),
        is_x86_feature_detected!("avx512vnni"),
        false,
    );

    #[cfg(target_arch = "aarch64")]
    let (has_avx2, has_avx512, has_vnni, has_neon) = (false, false, false, true);

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let (has_avx2, has_avx512, has_vnni, has_neon) = (false, false, false, false);

    let has_metal = cfg!(target_os = "macos");

    let mut features = Vec::new();
    if has_avx2 {
        features.push("AVX2");
    }
    if has_avx512 {
        features.push("AVX-512");
    }
    if has_vnni {
        features.push("VNNI (Neural Net)");
    }
    if has_neon {
        features.push("ARM NEON");
    }
    if has_metal {
        features.push("Apple Metal/ANE");
    }

    let features_str = if features.is_empty() {
        "Baseline SIMD".to_string()
    } else {
        features.join(", ")
    };

    let summary = format!(
        "{} {} ({} cores) | Accelerators: [{}] | Recommended Threads: {}",
        os.to_uppercase(),
        arch,
        logical_cores,
        features_str,
        recommended_threads
    );

    HardwareProfile {
        os,
        arch,
        logical_cores,
        recommended_threads,
        has_avx2,
        has_avx512,
        has_vnni,
        has_neon,
        has_metal,
        summary,
    }
}
