use anyhow::anyhow;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::ops::{Index, IndexMut};

use crate::{ByteSized, Deserialize, Serialize};

// the list of types can be found here:
// https://www.postgresql.org/docs/17/protocol-message-types.html

//--------------------------------------------------------------------------------
// Implement base types
//--------------------------------------------------------------------------------

impl Serialize for i8 {
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        buffer.put_i8(*self);
        Ok(())
    }
}

impl Deserialize for i8 {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
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
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        buffer.put_i16(*self);
        Ok(())
    }
}

impl Deserialize for i16 {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
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
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        buffer.put_i32(*self);
        Ok(())
    }
}

impl Deserialize for i32 {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
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
impl Serialize for i64 {
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        buffer.put_i64(*self);
        Ok(())
    }
}

impl Deserialize for i64 {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        buffer.try_get_i64().map_err(|e| e.into())
    }
}

impl ByteSized for i64 {
    fn byte_size(&self) -> i32 {
        4
    }
}

//--------------------------------------------------------------------------------
pub type Byte = u8;

impl Serialize for Byte {
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        buffer.put_u8(*self);
        Ok(())
    }
}

impl Deserialize for Byte {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
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
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        buffer.put_slice(self);
        Ok(())
    }
}

impl Deserialize for Byte4 {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
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
// We ensure that the string doesn't contain a null terminated value before
// encoding it. We could use CStrings (we used to) but it's a pain in the code.

impl Serialize for String {
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        if self.as_bytes().iter().any(|&c| c == 0x00) {
            return Err(anyhow!("null terminator in String sent for encoding"));
        }
        buffer.put_slice(self.as_bytes());
        buffer.put_u8(0);
        Ok(())
    }
}

impl Deserialize for String {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut v = Vec::new();
        let mut c: u8 = buffer.try_get_u8()?;

        while c != 0_u8 {
            v.push(c);
            c = buffer.try_get_u8()?;
        }

        Ok(String::from_utf8(v)?)
    }
}

impl ByteSized for String {
    fn byte_size(&self) -> i32 {
        self.bytes().len() as i32 + 1
    }
}

//--------------------------------------------------------------------------------
// Implement different kind of encoding for arrays
//--------------------------------------------------------------------------------

/// An array where the objects are sticked one after the other without
/// a precise count of them. It's ended byt a 0x00 byte and is assumed to
/// occupy the full buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct NullLength;

/// An array where the length is encoded on 16 bit
#[derive(Debug, Clone, PartialEq)]
pub struct Length16;

/// An array where the length is encoded on 32 bit
#[derive(Debug, Clone, PartialEq)]
pub struct Length32;

#[derive(Debug, Clone, PartialEq)]
pub struct VecWithEncoding<T, L> {
    data: Vec<T>,
    length: std::marker::PhantomData<L>,
}

/// Arrays are implemented with a length "field" followed by the data, the
/// length can be encoded as i16 or i32. In some cases like null delimited
/// strings (e.g. in ParameterStatus), there is no size parameter, we continue
/// until we find an additionnal null delimiter (0x00).
impl<T, L> VecWithEncoding<T, L> {
    pub fn new() -> Self {
        Self {
            data: Vec::<T>::new(),
            length: std::marker::PhantomData::<L>,
        }
    }

    /// Returns the number of elements in the vector.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Appends an element to the end of the vector.
    pub fn push(&mut self, elem: T) {
        self.data.push(elem)
    }

    /// Removes the last element from the vector and returns it, or `None` if the vector is empty.
    pub fn pop(&mut self) -> Option<T> {
        self.data.pop()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.data.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.data.iter_mut()
    }
}

// Implement from Vec for VecWithEncoding
impl<T, L> From<Vec<T>> for VecWithEncoding<T, L> {
    fn from(item: Vec<T>) -> VecWithEncoding<T, L> {
        Self {
            data: item,
            length: std::marker::PhantomData::<L>,
        }
    }
}

// Returns a immutable reference to the inner Vec for VecWithEncoding
impl<T, L> AsRef<Vec<T>> for VecWithEncoding<T, L> {
    fn as_ref(&self) -> &Vec<T> {
        &self.data
    }
}

// Returns a mutable reference to the inner Vec for VecWithEncoding
impl<T, L> AsMut<Vec<T>> for VecWithEncoding<T, L> {
    fn as_mut(&mut self) -> &mut Vec<T> {
        self.data.as_mut()
    }
}

// Implement indexation support for VecWithEncoding
impl<T, L> Index<usize> for VecWithEncoding<T, L> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

// Implement indexation support for VecWithEncoding
impl<T, L> IndexMut<usize> for VecWithEncoding<T, L> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.data[index]
    }
}

// Implement the IntoIterator trait for VecWithEncoding to consume the vector
impl<T, L> IntoIterator for VecWithEncoding<T, L> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

// Implement the IntoIterator trait for &VecWithEncoding to iterate over references
impl<'a, T, L> IntoIterator for &'a VecWithEncoding<T, L> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

impl<T> Serialize for VecWithEncoding<T, Length16>
where
    T: Serialize,
{
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        // length
        (self.data.len() as i16).serialize(buffer)?;
        // data
        for elt in &self.data {
            elt.serialize(buffer)?;
        }
        Ok(())
    }
}

impl<T> Serialize for VecWithEncoding<T, Length32>
where
    T: Serialize,
{
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        // length
        (self.data.len() as i32).serialize(buffer)?;
        // data
        for elt in &self.data {
            elt.serialize(buffer)?;
        }
        Ok(())
    }
}

impl<T> Serialize for VecWithEncoding<T, NullLength>
where
    T: Serialize,
{
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        // data
        for elt in &self.data {
            elt.serialize(buffer)?;
        }
        buffer.put_u8(0x00);
        Ok(())
    }
}

impl<T> Deserialize for VecWithEncoding<T, Length16>
where
    T: Deserialize,
{
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut v = Self::new();
        let len = buffer.try_get_i16()?;
        for _ in 0..len {
            v.data.push(T::deserialize(buffer)?);
        }
        Ok(v)
    }
}

impl<T> Deserialize for VecWithEncoding<T, Length32>
where
    T: Deserialize,
{
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut v = Self::new();
        let len = buffer.try_get_i32()?;
        for _ in 0..len {
            v.data.push(T::deserialize(buffer)?);
        }
        Ok(v)
    }
}

impl<T> Deserialize for VecWithEncoding<T, NullLength>
where
    T: Deserialize,
{
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
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
                v.data.push(T::deserialize(buffer)?);
            }
        }
    }
}

impl<T> ByteSized for VecWithEncoding<T, Length16>
where
    T: ByteSized,
{
    fn byte_size(&self) -> i32 {
        let mut size = 2;
        for elt in &self.data {
            size += elt.byte_size();
        }
        size
    }
}

impl<T> ByteSized for VecWithEncoding<T, Length32>
where
    T: ByteSized,
{
    fn byte_size(&self) -> i32 {
        let mut size = 4;
        for elt in &self.data {
            size += elt.byte_size();
        }
        size
    }
}

impl<T> ByteSized for VecWithEncoding<T, NullLength>
where
    T: ByteSized,
{
    fn byte_size(&self) -> i32 {
        let mut size = 1;
        for elt in &self.data {
            size += elt.byte_size();
        }
        size
    }
}

//--------------------------------------------------------------------------------
// Implement empty arrays
//--------------------------------------------------------------------------------
// Some arrays can be empty if the size is -1. For this to work
// the size must be i32.
//TODO:the "API" works and is enought for our need but:
//* there is no trait coercion here
//* we assume the size is a i32

impl<T> Serialize for Option<T>
where
    T: Serialize,
{
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        match self {
            Some(t) => t.serialize(buffer)?,
            None => buffer.put_slice(&[0xFF, 0xFF, 0xFF, 0xFF]),
        };
        Ok(())
    }
}

impl<T> Deserialize for Option<T>
where
    T: Deserialize,
{
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
        Bytes: Buf,
    {
        let mut tbuffer = [0_u8; 4];
        //FIXME: is it safe if buffer is smaller than tbuffer?
        //it should panic from what I understand
        tbuffer.copy_from_slice(&buffer[0..4]);

        match tbuffer {
            [0xFF, 0xFF, 0xFF, 0xFF] => Ok(None),
            _ => Ok(Some(T::deserialize(buffer)?)),
        }
    }
}

impl<T> ByteSized for Option<T>
where
    T: ByteSized,
{
    fn byte_size(&self) -> i32 {
        match self {
            None => 4,
            Some(t) => t.byte_size(),
        }
    }
}

//--------------------------------------------------------------------------------
// Implement BytesArray
//--------------------------------------------------------------------------------
// Bytes are encoded in two parts
// * i32 the length
// * n Bytes where n is the size

impl Serialize for Bytes {
    fn serialize(&self, buffer: &mut BytesMut) -> anyhow::Result<()> {
        (self.len() as i32).serialize(buffer)?;
        buffer.put_slice(&self.slice(0..self.len()));
        Ok(())
    }
}

impl Deserialize for Bytes {
    fn deserialize(buffer: &mut Bytes) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let len = buffer.try_get_i32()?;
        let mut v = vec![0_u8; len as usize];
        //FIXME: Can we do withour the copy?
        buffer.try_copy_to_slice(&mut v)?;

        Ok(Bytes::from(v))
    }
}

impl ByteSized for Bytes {
    fn byte_size(&self) -> i32 {
        (self.len() + 4) as i32
    }
}

//TODO:int array => Intn[k]

//--------------------------------------------------------------------------------
// ... and now, the tests
//--------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;

    //----------------------------------------------------------------------------
    #[test]
    fn i8_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        5_i8.serialize(&mut m)?;
        assert_eq!(vec![5_u8], m.to_vec());

        Ok(())
    }

    #[test]
    fn i8_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[0x08]);
        assert_eq!(8_i8, i8::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn i8_byte_size() -> anyhow::Result<()> {
        assert_eq!(1, 8_i8.byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn i16_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        5_i16.serialize(&mut m)?;
        assert_eq!(vec![0_u8, 5_u8], m.to_vec());

        Ok(())
    }

    #[test]
    fn i16_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[0, 8]);
        assert_eq!(8_i16, i16::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn i16_byte_size() -> anyhow::Result<()> {
        assert_eq!(2, 8_i16.byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn i32_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        5_i32.serialize(&mut m)?;
        assert_eq!(vec![0_u8, 0_u8, 0_u8, 5_u8], m.to_vec());

        Ok(())
    }

    #[test]
    fn i32_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[0, 0, 0, 8]);
        assert_eq!(8_i32, i32::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn i32_byte_size() -> anyhow::Result<()> {
        assert_eq!(4, 8i32.byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn i64_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        5_i64.serialize(&mut m)?;
        assert_eq!(
            vec![0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 5_u8],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn i64_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[0, 0, 0, 0, 0, 0, 0, 8]);
        assert_eq!(8_i64, i64::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn i64_byte_size() -> anyhow::Result<()> {
        assert_eq!(4, 8i64.byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn byte_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        ('A' as u8).serialize(&mut m)?;
        assert_eq!(vec!['A' as u8], m.to_vec());

        Ok(())
    }

    #[test]
    fn byte_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&['T' as u8]);
        assert_eq!('T' as u8, Byte::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn byte_byte_size() -> anyhow::Result<()> {
        assert_eq!(1, (1u8 as Byte).byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn string_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        String::from("aldabis").serialize(&mut m)?;
        assert_eq!(
            vec![
                'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0
            ],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn string_serialize_with_null() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        assert!(
            String::from_utf8(vec![
                'a' as u8, 'l' as u8, 'd' as u8, 0x00, 'b' as u8, 'i' as u8, 's' as u8,
            ])?
            .serialize(&mut m)
            .is_err()
        );

        Ok(())
    }

    #[test]
    fn string_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[
            'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0,
        ]);
        assert_eq!(String::from("aldabis"), String::deserialize(&mut buffer)?);

        Ok(())
    }

    #[test]
    fn string_byte_size() -> anyhow::Result<()> {
        assert_eq!(8, String::from("aldabis").byte_size());

        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn vec32_i32_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<i32, Length32> = VecWithEncoding::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m)?;
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
    fn vec32_byte_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<Byte, Length32> = VecWithEncoding::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m)?;
        assert_eq!(
            vec![0x00, 0x00, 0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05,],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn vec32_cstring_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<String, Length32> =
            VecWithEncoding::from(vec![String::from("aldabis"), String::from("aldabis")]);
        v.serialize(&mut m)?;
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
    fn vec32_empty_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<String, Length32> = VecWithEncoding::new();
        v.serialize(&mut m)?;
        assert_eq!(vec![0x00, 0x00, 0x00, 0x00,], m.to_vec());

        Ok(())
    }

    #[test]
    fn vec32_i32_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x05,
        ]);
        assert_eq!(
            VecWithEncoding::<i32, Length32>::from(vec![1, 2, 3, 4, 5]),
            VecWithEncoding::<i32, Length32>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec32_byte_deserialize() -> anyhow::Result<()> {
        let mut buffer =
            Bytes::from_static(&[0x00, 0x00, 0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05]);
        assert_eq!(
            VecWithEncoding::<Byte, Length32>::from(vec![1, 2, 3, 4, 5]),
            VecWithEncoding::<Byte, Length32>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn vec32_cstring_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x00, 0x00, 0x02, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8,
            'i' as u8, 's' as u8, 0, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8,
            'i' as u8, 's' as u8, 0,
        ]);
        assert_eq!(
            VecWithEncoding::<String, Length32>::from(vec![
                String::from("aldabis"),
                String::from("aldabis")
            ]),
            VecWithEncoding::<String, Length32>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec32_empty_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[0x00, 0x00, 0x00, 0x00]);
        assert_eq!(
            VecWithEncoding::<String, Length32>::new(),
            VecWithEncoding::<String, Length32>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec32_i32_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            24,
            VecWithEncoding::<i32, Length32>::from(vec![1, 2, 3, 4, 5]).byte_size()
        );
        Ok(())
    }

    #[test]
    fn vec32_byte_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            9,
            VecWithEncoding::<Byte, Length32>::from(vec![1, 2, 3, 4, 5]).byte_size()
        );
        Ok(())
    }

    #[test]
    fn vec32_cstring_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            20,
            VecWithEncoding::<String, Length32>::from(vec![
                String::from("aldabis"),
                String::from("aldabis")
            ])
            .byte_size()
        );
        Ok(())
    }

    #[test]
    fn vec32_empty_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            4,
            VecWithEncoding::<String, Length32>::from(vec![]).byte_size()
        );
        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn vec16_i32_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<i32, Length16> = VecWithEncoding::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m)?;
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
    fn vec16_byte_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<Byte, Length16> = VecWithEncoding::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m)?;
        assert_eq!(vec![0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05,], m.to_vec());

        Ok(())
    }

    #[test]
    fn vec16_cstring_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<String, Length16> =
            VecWithEncoding::from(vec![String::from("aldabis"), String::from("aldabis")]);
        v.serialize(&mut m)?;
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
    fn vec16_empty_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<String, Length16> = VecWithEncoding::new();
        v.serialize(&mut m)?;
        assert_eq!(vec![0x00, 0x00,], m.to_vec());

        Ok(())
    }

    #[test]
    fn vec16_i32_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03,
            0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x05,
        ]);
        assert_eq!(
            VecWithEncoding::<i32, Length16>::from(vec![1, 2, 3, 4, 5]),
            VecWithEncoding::<i32, Length16>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec16_byte_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05]);
        assert_eq!(
            VecWithEncoding::<Byte, Length16>::from(vec![1, 2, 3, 4, 5]),
            VecWithEncoding::<Byte, Length16>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec16_cstring_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x02, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8,
            's' as u8, 0, 'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8,
            's' as u8, 0,
        ]);
        assert_eq!(
            VecWithEncoding::<String, Length16>::from(vec![
                String::from("aldabis"),
                String::from("aldabis")
            ]),
            VecWithEncoding::<String, Length16>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec16_empty_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[0x00, 0x00]);
        assert_eq!(
            VecWithEncoding::<String, Length16>::new(),
            VecWithEncoding::<String, Length16>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vec16_i32_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            22,
            VecWithEncoding::<i32, Length16>::from(vec![1, 2, 3, 4, 5]).byte_size()
        );
        Ok(())
    }

    #[test]
    fn vec16_byte_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            7,
            VecWithEncoding::<Byte, Length16>::from(vec![1, 2, 3, 4, 5]).byte_size()
        );
        Ok(())
    }

    #[test]
    fn vec16_cstring_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            18,
            VecWithEncoding::<String, Length16>::from(vec![
                String::from("aldabis"),
                String::from("aldabis")
            ])
            .byte_size()
        );
        Ok(())
    }

    #[test]
    fn vec16_empty_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            2,
            VecWithEncoding::<String, Length16>::from(vec![]).byte_size()
        );
        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn vecnull_i32_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<i32, NullLength> = VecWithEncoding::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m)?;
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
    fn vecnull_byte_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<Byte, NullLength> = VecWithEncoding::from(vec![1, 2, 3, 4, 5]);
        v.serialize(&mut m)?;
        assert_eq!(vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x00], m.to_vec());

        Ok(())
    }

    #[test]
    fn vecnull_cstring_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<String, NullLength> =
            VecWithEncoding::from(vec![String::from("aldabis"), String::from("aldabis")]);
        v.serialize(&mut m)?;
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
    fn vecnull_empty_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v: VecWithEncoding<String, NullLength> = VecWithEncoding::new();
        v.serialize(&mut m)?;
        assert_eq!(vec![0x00,], m.to_vec());

        Ok(())
    }

    #[test]
    fn vecnull_i32_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00,
            0x00, 0x04, 0x00, 0x00, 0x00, 0x05, 0x00,
        ]);
        assert_eq!(
            VecWithEncoding::<i32, NullLength>::from(vec![1, 2, 3, 4, 5]),
            VecWithEncoding::<i32, NullLength>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn vecnull_byte_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x00]);
        assert_eq!(
            VecWithEncoding::<Byte, NullLength>::from(vec![1, 2, 3, 4, 5]),
            VecWithEncoding::<Byte, NullLength>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn vecnull_cstring_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[
            'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0,
            'a' as u8, 'l' as u8, 'd' as u8, 'a' as u8, 'b' as u8, 'i' as u8, 's' as u8, 0, 0,
        ]);
        assert_eq!(
            VecWithEncoding::<String, NullLength>::from(vec![
                String::from("aldabis"),
                String::from("aldabis")
            ]),
            VecWithEncoding::<String, NullLength>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn vecnull_empty_deserialize() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[0x00]);
        assert_eq!(
            VecWithEncoding::<String, NullLength>::new(),
            VecWithEncoding::<String, NullLength>::deserialize(&mut buffer)?
        );

        Ok(())
    }

    #[test]
    fn vecnull_i32_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            21,
            VecWithEncoding::<i32, NullLength>::from(vec![1, 2, 3, 4, 5]).byte_size()
        );
        Ok(())
    }

    #[test]
    fn vecnull_byte_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            6,
            VecWithEncoding::<Byte, NullLength>::from(vec![1, 2, 3, 4, 5]).byte_size()
        );
        Ok(())
    }

    #[test]
    fn vecnull_cstring_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            17,
            VecWithEncoding::<String, NullLength>::from(vec![
                String::from("aldabis"),
                String::from("aldabis")
            ])
            .byte_size()
        );
        Ok(())
    }

    #[test]
    fn vecnull_empty_byte_size() -> anyhow::Result<()> {
        assert_eq!(
            1,
            VecWithEncoding::<String, NullLength>::from(vec![]).byte_size()
        );
        Ok(())
    }
    //----------------------------------------------------------------------------
    #[test]
    fn optionvec32_none_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let o: Option<VecWithEncoding<i32, Length32>> = None;
        o.serialize(&mut m)?;

        assert_eq!(vec![0xFF, 0xFF, 0xFF, 0xFF], m.to_vec());

        Ok(())
    }

    #[test]
    fn optionvec32_some_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let o: Option<VecWithEncoding<i32, Length32>> = Some(VecWithEncoding::from(vec![1_i32]));
        o.serialize(&mut m)?;

        assert_eq!(
            vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01],
            m.to_vec()
        );

        Ok(())
    }

    #[test]
    fn optionvec32_none_deserialize() -> anyhow::Result<()> {
        let o: Option<VecWithEncoding<i32, Length32>> = None;
        let mut buffer = Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]);

        assert_eq!(
            o,
            Option::<VecWithEncoding::<i32, Length32>>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn optionvec32_some_deserialize() -> anyhow::Result<()> {
        let o: Option<VecWithEncoding<i32, Length32>> = Some(VecWithEncoding::from(vec![10_i32]));
        let mut buffer = Bytes::from_static(&[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0A]);

        assert_eq!(
            o,
            Option::<VecWithEncoding::<i32, Length32>>::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn optionvec32_none_byte_size() -> anyhow::Result<()> {
        let o: Option<VecWithEncoding<i32, Length32>> = None;
        assert_eq!(4, o.byte_size());
        Ok(())
    }

    #[test]
    fn optionvec32_some_byte_size() -> anyhow::Result<()> {
        let o: Option<VecWithEncoding<i32, Length32>> = Some(VecWithEncoding::from(vec![0_i32]));
        assert_eq!(8, o.byte_size());
        Ok(())
    }

    //----------------------------------------------------------------------------
    #[test]
    fn bytes_deserialize() -> anyhow::Result<()> {
        let mut buffer =
            Bytes::from_static(&[0x00, 0x00, 0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05]);
        assert_eq!(
            Bytes::from(vec![1, 2, 3, 4, 5]),
            Bytes::deserialize(&mut buffer)?
        );
        Ok(())
    }

    #[test]
    fn bytes_deserialize_with_rest() -> anyhow::Result<()> {
        let mut buffer = Bytes::from_static(&[
            0x00, 0x00, 0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05, 0x00, 0x00,
        ]);
        assert_eq!(
            Bytes::from(vec![1, 2, 3, 4, 5]),
            Bytes::deserialize(&mut buffer)?
        );
        assert_eq!(Bytes::from(vec![0, 0]), buffer);
        Ok(())
    }

    #[test]
    fn bytes_serialize() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v = Bytes::from_static(&[1_u8, 2, 3, 4, 5]);
        v.serialize(&mut m)?;
        assert_eq!(
            vec![0x00, 0x00, 0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05,],
            m.to_vec()
        );
        Ok(())
    }

    #[test]
    fn bytes_bytes_size() -> anyhow::Result<()> {
        let mut m = BytesMut::new();
        let v = Bytes::from_static(&[1_u8, 2, 3, 4, 5]);
        v.serialize(&mut m)?;
        assert_eq!(9, v.byte_size());
        Ok(())
    }
}
