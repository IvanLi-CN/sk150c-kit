#![cfg_attr(not(test), no_std)]

extern crate alloc;

// Expose only modules needed for host-side unit tests.
pub mod button;

// Provide a minimal defmt global logger for host-side tests to satisfy symbols.
#[cfg(test)]
mod defmt_test_logger {
    use defmt as _;

    #[defmt::global_logger]
    struct HostTestLogger;

    unsafe impl defmt::Logger for HostTestLogger {
        fn acquire() {
            // no-op
        }
        unsafe fn flush() {}
        unsafe fn release() {}
        unsafe fn write(_bytes: &[u8]) {}
    }

    // Provide a constant timestamp to satisfy formatting.
    defmt::timestamp!("{=u64}", 0u64);
}
