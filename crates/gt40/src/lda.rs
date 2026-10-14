//! DEC absolute loader (paper tape) format: blocks of
//! `001 000 len_lo len_hi addr_lo addr_hi data... checksum`, where `len`
//! counts the six header bytes plus data and the bytes of a block sum to
//! zero modulo 256. A block with no data carries the start address.

/// A loaded image: the bytes and where they go, plus the start address.
#[derive(Clone, Debug, Default)]
pub struct Image {
    pub bytes: Vec<(u16, u8)>,
    pub start: Option<u16>,
}

#[derive(Debug)]
pub enum LdaError {
    BadHeader(usize),
    Checksum(usize),
    Truncated,
}

pub fn parse(data: &[u8]) -> Result<Image, LdaError> {
    let mut img = Image::default();
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0 {
            i += 1;
            continue;
        }
        if data[i] != 1 || data.get(i + 1) != Some(&0) {
            return Err(LdaError::BadHeader(i));
        }
        let hdr = data.get(i..i + 6).ok_or(LdaError::Truncated)?;
        let len = u16::from_le_bytes([hdr[2], hdr[3]]) as usize;
        let addr = u16::from_le_bytes([hdr[4], hdr[5]]);
        let block = data.get(i..i + len + 1).ok_or(LdaError::Truncated)?;
        if block.iter().fold(0u8, |a, &b| a.wrapping_add(b)) != 0 {
            return Err(LdaError::Checksum(i));
        }
        if len == 6 {
            img.start = Some(addr);
            break;
        }
        for (k, &b) in block[6..len].iter().enumerate() {
            img.bytes.push((addr.wrapping_add(k as u16), b));
        }
        i += len + 1;
    }
    Ok(img)
}

impl Image {
    /// Copy the image into word-addressed memory.
    pub fn load_into(&self, words: &mut [u16]) {
        for &(a, b) in &self.bytes {
            let w = &mut words[(a >> 1) as usize];
            if a & 1 == 0 {
                *w = (*w & 0xFF00) | b as u16;
            } else {
                *w = (*w & 0x00FF) | ((b as u16) << 8);
            }
        }
    }
}
