use crate::{Art, UPCALLS};
use mmtk::{
    util::{Address, ObjectReference, copy::*},
    vm::*,
};

/// Implements object model trait
pub struct ArtObjectModel;

impl ObjectModel<Art> for ArtObjectModel {
    const GLOBAL_LOG_BIT_SPEC: VMGlobalLogBitSpec = VMGlobalLogBitSpec::side_first();
    const LOCAL_FORWARDING_POINTER_SPEC: VMLocalForwardingPointerSpec = VMLocalForwardingPointerSpec::in_header(0);
    const LOCAL_FORWARDING_BITS_SPEC: VMLocalForwardingBitsSpec = VMLocalForwardingBitsSpec::in_header(0);
    const LOCAL_LOS_MARK_NURSERY_SPEC: VMLocalLOSMarkNurserySpec = VMLocalLOSMarkNurserySpec::side_first();
    const LOCAL_MARK_BIT_SPEC: VMLocalMarkBitSpec = VMLocalMarkBitSpec::side_after(Self::LOCAL_LOS_MARK_NURSERY_SPEC.as_spec());

    const UNIFIED_OBJECT_REFERENCE_ADDRESS: bool = true;
    const OBJECT_REF_OFFSET_LOWER_BOUND: isize = 0;

    fn copy(
        from: ObjectReference,
        semantics: CopySemantics,
        copy_context: &mut GCWorkerCopyContext<Art>,
    ) -> ObjectReference {
        let bytes = Self::get_size_when_copied(from);
        let dst = copy_context.alloc_copy(from, bytes, ::std::mem::size_of::<usize>(), 0, semantics);
        let src = from.to_raw_address();
        unsafe { std::ptr::copy_nonoverlapping::<u8>(src.to_ptr(), dst.to_mut_ptr(), bytes) }
        let to_obj = ObjectReference::from_raw_address(dst);
        copy_context.post_copy(to_obj, bytes, semantics);
        to_obj
    }

    fn copy_to(_from: ObjectReference, _to: ObjectReference, _region: Address) -> Address {
        unimplemented!()
    }

    fn get_current_size(object: ObjectReference) -> usize {
        use mmtk::util::conversions;
        conversions::raw_align_up(unsafe { ((*UPCALLS).size_of)(object) }, Art::MIN_ALIGNMENT)
    }

    fn get_size_when_copied(object: ObjectReference) -> usize {
        Self::get_current_size(object)
    }

    fn get_align_when_copied(_object: ObjectReference) -> usize {
        Art::MIN_ALIGNMENT
    }

    fn get_align_offset_when_copied(_object: ObjectReference) -> usize {
        0
    }

    fn get_reference_when_copied_to(_from: ObjectReference, _to: Address) -> ObjectReference {
        unimplemented!()
    }

    fn get_type_descriptor(_reference: ObjectReference) -> &'static [i8] {
        unimplemented!()
    }

    fn ref_to_object_start(object: ObjectReference) -> Address {
        object.to_raw_address()
    }

    fn ref_to_header(object: ObjectReference) -> Address {
        object.to_raw_address()
    }

    fn ref_to_address(object: ObjectReference) -> Address {
        Self::ref_to_object_start(object)
    }

    fn address_to_ref(addr: Address) -> ObjectReference {
        ObjectReference::from_raw_address(addr)
    }

    fn dump_object(_object: ObjectReference) {
        unimplemented!()
    }
}
