use bytes::{Buf, Bytes, BytesMut};

pub mod libpq_types;

pub trait Serialize {
    fn serialize(&self, buffer: &mut BytesMut);
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

#[cfg(test)]
mod tests {
    use super::*;
    use libpq_serde_macros::*;
    use libpq_types::*;
    use std::ffi::CString;

    #[derive(Debug, PartialEq, SerdeLibpqData)]
    struct AllTypes {
        byte: Byte,
        byte4: Byte4,
        int_16: i16,
        int_32: i32,
        cstring: CString,
        vec16_cstring: Vec16<CString>,
        vec32_bytes: Vec32<Byte>,
    }

    fn example_struct() -> AllTypes {
        AllTypes {
            byte: 0x01,
            byte4: [0x00, 0x00, 0x00, 0x00],
            int_16: 125,
            int_32: 521,
            cstring: CString::new("aldabis").expect("No 0x00 in string"),
            vec16_cstring: vec![
                CString::new("aldabis").expect("There is no 0x00 inside"),
                CString::new("aldabis").expect("There is no 0x00 inside"),
            ]
            .into(),
            vec32_bytes: vec![0x01, 0x02].into(),
        }
    }

    fn example_from_serialize() -> BytesMut {
        let mut m = BytesMut::new();

        (1 as Byte).serialize(&mut m);
        ([0x00, 0x00, 0x00, 0x00] as Byte4).serialize(&mut m);
        125i16.serialize(&mut m);
        521i32.serialize(&mut m);
        CString::new("aldabis")
            .expect("No 0x00 in string")
            .serialize(&mut m);
        Vec16::<CString>::from(vec![
            CString::new("aldabis").expect("There is no 0x00 inside"),
            CString::new("aldabis").expect("There is no 0x00 inside"),
        ])
        .serialize(&mut m);
        Vec32::<Byte>::from(vec![0x01, 0x02]).serialize(&mut m);

        m
    }

    #[test]
    fn derive_macro_serialize_struct() -> anyhow::Result<()> {
        let s = example_struct();

        let mut m = Bytes::from(example_from_serialize());
        assert_eq!(s, <AllTypes>::deserialize(&mut m)?);

        Ok(())
    }

    #[test]
    fn derive_macro_deserialize_struct() -> anyhow::Result<()> {
        let b = example_struct();
        let m = example_from_serialize();

        assert_eq!(b.byte_size(), m.len() as i32);

        Ok(())
    }

    #[test]
    fn derive_macro_bytesize_struct() -> anyhow::Result<()> {
        let b = example_struct();
        let m = example_from_serialize();

        assert_eq!(b.byte_size(), m.len() as i32);

        Ok(())
    }
}
