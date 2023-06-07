//! This crate interfaces with the MMTk framework to provide memory management capabilities.

#[macro_use]
extern crate lazy_static;

use crate::{
    active_plan::ArtActivePlan,
    collection::ArtCollection,
    object_model::ArtObjectModel,
    reference_glue::ArtReferenceGlue,
    scanning::ArtScanning,
};
use mmtk::{
    MMTK,
    MMTKBuilder,
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
}

/// Global static instance of upcalls
pub static mut UPCALLS: *const ArtUpcalls = null_mut();
