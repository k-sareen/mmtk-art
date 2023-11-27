use crate::{Art, UPCALLS};
use mmtk::{
    Mutator,
    util::opaque_pointer::*,
    vm::{Collection, GCThreadContext},
};

/// Type of GC worker
#[repr(C)]
pub enum GcThreadKind {
    /// GC Controller Context thread
    Controller = 0,
    /// Simple GC Worker thread
    Worker     = 1,
}

/// Implements collection trait
pub struct ArtCollection;

impl Collection<Art> for ArtCollection {
    fn stop_all_mutators<F>(_tls: VMWorkerThread, _mutator_visitor: F)
    where
        F: FnMut(&'static mut Mutator<Art>),
    {
        unsafe { ((*UPCALLS).stop_all_mutators)() }
    }

    fn resume_mutators(tls: VMWorkerThread) {
        unsafe { ((*UPCALLS).resume_mutators)(tls) }
    }

    fn block_for_gc(tls: VMMutatorThread) {
        unsafe { ((*UPCALLS).block_for_gc)(tls) }
    }

    fn spawn_gc_thread(tls: VMThread, ctx: GCThreadContext<Art>) {
        let (ctx_ptr, kind) = match ctx {
            GCThreadContext::Controller(c) => (
                Box::into_raw(c) as *mut libc::c_void,
                GcThreadKind::Controller,
            ),
            GCThreadContext::Worker(w) => (
                Box::into_raw(w) as *mut libc::c_void,
                GcThreadKind::Worker,
            ),
        };
        unsafe {
            ((*UPCALLS).spawn_gc_thread)(tls, kind, ctx_ptr);
        }
    }
}
