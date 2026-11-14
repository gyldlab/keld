//! Field-blind serde access wrappers. They retain no parsed values.
use super::{DecodeError, State, layout, visitor::AdmittedVisitor};
use serde::Deserializer;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::alloc::Layout;

pub(super) struct Admitted<'a, 'b, D, F: FnMut(usize) -> bool> {
    pub(super) inner: D,
    pub(super) state: &'a mut State<'b, F>,
}

impl<'de, D: Deserializer<'de>, F: FnMut(usize) -> bool> Deserializer<'de>
    for Admitted<'_, '_, D, F>
{
    type Error = DecodeError;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let result = self.inner.deserialize_any(AdmittedVisitor {
            inner: visitor,
            state: &mut *self.state,
        });
        self.state.inner(result)
    }

    // Every method used by RawCompatEnvelope and LocalValueSeed keeps its
    // ORIGINAL typed Visitor. JSON's any dispatcher merely avoids allocating
    // an attacker-sized peek_invalid_type diagnostic before that visitor.
    // Integer range/type checks still execute in serde's primitive visitor.
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f64 char str string unit
        unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        let result = self.inner.deserialize_option(AdmittedVisitor {
            inner: visitor,
            state: &mut *self.state,
        });
        self.state.inner(result)
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let result = self.inner.deserialize_newtype_struct(
            name,
            AdmittedVisitor {
                inner: visitor,
                state: &mut *self.state,
            },
        );
        self.state.inner(result)
    }

    // The closed schema uses no external enum, binary, f32 or128-bit decoder.
    // Refuse those methods instead of claiming a general serde replacement.
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(DecodeError::Malformed)
    }
    fn deserialize_i128<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(DecodeError::Malformed)
    }
    fn deserialize_u128<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(DecodeError::Malformed)
    }
    fn deserialize_f32<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(DecodeError::Malformed)
    }
    fn deserialize_bytes<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(DecodeError::Malformed)
    }
    fn deserialize_byte_buf<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
        Err(DecodeError::Malformed)
    }
    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

pub(super) struct Seed<'a, 'b, T, F: FnMut(usize) -> bool> {
    inner: T,
    state: &'a mut State<'b, F>,
}
impl<'de, T: DeserializeSeed<'de>, F: FnMut(usize) -> bool> DeserializeSeed<'de>
    for Seed<'_, '_, T, F>
{
    type Value = T::Value;
    fn deserialize<D: Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        let result = self.inner.deserialize(Admitted {
            inner,
            state: &mut *self.state,
        });
        self.state.outer(result)
    }
}

pub(super) struct Sequence<'a, 'b, A, F: FnMut(usize) -> bool> {
    pub(super) inner: A,
    pub(super) state: &'a mut State<'b, F>,
    pub(super) first: bool,
}
impl<'de, A: SeqAccess<'de>, F: FnMut(usize) -> bool> SeqAccess<'de> for Sequence<'_, '_, A, F> {
    type Error = DecodeError;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        let bytes = layout::sequence_step(Layout::new::<T::Value>(), self.first)
            .ok_or(DecodeError::Storage)?;
        self.state.admit(bytes)?;
        self.first = false;
        let result = self.inner.next_element_seed(Seed {
            inner: seed,
            state: &mut *self.state,
        });
        self.state.inner(result)
    }
    fn size_hint(&self) -> Option<usize> {
        None
    }
}

pub(super) struct Mapping<'a, 'b, A, F: FnMut(usize) -> bool> {
    pub(super) inner: A,
    pub(super) state: &'a mut State<'b, F>,
    pub(super) key: Option<Layout>,
}
impl<'de, A: MapAccess<'de>, F: FnMut(usize) -> bool> MapAccess<'de> for Mapping<'_, '_, A, F> {
    type Error = DecodeError;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        if self.key.is_some() {
            return Err(DecodeError::Malformed);
        }
        let result = self.inner.next_key_seed(Seed {
            inner: seed,
            state: &mut *self.state,
        });
        let key = self.state.inner(result)?;
        if key.is_some() {
            self.key = Some(Layout::new::<K::Value>());
        }
        Ok(key)
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        let key = self.key.take().ok_or(DecodeError::Malformed)?;
        let bytes =
            layout::map_entry(key, Layout::new::<V::Value>()).ok_or(DecodeError::Storage)?;
        // Every BTree insert/Content pair push occurs after this value returns.
        // The key's owned text, if any, was already charged by its visitor.
        self.state.admit(bytes)?;
        let result = self.inner.next_value_seed(Seed {
            inner: seed,
            state: &mut *self.state,
        });
        self.state.inner(result)
    }
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
