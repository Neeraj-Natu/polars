use std::sync::Arc;

use super::{make_growable, Growable};
use crate::array::growable::utils::{extend_validity, prepare_validity};
use crate::array::{Array, MapArray};
use crate::bitmap::MutableBitmap;
use crate::offset::Offsets;

fn extend_offset_values(growable: &mut GrowableMap<'_>, index: usize, start: usize, len: usize) {
    let array = growable.arrays[index];
    let offsets = array.offsets();

    growable
        .offsets
        .try_extend_from_slice(offsets, start, len)
        .unwrap();

    let end = offsets.buffer()[start + len] as usize;
    let start = offsets.buffer()[start] as usize;
    let len = end - start;
    growable.values.extend(index, start, len);
}

/// Concrete [`Growable`] for the [`MapArray`].
pub struct GrowableMap<'a> {
    arrays: Vec<&'a MapArray>,
    validity: Option<MutableBitmap>,
    values: Box<dyn Growable<'a> + 'a>,
    offsets: Offsets<i32>,
}

impl<'a> GrowableMap<'a> {
    /// Creates a new [`GrowableMap`] bound to `arrays` with a pre-allocated `capacity`.
    /// # Panics
    /// If `arrays` is empty.
    pub fn new(arrays: Vec<&'a MapArray>, mut use_validity: bool, capacity: usize) -> Self {
        // if any of the arrays has nulls, insertions from any array requires setting bits
        // as there is at least one array with nulls.
        if !use_validity & arrays.iter().any(|array| array.null_count() > 0) {
            use_validity = true;
        };

        let inner = arrays
            .iter()
            .map(|array| array.field().as_ref())
            .collect::<Vec<_>>();
        let values = make_growable(&inner, use_validity, 0);

        Self {
            arrays,
            offsets: Offsets::with_capacity(capacity),
            values,
            validity: prepare_validity(use_validity, capacity),
        }
    }

    fn to(&mut self) -> MapArray {
        let validity = std::mem::take(&mut self.validity);
        let offsets = std::mem::take(&mut self.offsets);
        let values = self.values.as_box();

        MapArray::new(
            self.arrays[0].data_type().clone(),
            offsets.into(),
            values,
            validity.map(|v| v.into()),
        )
    }
}

impl<'a> Growable<'a> for GrowableMap<'a> {
    fn extend(&mut self, index: usize, start: usize, len: usize) {
        let array = self.arrays[index];
        extend_validity(&mut self.validity, array, start, len);
        extend_offset_values(self, index, start, len);
    }

    fn extend_validity(&mut self, additional: usize) {
        self.offsets.extend_constant(additional);
        if let Some(validity) = &mut self.validity {
            validity.extend_constant(additional, false);
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    fn as_arc(&mut self) -> Arc<dyn Array> {
        Arc::new(self.to())
    }

    fn as_box(&mut self) -> Box<dyn Array> {
        Box::new(self.to())
    }
}

impl<'a> From<GrowableMap<'a>> for MapArray {
    fn from(mut val: GrowableMap<'a>) -> Self {
        val.to()
    }
}
