//*----------------------------------------------------------------------------
// Derive marco tests
//*----------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};
    use libpq_serde_macros::SerdeLibpqData;
    use libpq_serde_types::{ByteSized, Deserialize, Serialize};

    //*------------------------------------------------------------------------
    // Macro serde: implementation of Serialize
    //*------------------------------------------------------------------------
    #[test]
    fn macro_serde_serialize_vec_i16_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            vec16: Vec<i32>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding { vec16: vec![] }.serialize(&mut buffer)?;
        assert_eq!(vec![0_u8, 0], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_vec_i16_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            vec16: Vec<i32>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            vec16: vec![0, 1, 2],
        }
        .serialize(&mut buffer)?;
        assert_eq!(
            vec![0_u8, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2],
            buffer.to_vec()
        );

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_vec_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            vec32: Vec<i32>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding { vec32: vec![] }.serialize(&mut buffer)?;
        assert_eq!(vec![0_u8, 0, 0, 0], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_vec_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            vec32: Vec<i32>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            vec32: vec![0, 1, 2],
        }
        .serialize(&mut buffer)?;
        assert_eq!(
            vec![0_u8, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2],
            buffer.to_vec()
        );

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_vec_null_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "null")]
            vecnull: Vec<String>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            vecnull: Vec::new(),
        }
        .serialize(&mut buffer)?;
        assert_eq!(vec![0], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_vec_null_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "null")]
            vecnull: Vec<String>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            vecnull: vec!["un".into(), "deux".into(), "trois".into()],
        }
        .serialize(&mut buffer)?;
        assert_eq!(
            vec![
                117, 110, 0, 100, 101, 117, 120, 0, 116, 114, 111, 105, 115, 0, 0
            ],
            buffer.to_vec()
        );

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_bytes_i16_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            bytes16: Bytes,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            bytes16: Bytes::from(vec![]),
        }
        .serialize(&mut buffer)?;
        assert_eq!(vec![0_u8, 0], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_bytes_i16_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            bytes16: Bytes,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            bytes16: Bytes::from(vec![1_u8, 2, 3, 4]),
        }
        .serialize(&mut buffer)?;
        assert_eq!(vec![0_u8, 4, 1, 2, 3, 4], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_bytes_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            bytes32: Bytes,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            bytes32: Bytes::from(vec![]),
        }
        .serialize(&mut buffer)?;
        assert_eq!(vec![0_u8, 0, 0, 0], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_bytes_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            bytes32: Bytes,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            bytes32: Bytes::from(vec![1_u8, 2, 3, 4]),
        }
        .serialize(&mut buffer)?;
        assert_eq!(vec![0_u8, 0, 0, 4, 1, 2, 3, 4], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_opt_vec16_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_vec16: Option<Vec<i32>>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding { opt_vec16: None }.serialize(&mut buffer)?;
        assert_eq!(vec![0xFF, 0xFF], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_opt_vec16_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_vec16: Option<Vec<i32>>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            opt_vec16: Some(vec![0, 1, 2]),
        }
        .serialize(&mut buffer)?;
        assert_eq!(
            vec![0_u8, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2],
            buffer.to_vec()
        );

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_opt_vec32_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_vec32: Option<Vec<i32>>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding { opt_vec32: None }.serialize(&mut buffer)?;
        assert_eq!(vec![0xFF, 0xFF, 0xFF, 0xFF], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_opt_vec32_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_vec32: Option<Vec<i32>>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            opt_vec32: Some(vec![0, 1, 2]),
        }
        .serialize(&mut buffer)?;
        assert_eq!(
            vec![0_u8, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2],
            buffer.to_vec()
        );

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_opt_bytes_i16_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_bytes16: Option<Bytes>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding { opt_bytes16: None }.serialize(&mut buffer)?;
        assert_eq!(vec![0xFF, 0xFF], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_opt_bytes_i16_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_bytes16: Option<Bytes>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            opt_bytes16: Some(Bytes::from(vec![1_u8, 2, 3, 4])),
        }
        .serialize(&mut buffer)?;
        assert_eq!(vec![0_u8, 4, 1, 2, 3, 4], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_opt_bytes_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_bytes32: Option<Bytes>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding { opt_bytes32: None }.serialize(&mut buffer)?;
        assert_eq!(vec![0xFF, 0xFF, 0xFF, 0xFF], buffer.to_vec());

        Ok(())
    }

    #[test]
    fn macro_serde_serialize_opt_bytes_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_bytes32: Option<Bytes>,
        }

        let mut buffer = BytesMut::new();
        TestLengthEncoding {
            opt_bytes32: Some(Bytes::from(vec![1_u8, 2, 3, 4])),
        }
        .serialize(&mut buffer)?;
        assert_eq!(vec![0_u8, 0, 0, 4, 1, 2, 3, 4], buffer.to_vec());

        Ok(())
    }
    //*------------------------------------------------------------------------
    // Macro serde: implementation of Deserialize
    //*------------------------------------------------------------------------
    #[test]
    fn macro_serde_deserialize_vec_i16_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            vec16: Vec<i32>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0_u8, 0]))?,
            TestLengthEncoding { vec16: vec![] }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_vec_i16_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            vec16: Vec<i32>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[
                0_u8, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2
            ]))?,
            TestLengthEncoding {
                vec16: vec![0, 1, 2],
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_vec_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            vec32: Vec<i32>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0_u8, 0, 0, 0]))?,
            TestLengthEncoding { vec32: vec![] }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_vec_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            vec32: Vec<i32>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[
                0_u8, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2
            ]))?,
            TestLengthEncoding {
                vec32: vec![0, 1, 2],
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_vec_null_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "null")]
            vecnull: Vec<String>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0]))?,
            TestLengthEncoding {
                vecnull: Vec::new(),
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_vec_null_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "null")]
            vecnull: Vec<String>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[
                117, 110, 0, 100, 101, 117, 120, 0, 116, 114, 111, 105, 115, 0, 0
            ]))?,
            TestLengthEncoding {
                vecnull: vec!["un".into(), "deux".into(), "trois".into()],
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_bytes_i16_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            bytes16: Bytes,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0_u8, 0]))?,
            TestLengthEncoding {
                bytes16: Bytes::from(vec![]),
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_bytes_i16_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            bytes16: Bytes,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0_u8, 4, 1, 2, 3, 4]))?,
            TestLengthEncoding {
                bytes16: Bytes::from(vec![1_u8, 2, 3, 4]),
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_bytes_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            bytes32: Bytes,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0_u8, 0, 0, 0]))?,
            TestLengthEncoding {
                bytes32: Bytes::from(vec![]),
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_bytes_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            bytes32: Bytes,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0_u8, 0, 0, 4, 1, 2, 3, 4]))?,
            TestLengthEncoding {
                bytes32: Bytes::from(vec![1_u8, 2, 3, 4]),
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_opt_vec16_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_vec16: Option<Vec<i32>>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0xFF, 0xFF]))?,
            TestLengthEncoding { opt_vec16: None }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_opt_vec16_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_vec16: Option<Vec<i32>>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[
                0_u8, 2, 0, 0, 0, 0, 0, 0, 0, 1
            ]))?,
            TestLengthEncoding {
                opt_vec16: Some(vec![0, 1]),
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_opt_vec32_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_vec32: Option<Vec<i32>>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?,
            TestLengthEncoding { opt_vec32: None }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_opt_vec32_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_vec32: Option<Vec<i32>>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[
                0_u8, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1
            ]))?,
            TestLengthEncoding {
                opt_vec32: Some(vec![0, 1]),
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_opt_bytes_i16_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_bytes16: Option<Bytes>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0xFF, 0xFF]))?,
            TestLengthEncoding { opt_bytes16: None }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_opt_bytes_i16_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_bytes16: Option<Bytes>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0_u8, 4, 1, 2, 3, 4]))?,
            TestLengthEncoding {
                opt_bytes16: Some(Bytes::from(vec![1_u8, 2, 3, 4])),
            }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_opt_bytes_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_bytes32: Option<Bytes>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))?,
            TestLengthEncoding { opt_bytes32: None }
        );

        Ok(())
    }

    #[test]
    fn macro_serde_deserialize_opt_bytes_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_bytes32: Option<Bytes>,
        }

        assert_eq!(
            TestLengthEncoding::deserialize(&mut Bytes::from_static(&[0_u8, 0, 0, 4, 1, 2, 3, 4]))?,
            TestLengthEncoding {
                opt_bytes32: Some(Bytes::from(vec![1_u8, 2, 3, 4])),
            }
        );

        Ok(())
    }

    //*------------------------------------------------------------------------
    // Macro serde: implementation of ByteSized
    //*------------------------------------------------------------------------
    #[test]
    fn macro_serde_bytesized_vec_i16_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            vec16: Vec<i32>,
        }

        assert_eq!(TestLengthEncoding { vec16: vec![] }.byte_size(), 2);

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_vec_i16_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            vec16: Vec<i32>,
        }

        assert_eq!(
            TestLengthEncoding {
                vec16: vec![0, 1, 2],
            }
            .byte_size(),
            2 + 4 + 4 + 4
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_vec_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            vec32: Vec<i32>,
        }

        assert_eq!(TestLengthEncoding { vec32: vec![] }.byte_size(), 4);

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_vec_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            vec32: Vec<i32>,
        }

        assert_eq!(
            TestLengthEncoding {
                vec32: vec![0, 1, 2],
            }
            .byte_size(),
            4 + 4 + 4 + 4
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_vec_null_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "null")]
            vecnull: Vec<String>,
        }

        assert_eq!(
            TestLengthEncoding {
                vecnull: Vec::new(),
            }
            .byte_size(),
            1
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_vec_null_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "null")]
            vecnull: Vec<String>,
        }

        assert_eq!(
            TestLengthEncoding {
                vecnull: vec!["un".into(), "deux".into(), "trois".into()],
            }
            .byte_size(),
            3 + 5 + 6 + 1
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_bytes_i16_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            bytes16: Bytes,
        }

        assert_eq!(
            TestLengthEncoding {
                bytes16: Bytes::from(vec![]),
            }
            .byte_size(),
            2
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_bytes_i16_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            bytes16: Bytes,
        }

        assert_eq!(
            TestLengthEncoding {
                bytes16: Bytes::from(vec![1, 2, 3, 4]),
            }
            .byte_size(),
            2 + 4
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_bytes_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            bytes32: Bytes,
        }

        assert_eq!(
            TestLengthEncoding {
                bytes32: Bytes::from(vec![]),
            }
            .byte_size(),
            4
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_bytes_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            bytes32: Bytes,
        }

        assert_eq!(
            TestLengthEncoding {
                bytes32: Bytes::from(vec![1_u8, 2, 3, 4]),
            }
            .byte_size(),
            4 + 4
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_opt_vec16_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_vec16: Option<Vec<i32>>,
        }

        assert_eq!(TestLengthEncoding { opt_vec16: None }.byte_size(), 2);

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_opt_vec16_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_vec16: Option<Vec<i32>>,
        }

        assert_eq!(
            TestLengthEncoding {
                opt_vec16: Some(vec![1, 2, 3]),
            }
            .byte_size(),
            2 + 4 * 3
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_opt_vec32_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_vec32: Option<Vec<i32>>,
        }

        assert_eq!(TestLengthEncoding { opt_vec32: None }.byte_size(), 4);

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_opt_vec32_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_vec32: Option<Vec<i32>>,
        }

        assert_eq!(
            TestLengthEncoding {
                opt_vec32: Some(vec![1, 2, 3]),
            }
            .byte_size(),
            4 + 4 * 3
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_opt_bytes16_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_bytes16: Option<Bytes>,
        }

        assert_eq!(TestLengthEncoding { opt_bytes16: None }.byte_size(), 2);

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_opt_bytes16_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i16")]
            opt_bytes16: Option<Bytes>,
        }

        assert_eq!(
            TestLengthEncoding {
                opt_bytes16: Some(Bytes::from(vec![1_u8, 2, 3])),
            }
            .byte_size(),
            2 + 3
        );

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_opt_bytes32_i32_empty() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_bytes32: Option<Bytes>,
        }

        assert_eq!(TestLengthEncoding { opt_bytes32: None }.byte_size(), 4);

        Ok(())
    }

    #[test]
    fn macro_serde_bytesized_opt_bytes32_i32_with_data() -> anyhow::Result<()> {
        #[derive(Debug, PartialEq, SerdeLibpqData)]
        struct TestLengthEncoding {
            #[serde_libpq(length_encoding = "i32")]
            opt_bytes32: Option<Bytes>,
        }

        assert_eq!(
            TestLengthEncoding {
                opt_bytes32: Some(Bytes::from(vec![1_u8, 2, 3])),
            }
            .byte_size(),
            4 + 3
        );

        Ok(())
    }
}
