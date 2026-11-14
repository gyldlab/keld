//! Preserve original typed visitors while admitting their owned storage first.
use super::{
    DecodeError, State,
    access::{Admitted, Mapping, Sequence},
    layout,
};
use serde::Deserializer;
use serde::de::{Error, MapAccess, SeqAccess, Visitor};
use std::fmt;

pub(super) struct AdmittedVisitor<'a, 'b, V, F: FnMut(usize) -> bool> {
    pub(super) inner: V,
    pub(super) state: &'a mut State<'b, F>,
}
macro_rules! scalar {
    ($method:ident, $ty:ty) => {
        fn $method<E: Error>(self, value: $ty) -> Result<Self::Value, E> {
            let result = self.inner.$method::<DecodeError>(value);
            self.state.outer(result)
        }
    };
}
impl<'de, V: Visitor<'de>, F: FnMut(usize) -> bool> Visitor<'de> for AdmittedVisitor<'_, '_, V, F> {
    type Value = V::Value;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.expecting(formatter)
    }
    scalar!(visit_bool, bool);
    scalar!(visit_i8, i8);
    scalar!(visit_i16, i16);
    scalar!(visit_i32, i32);
    scalar!(visit_i64, i64);
    scalar!(visit_u8, u8);
    scalar!(visit_u16, u16);
    scalar!(visit_u32, u32);
    scalar!(visit_u64, u64);
    scalar!(visit_f64, f64);
    scalar!(visit_char, char);

    fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
        let result = self.inner.visit_unit::<DecodeError>();
        self.state.outer(result)
    }
    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        let result = self.inner.visit_none::<DecodeError>();
        self.state.outer(result)
    }
    fn visit_some<D: Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        let result = self.inner.visit_some(Admitted {
            inner,
            state: &mut *self.state,
        });
        self.state.outer(result)
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        let result = self.inner.visit_newtype_struct(Admitted {
            inner,
            state: &mut *self.state,
        });
        self.state.outer(result)
    }
    fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
        let charge = layout::string_bytes(value.len())
            .ok_or(DecodeError::Storage)
            .and_then(|bytes| self.state.admit(bytes));
        self.state.outer::<E, _>(charge)?;
        let result = self.inner.visit_str::<DecodeError>(value);
        self.state.outer(result)
    }
    fn visit_borrowed_str<E: Error>(self, value: &'de str) -> Result<Self::Value, E> {
        let charge = layout::string_bytes(value.len())
            .ok_or(DecodeError::Storage)
            .and_then(|bytes| self.state.admit(bytes));
        self.state.outer::<E, _>(charge)?;
        let result = self.inner.visit_borrowed_str::<DecodeError>(value);
        self.state.outer(result)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        let result = self.inner.visit_seq(Sequence {
            inner,
            state: &mut *self.state,
            first: true,
        });
        self.state.outer(result)
    }
    fn visit_map<A: MapAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        let result = self.inner.visit_map(Mapping {
            inner,
            state: &mut *self.state,
            key: None,
        });
        self.state.outer(result)
    }
}
