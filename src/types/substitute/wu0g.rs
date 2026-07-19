//! Scalar-only counters for one WU0G substitution application.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SubstitutionAccumulator {
    visits: u64,
    current_depth: u64,
    max_depth: u64,
    cycle_reentries: u64,
    copied_objects: u64,
    copied_properties: u64,
    copied_property_name_bytes: u64,
    copied_signatures: u64,
    interner_attempts: u64,
    interner_hits: u64,
    interner_new_ids: u64,
    saturated: bool,
}

impl SubstitutionAccumulator {
    fn add(counter: &mut u64, value: u64, saturated: &mut bool) {
        *counter = match counter.checked_add(value) {
            Some(sum) => sum,
            None => {
                *saturated = true;
                u64::MAX
            }
        };
    }

    fn add_usize(counter: &mut u64, value: usize, saturated: &mut bool) {
        match u64::try_from(value) {
            Ok(value) => Self::add(counter, value, saturated),
            Err(_) => {
                *counter = u64::MAX;
                *saturated = true;
            }
        }
    }

    pub(super) fn record_visit_enter(&mut self) {
        Self::add(&mut self.visits, 1, &mut self.saturated);
        Self::add(&mut self.current_depth, 1, &mut self.saturated);
        self.max_depth = self.max_depth.max(self.current_depth);
    }

    pub(super) fn record_visit_exit(&mut self) {
        match self.current_depth.checked_sub(1) {
            Some(depth) => self.current_depth = depth,
            None => self.saturated = true,
        }
    }

    pub(super) fn record_cycle_reentry(&mut self) {
        Self::add(&mut self.cycle_reentries, 1, &mut self.saturated);
    }

    pub(super) fn record_object_copy(
        &mut self,
        property_name_byte_lengths: impl IntoIterator<Item = usize>,
        property_count: usize,
        call_signature_count: usize,
        construct_signature_count: usize,
    ) {
        let (property_name_bytes, signature_count, overflowed) = checked_object_copy_cardinality(
            property_name_byte_lengths,
            call_signature_count,
            construct_signature_count,
        );
        Self::add(&mut self.copied_objects, 1, &mut self.saturated);
        Self::add_usize(
            &mut self.copied_properties,
            property_count,
            &mut self.saturated,
        );
        Self::add_usize(
            &mut self.copied_property_name_bytes,
            property_name_bytes,
            &mut self.saturated,
        );
        Self::add_usize(
            &mut self.copied_signatures,
            signature_count,
            &mut self.saturated,
        );
        self.saturated |= overflowed;
    }

    pub(super) fn record_interner_attempt(&mut self, before: usize, after: usize) {
        Self::add(&mut self.interner_attempts, 1, &mut self.saturated);
        if after == before {
            Self::add(&mut self.interner_hits, 1, &mut self.saturated);
        } else if after > before {
            Self::add(&mut self.interner_new_ids, 1, &mut self.saturated);
        } else {
            self.saturated = true;
        }
    }

    pub(super) fn merge_from(&mut self, nested: Self) {
        for (target, value) in [
            (&mut self.visits, nested.visits),
            (&mut self.cycle_reentries, nested.cycle_reentries),
            (&mut self.copied_objects, nested.copied_objects),
            (&mut self.copied_properties, nested.copied_properties),
            (
                &mut self.copied_property_name_bytes,
                nested.copied_property_name_bytes,
            ),
            (&mut self.copied_signatures, nested.copied_signatures),
            (&mut self.interner_attempts, nested.interner_attempts),
            (&mut self.interner_hits, nested.interner_hits),
            (&mut self.interner_new_ids, nested.interner_new_ids),
        ] {
            Self::add(target, value, &mut self.saturated);
        }
        self.max_depth = self.max_depth.max(
            self.current_depth
                .checked_add(nested.max_depth)
                .unwrap_or_else(|| {
                    self.saturated = true;
                    u64::MAX
                }),
        );
        self.saturated |= nested.saturated || nested.current_depth != 0;
    }

    pub(super) fn into_parts(self) -> ([u64; 11], bool) {
        (
            [
                self.visits,
                self.current_depth,
                self.max_depth,
                self.cycle_reentries,
                self.copied_objects,
                self.copied_properties,
                self.copied_property_name_bytes,
                self.copied_signatures,
                self.interner_attempts,
                self.interner_hits,
                self.interner_new_ids,
            ],
            self.saturated,
        )
    }
}

pub(super) fn checked_object_copy_cardinality(
    property_name_byte_lengths: impl IntoIterator<Item = usize>,
    call_signature_count: usize,
    construct_signature_count: usize,
) -> (usize, usize, bool) {
    let property_name_bytes = property_name_byte_lengths
        .into_iter()
        .try_fold(0_usize, usize::checked_add);
    let signature_count = call_signature_count.checked_add(construct_signature_count);
    match (property_name_bytes, signature_count) {
        (Some(property_name_bytes), Some(signature_count)) => {
            (property_name_bytes, signature_count, false)
        }
        (property_name_bytes, signature_count) => (
            property_name_bytes.unwrap_or(usize::MAX),
            signature_count.unwrap_or(usize::MAX),
            true,
        ),
    }
}
