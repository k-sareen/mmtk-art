use crate::{
    abi::{
        get_field_slot,
        ART_HEAP_REFERENCE_SIZE,
        Class,
        ClassLoader,
        Object,
        ObjectArray,
        OBJECT_HEADER_SIZE,
        Reference,
        DexCache, ArtHeapReference,
    },
    likely,
    scanning::to_scan_object_closure,
    unlikely,
    ArtObjectWithNativeRootsType,
    ArtSlot,
    UPCALLS,
};
use mmtk::{
    util::{
        opaque_pointer::VMWorkerThread,
        ObjectReference,
    },
    vm::*,
};
use std::sync::atomic::Ordering;

impl Object {
    /// Visit the instance fields of an object.
    #[inline]
    pub fn visit_instance_fields_references<SV: SlotVisitor<ArtSlot>>(
        &self,
        klass: &Class,
        slot_visitor: &mut SV,
    ) {
        let mut visit_one_word = |mut field_offset: u32, mut ref_offsets: u32| {
            while ref_offsets != 0 {
                if (ref_offsets & 0b1) != 0 {
                    slot_visitor.visit_slot(get_field_slot(self, field_offset as i32));
                }
                ref_offsets >>= 1;
                field_offset += ART_HEAP_REFERENCE_SIZE as u32;
            }
        };

        let ref_offsets = klass.get_reference_instance_offsets();
        debug_assert_ne!(ref_offsets, 0);
        if unlikely((ref_offsets & Class::kVisitReferencesSlowpathMask) != 0) {
            // #[cfg(debug_assertions)]
            // klass.verify_overflow_reference_bitmap();
            let bitmap_num_words = ref_offsets & !Class::kVisitReferencesSlowpathMask;
            let klass_obj: &Object = klass.into();
            let klass_objref: ObjectReference = klass_obj.into();
            let overflow_bitmap: *const u32 = (klass_objref.to_raw_address().as_usize() + klass.class_size as usize
                - (bitmap_num_words * (std::mem::size_of::<u32>() as u32)) as usize) as *const u32;
            for i in 0..bitmap_num_words {
                visit_one_word(
                    OBJECT_HEADER_SIZE + i * ART_HEAP_REFERENCE_SIZE as u32 * 32_u32,
                    // SAFETY: overflow_bitmap is a valid pointer since it is constructed from the klass
                    unsafe { *overflow_bitmap.add(i as usize) },
                );
            }
        } else {
            visit_one_word(OBJECT_HEADER_SIZE, ref_offsets);
        }
    }
}

impl Class {
    /// Visit references in an instance of a java.lang.Class.
    #[allow(non_upper_case_globals)]
    #[inline]
    pub fn visit_references<const kVisitNativeRoots: bool, SV: SlotVisitor<ArtSlot>>(
        &self,
        object: ObjectReference,
        slot_visitor: &mut SV,
    ) {
        // XXX(kunals): Note that we visit the instance fields in the scan_object function itself
        if self.is_resolved() {
            self.visit_static_fields_references(slot_visitor);
        }
        if kVisitNativeRoots {
            // SAFETY: Assumes upcalls is valid
            unsafe {
                ((*UPCALLS).scan_native_roots)(
                    object,
                    to_scan_object_closure::<SV>(slot_visitor),
                    ArtObjectWithNativeRootsType::Class,
                );
            }
        }
    }

    /// Visit static reference fields in an instance of a java.lang.Class.
    #[inline]
    fn visit_static_fields_references<SV: SlotVisitor<ArtSlot>>(&self, slot_visitor: &mut SV) {
        debug_assert!(!self.is_temp());
        debug_assert!(self.is_resolved());
        let num_reference_fields = self.num_reference_static_fields;
        if num_reference_fields > 0_u32 {
            let obj: &Object = self.into();
            let mut field_offset = self.get_first_reference_static_fields_offset(crate::ART_POINTER_SIZE.load(Ordering::Relaxed));
            for _i in 0..num_reference_fields {
                debug_assert_ne!(field_offset, Object::class_offset());
                let field = get_field_slot(obj, field_offset as i32);
                slot_visitor.visit_slot(field);
                field_offset += ART_HEAP_REFERENCE_SIZE;
            }
        }
    }
}

impl<T> ObjectArray<T> {
    /// Visit references in an object array.
    #[inline]
    pub fn visit_references<SV: SlotVisitor<ArtSlot>>(&self, slot_visitor: &mut SV) {
        let length = self.array.length as usize;
        for i in 0..length {
            let obj: &Object = self.into();
            let element = get_field_slot(obj, self.offset_of_element(i) as i32);
            slot_visitor.visit_slot(element);
        }
    }
}

impl Reference {
    /// Visit the referent of a reference object.
    pub fn visit_referent<SV: SlotVisitor<ArtSlot>>(
        object: ObjectReference,
        klass: &ArtHeapReference<Class>,
        slot_visitor: &mut SV,
    ) {
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).process_referent)(
                klass.into(),
                object,
                to_scan_object_closure::<SV>(slot_visitor),
            );
        }
    }
}

impl DexCache {
    /// Visit native roots in an instance of a java.lang.DexCache object.
    pub fn visit_native_roots<SV: SlotVisitor<ArtSlot>>(
        object: ObjectReference,
        slot_visitor: &mut SV,
    ) {
        // XXX(kunals): Note that we visit the instance fields in the scan_object function itself
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).scan_native_roots)(
                object,
                to_scan_object_closure::<SV>(slot_visitor),
                ArtObjectWithNativeRootsType::DexCache,
            );
        }
    }
}

impl ClassLoader {
    /// Visit Classes in an instance of a java.lang.ClassLoader object.
    pub fn visit_classes<SV: SlotVisitor<ArtSlot>>(
        object: ObjectReference,
        slot_visitor: &mut SV,
    ) {
        // XXX(kunals): Note that we visit the instance fields in the scan_object function itself
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).scan_native_roots)(
                object,
                to_scan_object_closure::<SV>(slot_visitor),
                ArtObjectWithNativeRootsType::ClassLoader,
            );
        }
    }
}

/// Efficiently scan an object.
#[allow(non_upper_case_globals)]
#[inline]
pub fn scan_object<const kVisitNativeRoots: bool, SV: SlotVisitor<ArtSlot>>(
    _tls: VMWorkerThread,
    object: ObjectReference,
    slot_visitor: &mut SV,
) {
    let obj: &Object = object.into();
    let klass = &obj.get_class();
    let class_flags = klass.class_flags;

    // Visit the klass slot
    slot_visitor.visit_slot(obj.klass_slot());

    // Visit instance fields if the object is an instance of a normal class or a record class
    if likely(class_flags == Class::kClassFlagNormal) || class_flags == Class::kClassFlagRecord {
        klass.check_normal_class();
        debug_assert!(klass.is_instantiable_non_array());
        obj.visit_instance_fields_references(klass, slot_visitor);
        return;
    }

    // Return early if the object has no reference fields
    if class_flags & Class::kClassFlagNoReferenceFields != 0 {
        klass.check_no_reference_fields();
        return;
    }

    // We should have processed the java.lang.String class already
    debug_assert!(!klass.is_string_class());
    // If we are an instance of java.lang.Class
    if class_flags == Class::kClassFlagClass {
        debug_assert!(klass.is_class_class());
        debug_assert!(klass.is_instantiable_non_array());
        obj.visit_instance_fields_references(klass, slot_visitor);
        let as_klass: &Class = obj.into();
        as_klass.visit_references::<kVisitNativeRoots, SV>(object, slot_visitor);
        return;
    }

    // Visit the array elements if we are is an object array
    if class_flags & Class::kClassFlagObjectArray != 0 {
        debug_assert!(klass.is_object_array_class());
        let as_obj_array: &ObjectArray<Object> = obj.into();
        as_obj_array.visit_references(slot_visitor);
        return;
    }

    // Visit the referent if we are an instance java.lang.Reference
    if class_flags & Class::kClassFlagReference != 0 {
        debug_assert!(klass.is_instantiable_non_array());
        obj.visit_instance_fields_references(klass, slot_visitor);
        Reference::visit_referent(object, klass, slot_visitor);
        return;
    }

    // If we are an instance of java.lang.DexCache
    if class_flags & Class::kClassFlagDexCache != 0 {
        debug_assert!(klass.is_instantiable_non_array());
        debug_assert!(klass.is_dexcache_class());
        obj.visit_instance_fields_references(klass, slot_visitor);
        if kVisitNativeRoots {
            DexCache::visit_native_roots(object, slot_visitor);
        }
        return;
    }

    // If we are an instance of java.lang.ClassLoader
    if class_flags & Class::kClassFlagClassLoader != 0 {
        debug_assert!(klass.is_instantiable_non_array());
        debug_assert!(klass.is_classloader_class());
        obj.visit_instance_fields_references(klass, slot_visitor);
        if kVisitNativeRoots {
            ClassLoader::visit_classes(object, slot_visitor);
        }
        return;
    }

    panic!("Unexpected class flags: {:x} for {:?}", class_flags, klass);
}