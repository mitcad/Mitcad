// SPDX-License-Identifier: MIT
//! Writer of ASM binary data, for generating our own test files.

/// Writes ASM binary data for tests (a small writer of our own).
pub struct Writer {
    pub data: Vec<u8>,
}

impl Writer {
    pub fn new(entity_count: i32, flags: i32) -> Self {
        let mut data = b"ASM BinaryFile4".to_vec();
        for v in [23100i32, 0, entity_count, flags] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut w = Writer { data };
        w.str("Mitcad test writer");
        w.str("ASM 231.6.3.65535 NT");
        w.str("Mon Mar 30 21:08:33 2026");
        w.dbl(10.0);
        w.dbl(1e-6);
        w.dbl(1e-10);
        w
    }
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.data.push(0x07);
        self.data.push(s.len() as u8);
        self.data.extend_from_slice(s.as_bytes());
        self
    }
    pub fn ident(&mut self, s: &str) -> &mut Self {
        self.data.push(0x0d);
        self.data.push(s.len() as u8);
        self.data.extend_from_slice(s.as_bytes());
        self
    }
    /// Record type name, prefixes separated by '-'.
    pub fn record(&mut self, name: &str) -> &mut Self {
        let parts: Vec<&str> = name.split('-').collect();
        for p in &parts[..parts.len() - 1] {
            self.data.push(0x0e);
            self.data.push(p.len() as u8);
            self.data.extend_from_slice(p.as_bytes());
        }
        self.ident(parts[parts.len() - 1])
    }
    pub fn int(&mut self, v: i32) -> &mut Self {
        self.data.push(0x04);
        self.data.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn en(&mut self, v: i32) -> &mut Self {
        self.data.push(0x15);
        self.data.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn ptr(&mut self, v: i32) -> &mut Self {
        self.data.push(0x0c);
        self.data.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn dbl(&mut self, v: f64) -> &mut Self {
        self.data.push(0x06);
        self.data.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.data.push(if v { 0x0a } else { 0x0b });
        self
    }
    pub fn pos(&mut self, p: [f64; 3]) -> &mut Self {
        self.data.push(0x13);
        for v in p {
            self.data.extend_from_slice(&v.to_le_bytes());
        }
        self
    }
    pub fn vec(&mut self, p: [f64; 3]) -> &mut Self {
        self.data.push(0x14);
        for v in p {
            self.data.extend_from_slice(&v.to_le_bytes());
        }
        self
    }
    pub fn sub_start(&mut self) -> &mut Self {
        self.data.push(0x0f);
        self
    }
    pub fn sub_end(&mut self) -> &mut Self {
        self.data.push(0x10);
        self
    }
    pub fn end(&mut self) -> &mut Self {
        self.data.push(0x11);
        self
    }
    /// Entity header fields: attribute, history id, unknown pointer.
    pub fn head(&mut self) -> &mut Self {
        self.ptr(-1).int(-1).ptr(-1)
    }
    pub fn finish(&mut self) -> Vec<u8> {
        self.data.push(0x0e);
        self.data.push(3);
        self.data.extend_from_slice(b"End");
        self.data.push(0x0e);
        self.data.push(2);
        self.data.extend_from_slice(b"of");
        self.data.push(0x0e);
        self.data.push(3);
        self.data.extend_from_slice(b"ASM");
        self.ident("data");
        std::mem::take(&mut self.data)
    }
}

impl Default for Writer {
    fn default() -> Self {
        Writer::new(1, 2)
    }
}
