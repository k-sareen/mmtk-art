use crate::{Art, SINGLETON};
use mmtk::{
    Mutator,
    Plan,
    util::opaque_pointer::*,
    vm::ActivePlan,
};

/// Implements active GC plan trait
pub struct ArtActivePlan;

impl ActivePlan<Art> for ArtActivePlan {
    fn global() -> &'static dyn Plan<VM=Art> {
        SINGLETON.get_plan()
    }

    fn number_of_mutators() -> usize {
        unimplemented!()
    }

    fn is_mutator(_tls: VMThread) -> bool {
        true
    }

    fn mutator(_tls: VMMutatorThread) -> &'static mut Mutator<Art> {
        unimplemented!()
    }

    fn mutators<'a>() -> Box<dyn Iterator<Item = &'a mut Mutator<Art>> + 'a> {
        unimplemented!()
    }
}
