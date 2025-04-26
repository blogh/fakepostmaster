use bytes::{Buf, BufMut, BytesMut};
use std::io::Write;

pub trait CString {
    fn get_cstring(&mut self) -> String;
    fn put_cstring(&mut self, str: &String);
}

impl CString for BytesMut {
    fn get_cstring(&mut self) -> String {
        let mut s = String::new();
        let mut c: u8 = self.get_u8();

        while c != 0 {
            s.push(c as char);
            c = self.get_u8();
        }

        s
    }

    fn put_cstring(&mut self, str: &String) {
        let bytes = str.as_bytes();

        for c in bytes.iter() {
            self.put_u8(c.clone() as u8);
        }
        self.put_u8(0x00);
    }
}
