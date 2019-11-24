#![no_std]

pub use ::num_enum_derive::{IntoPrimitive, TryFromPrimitive, UnsafeFromPrimitive};

use ::core::fmt;

pub trait TryFromPrimitive: Sized {
    type Primitive: Copy + Eq + fmt::Debug;
    fn try_from_primitive(number: Self::Primitive) -> Result<Self, ()>;
}
