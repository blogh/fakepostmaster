//*----------------------------------------------------------------------------
// Derive marco tests
//*----------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};
    use libpq_serde_macros::SerdeLibpqData;
    use libpq_serde_types::libpq_types::{Byte, Byte4, Length16, Length32, VecWithEncoding};
    use libpq_serde_types::{ByteSized, Deserialize, Serialize};

    #[derive(Debug, PartialEq, SerdeLibpqData)]
    struct AllTypes {
        byte: Byte,
        byte4: Byte4,
        int_16: i16,
        int_32: i32,
        cstring: String,
        vec16_string: VecWithEncoding<String, Length16>,
        vec32_bytes: VecWithEncoding<Byte, Length32>,
    }

    fn example_struct() -> AllTypes {
        AllTypes {
            byte: 0x01,
            byte4: [0x00, 0x00, 0x00, 0x00],
            int_16: 125,
            int_32: 521,
            cstring: String::from("aldabis"),
            vec16_string: vec![String::from("aldabis"), String::from("aldabis")].into(),
            vec32_bytes: vec![0x01, 0x02].into(),
        }
    }

    fn example_from_serialize() -> anyhow::Result<BytesMut> {
        let mut m = BytesMut::new();

        (1 as Byte).serialize(&mut m)?;
        ([0x00, 0x00, 0x00, 0x00] as Byte4).serialize(&mut m)?;
        125i16.serialize(&mut m)?;
        521i32.serialize(&mut m)?;
        String::from("aldabis").serialize(&mut m)?;
        VecWithEncoding::<String, Length16>::from(vec![
            String::from("aldabis"),
            String::from("aldabis"),
        ])
        .serialize(&mut m)?;
        VecWithEncoding::<Byte, Length32>::from(vec![0x01, 0x02]).serialize(&mut m)?;

        Ok(m)
    }

    #[test]
    fn derive_macro_serialize_struct() -> anyhow::Result<()> {
        let s = example_struct();

        let mut m = Bytes::from(example_from_serialize()?);
        assert_eq!(s, <AllTypes>::deserialize(&mut m)?);

        Ok(())
    }

    #[test]
    fn derive_macro_deserialize_struct() -> anyhow::Result<()> {
        let b = example_struct();
        let m = example_from_serialize()?;

        assert_eq!(b.byte_size(), m.len() as i32);

        Ok(())
    }

    #[test]
    fn derive_macro_bytesize_struct() -> anyhow::Result<()> {
        let b = example_struct();
        let m = example_from_serialize()?;

        assert_eq!(b.byte_size(), m.len() as i32);

        Ok(())
    }
}
