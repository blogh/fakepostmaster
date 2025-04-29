use anyhow::anyhow;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::ffi::CString;

use crate::{ByteSized, Deserialize, Serialize};

// the list of types can be found here:
// https://www.postgresql.org/docs/17/protocol-message-types.html

//--------------------------------------------------------------------------------
impl Serialize for i8 {
    fn serialize(&self, buffer: &mut BytesMut) {
        buffer.put_i8(*self);
    }
}

impl Deserialize for i8 {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        buffer.try_get_i8().map_err(|e| e.into())
    }
}

impl ByteSized for i8 {
    fn byte_size(&self) -> i32 {
        1
    }
}

//--------------------------------------------------------------------------------
impl Serialize for i16 {
    fn serialize(&self, buffer: &mut BytesMut) {
        buffer.put_i16(*self);
    }
}

impl Deserialize for i16 {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        buffer.try_get_i16().map_err(|e| e.into())
    }
}

impl ByteSized for i16 {
    fn byte_size(&self) -> i32 {
        2
    }
}

//--------------------------------------------------------------------------------
impl Serialize for i32 {
    fn serialize(&self, buffer: &mut BytesMut) {
        buffer.put_i32(*self);
    }
}

impl Deserialize for i32 {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        buffer.try_get_i32().map_err(|e| e.into())
    }
}

impl ByteSized for i32 {
    fn byte_size(&self) -> i32 {
        4
    }
}

//--------------------------------------------------------------------------------
pub type Byte = u8;

impl Serialize for Byte {
    fn serialize(&self, buffer: &mut BytesMut) {
        buffer.put_u8(*self);
    }
}

impl Deserialize for Byte {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        buffer.try_get_u8().map_err(|e| e.into())
    }
}

impl ByteSized for Byte {
    fn byte_size(&self) -> i32 {
        1
    }
}

//--------------------------------------------------------------------------------
//FIXME:keep ? if yes => test
pub type Byte4 = [u8; 4];

impl Serialize for Byte4 {
    fn serialize(&self, buffer: &mut BytesMut) {
        buffer.put_slice(self);
    }
}

impl Deserialize for Byte4 {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        let mut t = [0_u8; 4];
        buffer.try_copy_to_slice(&mut t)?;
        Ok(t)
    }
}

impl ByteSized for Byte4 {
    fn byte_size(&self) -> i32 {
        4
    }
}

//--------------------------------------------------------------------------------
impl Serialize for CString {
    fn serialize(&self, buffer: &mut BytesMut) {
        buffer.put_slice(self.as_bytes());
        buffer.put_u8(0);
    }
}

impl Deserialize for CString {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        let mut v = Vec::new();
        let mut c: u8 = buffer.try_get_u8()?;

        while c != 0_u8 {
            v.push(c);
            c = buffer.try_get_u8()?;
        }

        // This operation is safe because we stopped copying data when
        // we reached the first 0x00 therefore there is no 0x00 in the
        // middle of the CString
        Ok(unsafe { CString::from_vec_unchecked(v) })
    }
}

impl ByteSized for CString {
    fn byte_size(&self) -> i32 {
        self.count_bytes() as i32 + 1
    }
}

//--------------------------------------------------------------------------------
/// An array where the length is encoded on 16 bit
#[derive(Debug, PartialEq)]
pub struct Vec16<T>(Vec<T>);

impl<T> Vec16<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

impl<T> From<Vec<T>> for Vec16<T> {
    fn from(item: Vec<T>) -> Vec16<T> {
        Vec16(item)
    }
}

impl<T> AsRef<Vec<T>> for Vec16<T> {
    fn as_ref(&self) -> &Vec<T> {
        &self.0
    }
}

impl<T> AsMut<Vec<T>> for Vec16<T> {
    fn as_mut(&mut self) -> &mut Vec<T> {
        self.0.as_mut()
    }
}

impl<T> Serialize for Vec16<T>
where
    T: Serialize,
{
    fn serialize(&self, buffer: &mut BytesMut) {
        // length
        (self.0.len() as i16).serialize(buffer);
        // data
        for elt in &self.0 {
            elt.serialize(buffer);
        }
    }
}

impl<T> Deserialize for Vec16<T>
where
    T: Deserialize,
{
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        let mut v = Self::new();
        let len = buffer.try_get_i16()?;
        for _ in 0..len {
            v.0.push(T::deserialize(buffer)?);
        }
        Ok(v)
    }
}

impl<T> ByteSized for Vec16<T>
where
    T: ByteSized,
{
    fn byte_size(&self) -> i32 {
        let mut size = 2;
        for elt in &self.0 {
            size += elt.byte_size();
        }
        size
    }
}

//--------------------------------------------------------------------------------
//TODO: when it works implement from []
/// An array where the length is encoded on 32 bit
#[derive(Debug, PartialEq)]
pub struct Vec32<T>(Vec<T>);

impl<T> Vec32<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

impl<T> From<Vec<T>> for Vec32<T> {
    fn from(item: Vec<T>) -> Vec32<T> {
        Vec32(item)
    }
}

impl<T> AsRef<Vec<T>> for Vec32<T> {
    fn as_ref(&self) -> &Vec<T> {
        &self.0
    }
}

impl<T> AsMut<Vec<T>> for Vec32<T> {
    fn as_mut(&mut self) -> &mut Vec<T> {
        self.0.as_mut()
    }
}

impl<T> Serialize for Vec32<T>
where
    T: Serialize,
{
    fn serialize(&self, buffer: &mut BytesMut) {
        // length
        (self.0.len() as i32).serialize(buffer);
        // data
        for elt in &self.0 {
            elt.serialize(buffer);
        }
    }
}

impl<T> Deserialize for Vec32<T>
where
    T: Deserialize,
{
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        let mut v = Self::new();
        let len = buffer.try_get_i32()?;
        for _ in 0..len {
            v.0.push(T::deserialize(buffer)?);
        }
        Ok(v)
    }
}

impl<T> ByteSized for Vec32<T>
where
    T: ByteSized,
{
    fn byte_size(&self) -> i32 {
        let mut size = 4;
        for elt in &self.0 {
            size += elt.byte_size();
        }
        size
    }
}
//--------------------------------------------------------------------------------
/// An array where the objects are sticked one after the other without
/// a precise count of them. It's ended byt a 0x00 byte and is assumed to
/// occupy the full buffer.
//TODO: when it works implement from []
#[derive(Debug, PartialEq)]
pub struct VecNull<T>(Vec<T>);

impl<T> VecNull<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

impl<T> From<Vec<T>> for VecNull<T> {
    fn from(item: Vec<T>) -> VecNull<T> {
        VecNull(item)
    }
}

impl<T> From<VecNull<T>> for Vec<T> {
    fn from(item: VecNull<T>) -> Vec<T> {
        item.0
    }
}

impl<T> AsRef<Vec<T>> for VecNull<T> {
    fn as_ref(&self) -> &Vec<T> {
        &self.0
    }
}

impl<T> AsMut<Vec<T>> for VecNull<T> {
    fn as_mut(&mut self) -> &mut Vec<T> {
        self.0.as_mut()
    }
}

impl<T> Serialize for VecNull<T>
where
    T: Serialize,
{
    fn serialize(&self, buffer: &mut BytesMut) {
        // data
        for elt in &self.0 {
            elt.serialize(buffer);
        }
        buffer.put_u8(0x00);
    }
}

impl<T> Deserialize for VecNull<T>
where
    T: Deserialize,
{
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        let mut v = Self::new();
        loop {
            if buffer.len() == 1 {
                if let 0 = buffer.try_get_u8()? {
                    return Ok(v);
                } else {
                    return Err(anyhow!("Incorrect terminator in null terminated vec"));
                }
            } else if buffer.len() == 0 {
                return Err(anyhow!("missing null terminator in null terminated vec"));
            } else {
                v.0.push(T::deserialize(buffer)?);
            }
        }
    }
}

impl<T> ByteSized for VecNull<T>
where
    T: ByteSized,
{
    fn byte_size(&self) -> i32 {
        let mut size = 1;
        for elt in &self.0 {
            size += elt.byte_size();
        }
        size
    }
}

//TODO:int array => Intn[k]

#[cfg(test)]
mod test {
    use super::*;

    //----------------------------------------------------------------------------
    #[test]
    fn i8_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        5_i8.serialize(&mut m);
        assert_eq!(vec![5_u8], m.to_vec());

        Ok(())
    }

    #[test]
    fn i8_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[0x08]);
        assert_eq!(8_i8, i8::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn i8_byte_size() -> Result<()> {
        assert_eq!(1, 8_i8.byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn i16_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        5_i16.serialize(&mut m);
        assert_eq!(vec![0_u8, 5_u8], m.to_vec());

        Ok(())
    }

    #[test]
    fn i16_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[0, 8]);
        assert_eq!(8_i16, i16::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn i16_byte_size() -> Result<()> {
        assert_eq!(2, 8_i16.byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn i32_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        5_i32.serialize(&mut m);
        assert_eq!(vec![0_u8, 0_u8, 0_u8, 5_u8], m.to_vec());

        Ok(())
    }

    #[test]
    fn i32_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[0, 0, 0, 8]);
        assert_eq!(8_i32, i32::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn i32_byte_size() -> Result<()> {
        assert_eq!(4, 8i32.byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn byte_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        ('A' as u8).serialize(&mut m);
        assert_eq!(vec!['A' as u8], m.to_vec());

        Ok(())
    }

    #[test]
    fn byte_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&['T' as u8]);
        assert_eq!('T' as u8, Byte::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn byte_byte_size() -> Result<()> {
        assert_eq!(1, (1u8 as Byte).byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn cstring_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        CString::new("aldabis")?.serialize(&mut m);
        assert_eq!(
            vec![
                'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0
            ],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn cstring_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[
            'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0,
        ]);
        assert_eq!(CString::new("aldabis")?, CString::deserialize(&mut buffer)?);

        //FIXME:
        //let buffer: [u8; 0] = [];
        //let cursor = Cursor::new(buffer);
        //let mut buffer_reader = BufReader::new(cursor);
        //assert_eq!(CString::new("")?, CString::deserialize(&mut buffer_reader)?);

        Ok(())
    }

    #[test]
    fn cstring_byte_size() -> Result<()> {
        assert_eq!(8, CString::new("aldabis")?.byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn vec32_i32_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: Vec32<i32> = Vec32::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m);
        assert_eq!(
            vec![
                0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
                0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x05,
            ],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn vec32_byte_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: Vec32<Byte> = Vec32::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m);
        assert_eq!(
            vec![0x00, 0x00, 0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05,],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn vec32_cstring_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: Vec32<CString> =
            Vec32::from(vec![CString::new("aldabis")?, CString::new("aldabis")?]);
        v.serialize(&mut m);
        assert_eq!(
            vec![
                0x00, 0x00, 0x00, 0x02, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8,
                'i' as u8, 's' as u8, 0, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8,
                'i' as u8, 's' as u8, 0,
            ],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn vec32_empty_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: Vec32<CString> = Vec32::new();
        v.serialize(&mut m);
        assert_eq!(vec![0x00, 0x00, 0x00, 0x00,], m.to_vec());

        Ok(())
    }

    #[test]
    fn vec32_i32_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x05,
        ]);
        assert_eq!(
            Vec32::<i32>::from(vec![1, 2, 3, 4, 5]),
            Vec32::<i32>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec32_byte_deserialize() -> Result<()> {
        let mut buffer =
            Bytes::from_static(&[0x00, 0x00, 0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05]);
        assert_eq!(
            Vec32::<Byte>::from(vec![1, 2, 3, 4, 5]),
            Vec32::<Byte>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn vec32_cstring_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x00, 0x00, 0x02, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8,
            'i' as u8, 's' as u8, 0, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8,
            'i' as u8, 's' as u8, 0,
        ]);
        assert_eq!(
            Vec32::<CString>::from(vec![CString::new("aldabis")?, CString::new("aldabis")?]),
            Vec32::<CString>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec32_empty_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[0x00, 0x00, 0x00, 0x00]);
        assert_eq!(
            Vec32::<CString>::new(),
            Vec32::<CString>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec32_i32_byte_size() -> Result<()> {
        assert_eq!(24, Vec32::<i32>::from(vec![1, 2, 3, 4, 5]).byte_size());
        Ok(())
    }

    #[test]
    fn vec32_byte_byte_size() -> Result<()> {
        assert_eq!(9, Vec32::<Byte>::from(vec![1, 2, 3, 4, 5]).byte_size());
        Ok(())
    }

    #[test]
    fn vec32_cstring_byte_size() -> Result<()> {
        assert_eq!(
            20,
            Vec32::<CString>::from(vec![CString::new("aldabis")?, CString::new("aldabis")?])
                .byte_size()
        );
        Ok(())
    }

    #[test]
    fn vec32_empty_byte_size() -> Result<()> {
        assert_eq!(4, Vec32::<CString>::from(vec![]).byte_size());
        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn vec16_i32_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: Vec16<i32> = Vec16::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m);
        assert_eq!(
            vec![
                0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03,
                0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x05,
            ],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn vec16_byte_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: Vec16<Byte> = Vec16::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m);
        assert_eq!(vec![0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05,], m.to_vec());

        Ok(())
    }

    #[test]
    fn vec16_cstring_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: Vec16<CString> =
            Vec16::from(vec![CString::new("aldabis")?, CString::new("aldabis")?]);
        v.serialize(&mut m);
        assert_eq!(
            vec![
                0x00, 0x02, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8,
                's' as u8, 0, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8,
                's' as u8, 0,
            ],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn vec16_empty_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: Vec16<CString> = Vec16::new();
        v.serialize(&mut m);
        assert_eq!(vec![0x00, 0x00,], m.to_vec());

        Ok(())
    }

    #[test]
    fn vec16_i32_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03,
            0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x05,
        ]);
        assert_eq!(
            Vec16::<i32>::from(vec![1, 2, 3, 4, 5]),
            Vec16::<i32>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec16_byte_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05]);
        assert_eq!(
            Vec16::<Byte>::from(vec![1, 2, 3, 4, 5]),
            Vec16::<Byte>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec16_cstring_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x02, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8,
            's' as u8, 0, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8,
            's' as u8, 0,
        ]);
        assert_eq!(
            Vec16::<CString>::from(vec![CString::new("aldabis")?, CString::new("aldabis")?]),
            Vec16::<CString>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec16_empty_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[0x00, 0x00]);
        assert_eq!(
            Vec16::<CString>::new(),
            Vec16::<CString>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec16_i32_byte_size() -> Result<()> {
        assert_eq!(22, Vec16::<i32>::from(vec![1, 2, 3, 4, 5]).byte_size());
        Ok(())
    }

    #[test]
    fn vec16_byte_byte_size() -> Result<()> {
        assert_eq!(7, Vec16::<Byte>::from(vec![1, 2, 3, 4, 5]).byte_size());
        Ok(())
    }

    #[test]
    fn vec16_cstring_byte_size() -> Result<()> {
        assert_eq!(
            18,
            Vec16::<CString>::from(vec![CString::new("aldabis")?, CString::new("aldabis")?])
                .byte_size()
        );
        Ok(())
    }

    #[test]
    fn vec16_empty_byte_size() -> Result<()> {
        assert_eq!(2, Vec16::<CString>::from(vec![]).byte_size());
        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn vecnull_i32_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: VecNull<i32> = VecNull::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m);
        assert_eq!(
            vec![
                0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00,
                0x00, 0x04, 0x00, 0x00, 0x00, 0x05, 0x00
            ],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn vecnull_byte_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: VecNull<Byte> = VecNull::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m);
        assert_eq!(vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x00], m.to_vec());

        Ok(())
    }

    #[test]
    fn vecnull_cstring_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: VecNull<CString> =
            VecNull::from(vec![CString::new("aldabis")?, CString::new("aldabis")?]);
        v.serialize(&mut m);
        assert_eq!(
            vec![
                'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0,
                'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0,
                0x00,
            ],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn vecnull_empty_serialize() -> Result<()> {
        let mut m = BytesMut::new();
        let v: VecNull<CString> = VecNull::new();
        v.serialize(&mut m);
        assert_eq!(vec![0x00,], m.to_vec());

        Ok(())
    }

    #[test]
    fn vecnull_i32_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00,
            0x00, 0x04, 0x00, 0x00, 0x00, 0x05, 0x00,
        ]);
        assert_eq!(
            VecNull::<i32>::from(vec![1, 2, 3, 4, 5]),
            VecNull::<i32>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn vecnull_byte_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x00]);
        assert_eq!(
            VecNull::<Byte>::from(vec![1, 2, 3, 4, 5]),
            VecNull::<Byte>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn vecnull_cstring_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[
            'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0,
            'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0, 0,
        ]);
        assert_eq!(
            VecNull::<CString>::from(vec![CString::new("aldabis")?, CString::new("aldabis")?]),
            VecNull::<CString>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn vecnull_empty_deserialize() -> Result<()> {
        let mut buffer = Bytes::from_static(&[0x00]);
        assert_eq!(
            VecNull::<CString>::new(),
            VecNull::<CString>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vecnull_i32_byte_size() -> Result<()> {
        assert_eq!(21, VecNull::<i32>::from(vec![1, 2, 3, 4, 5]).byte_size());
        Ok(())
    }

    #[test]
    fn vecnull_byte_byte_size() -> Result<()> {
        assert_eq!(6, VecNull::<Byte>::from(vec![1, 2, 3, 4, 5]).byte_size());
        Ok(())
    }

    #[test]
    fn vecnull_cstring_byte_size() -> Result<()> {
        assert_eq!(
            17,
            VecNull::<CString>::from(vec![CString::new("aldabis")?, CString::new("aldabis")?])
                .byte_size()
        );
        Ok(())
    }

    #[test]
    fn vecnull_empty_byte_size() -> Result<()> {
        assert_eq!(1, VecNull::<CString>::from(vec![]).byte_size());
        Ok(())
    }
}
