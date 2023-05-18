//! This crate interfaces with the MMTk framework to provide memory management capabilities.

/// Represents an Address in memory
#[repr(transparent)]
#[derive(Copy, Clone, Eq, Hash, PartialOrd, Ord, PartialEq)]
pub struct Address(usize);

/// Allocate an object of `size` using MMTk
#[no_mangle]
pub extern "C" fn mmtk_alloc(size: usize) -> Address {
    println!("mmtk_alloc {}", size);
    Address(0_usize)
}
