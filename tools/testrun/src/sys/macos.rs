// SPDX-License-Identifier: MIT
//! Available physical memory for admission of more tests. This is a
//! conservative VM estimate, not a process-tree memory limit: speculative
//! pages are already in free_count, and purgeable pages may be inactive.

use std::mem::{size_of, zeroed};
use std::sync::OnceLock;

/// Under normal pressure inactive pages may be reclaimed. Under warning
/// pressure count only free pages; critical pressure admits no extra
/// memory. An unavailable pressure reading also uses only free pages.
fn available_bytes(free: u64, inactive: u64, page_size: u64, pressure: Option<u32>) -> u64 {
    let pages = match pressure {
        Some(1) => free.saturating_add(inactive),
        Some(level) if level & 4 != 0 => 0,
        _ => free,
    };
    pages.saturating_mul(page_size)
}

fn pressure_level() -> Option<u32> {
    let mut level = 0u32;
    let mut size = size_of::<u32>();
    let result = unsafe {
        libc::sysctlbyname(
            c"kern.memorystatus_vm_pressure_level".as_ptr(),
            std::ptr::from_mut(&mut level).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (result == 0 && size == size_of::<u32>()).then_some(level)
}

/// Read again before each scheduling decision. A failed VM reading gives
/// no headroom, so it cannot silently enable the full fixed RAM budget.
pub fn available_memory() -> u64 {
    unsafe extern "C" {
        fn mach_host_self() -> libc::mach_port_t;
    }
    static HOST: OnceLock<libc::mach_port_t> = OnceLock::new();
    let host = *HOST.get_or_init(|| unsafe { mach_host_self() });
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return 0;
    }
    let mut stats: libc::vm_statistics64 = unsafe { zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let result = unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            std::ptr::from_mut(&mut stats).cast(),
            &mut count,
        )
    };
    if result != libc::KERN_SUCCESS || count < 3 {
        return 0;
    }
    available_bytes(
        u64::from(stats.free_count),
        u64::from(stats.inactive_count),
        page_size as u64,
        pressure_level(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_reduces_headroom_without_counting_pages_twice() {
        assert_eq!(available_bytes(100, 200, 16384, Some(1)), 300 * 16384);
        assert_eq!(available_bytes(100, 200, 16384, Some(2)), 100 * 16384);
        assert_eq!(available_bytes(100, 200, 16384, Some(4)), 0);
        assert_eq!(available_bytes(100, 200, 16384, Some(6)), 0);
        assert_eq!(available_bytes(100, 200, 4096, None), 100 * 4096);
        assert_eq!(available_bytes(100, 200, 4096, Some(99)), 100 * 4096);
        assert_eq!(available_bytes(0, 0, 16384, Some(1)), 0);
        assert_eq!(available_bytes(u64::MAX, 1, 16384, Some(1)), u64::MAX);
    }

    #[test]
    fn native_reading_fits_physical_memory() {
        let available = available_memory();
        let physical = super::super::physical_memory().expect("native physical memory reading");
        assert!(
            available <= physical,
            "available {available}, physical {physical}"
        );
        if pressure_level() == Some(1) {
            assert!(
                available > 0,
                "normal pressure must give a native VM reading"
            );
        }
    }
}
