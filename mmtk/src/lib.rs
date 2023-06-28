//! This crate interfaces with the MMTk framework to provide memory management capabilities.

#[macro_use]
extern crate lazy_static;

use crate::{
    active_plan::ArtActivePlan,
    collection::{
        ArtCollection,
        GcThreadKind,
    },
    object_model::ArtObjectModel,
    reference_glue::ArtReferenceGlue,
    scanning::ArtScanning,
};
use mmtk::{
    MMTK,
    MMTKBuilder,
    Mutator,
    util::{
        Address,
        ObjectReference,
        opaque_pointer::*,
    },
    vm::VMBinding,
};
use std::{
    ops::Range,
    ptr::null_mut,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    }
};

/// Module which implements `ActivePlan` trait
pub mod active_plan;
/// Module which implements the user-facing API
pub mod api;
/// Module which implements `Collection` trait
pub mod collection;
/// Module which implements `ObjectModel` trait
pub mod object_model;
/// Module which implements `ReferenceGlue` trait
pub mod reference_glue;
/// Module which implements `Scanning` trait
pub mod scanning;

/// VMBinding to ART
#[derive(Default)]
pub struct Art;

/// Has MMTk been initialized?
pub static IS_MMTK_INITIALIZED: AtomicBool = AtomicBool::new(false);

lazy_static! {
    /// MMTk builder instance
    pub static ref BUILDER: Mutex<MMTKBuilder> = Mutex::new(MMTKBuilder::new());
    /// The actual MMTk instance
    pub static ref SINGLETON: MMTK<Art> = {
        let builder = BUILDER.lock().unwrap();
        assert!(!IS_MMTK_INITIALIZED.load(Ordering::Relaxed));
        let ret = mmtk::memory_manager::mmtk_init(&builder);
        IS_MMTK_INITIALIZED.store(true, Ordering::SeqCst);
        *ret
    };
}

/// The type of edge in ART
type ArtEdge = Address;

impl VMBinding for Art {
    type VMActivePlan = ArtActivePlan;
    type VMCollection = ArtCollection;
    type VMObjectModel = ArtObjectModel;
    type VMReferenceGlue = ArtReferenceGlue;
    type VMScanning = ArtScanning;

    type VMEdge = ArtEdge;
    type VMMemorySlice = Range<Address>;

    const MIN_ALIGNMENT: usize = 8;
    const MAX_ALIGNMENT: usize = 8;
    const ALLOC_END_ALIGNMENT: usize = 8;
    const USE_ALLOCATION_OFFSET: bool = false;
}

#[repr(C)]
/// Upcalls from MMTk to ART
pub struct ArtUpcalls {
    /// Get the size of the given object
    pub size_of: extern "C" fn(object: ObjectReference) -> usize,
    /// Block mutator thread for GC
    pub block_for_gc: extern "C" fn(tls: VMMutatorThread),
    /// Spawn GC thread with type `kind`
    pub spawn_gc_thread: extern "C" fn(tls: VMThread, kind: GcThreadKind, ctx: *mut libc::c_void),
    /// Stop all mutator threads
    pub stop_all_mutators: extern "C" fn(),
    /// Resume all mutator threads
    pub resume_mutators: extern "C" fn(),
    /// Return the number of mutators
    pub number_of_mutators: extern "C" fn() -> usize,
    /// Return if given thread `tls` is a mutator thread
    pub is_mutator: extern "C" fn(tls: VMThread) -> bool,
    /// Return the `Mutator` instance associated with thread `tls`
    pub get_mmtk_mutator: extern "C" fn(tls: VMMutatorThread) -> *mut Mutator<Art>,
    /// Evaluate given closure for each mutator thread
    pub for_all_mutators: extern "C" fn(closure: MutatorClosure),
    /// Scan all VM roots and report edges to MMTk
    pub scan_all_roots: extern "C" fn(closure: EdgesClosure),
}

/// Global static instance of upcalls
pub static mut UPCALLS: *const ArtUpcalls = null_mut();

/// A closure iterating on mutators. The C++ code should pass `data` back as the
/// last argument.
#[repr(C)]
pub struct MutatorClosure {
    /// The closure to invoke
    pub func: extern "C" fn(mutator: *mut Mutator<Art>, data: *mut libc::c_void),
    /// The Rust context associated with the closure
    pub data: *mut libc::c_void,
}

impl MutatorClosure {
    fn from_rust_closure<F>(callback: &mut F) -> Self
    where
        F: FnMut(&'static mut Mutator<Art>),
    {
        Self {
            func: Self::call_rust_closure::<F>,
            data: callback as *mut F as *mut libc::c_void,
        }
    }

    extern "C" fn call_rust_closure<F>(
        mutator: *mut Mutator<Art>,
        callback_ptr: *mut libc::c_void,
    ) where
        F: FnMut(&'static mut Mutator<Art>),
    {
        let callback: &mut F = unsafe { &mut *(callback_ptr as *mut F) };
        callback(unsafe { &mut *mutator });
    }
}

/// A representation of a Rust buffer
#[repr(C)]
pub struct RustBuffer {
    /// The start address of the buffer
    pub ptr: *mut Address,
    /// The capacity of the buffer
    pub capacity: usize,
}

/// A closure for reporting root edges. The C++ code should pass `data` back as the last argument.
#[repr(C)]
pub struct EdgesClosure {
    /// The closure to invoke
    pub func: extern "C" fn(
        buf: *mut Address,
        size: usize,
        cap: usize,
        data: *mut libc::c_void,
    ) -> RustBuffer,
    /// The Rust context associated with the closure
    pub data: *const libc::c_void,
}
