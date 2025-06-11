use bytes::{Buf, Bytes, BytesMut};

pub mod libpq_types;

// This crate provides:
//
// * serialization / deserialization implementation for the basic types used in the messages
// * trait for serialization / deserialization
// * tests for the serialization / deserialization
// * tests for the crate serde-libpq-macros (since derive macro must be put in a separate crate
// and cannot be tested there

//*----------------------------------------------------------------------------
// Traits
//*----------------------------------------------------------------------------
pub trait Serialize {
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()>;
}

pub trait Deserialize {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf;
}

pub trait ByteSized {
    fn byte_size(&self) -> i32;
}
