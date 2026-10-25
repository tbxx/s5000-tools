//! SysEx value encodings. All data bytes are 7-bit; larger numbers are split
//! into 7-bit groups, most significant first.

/// Splits `value` into `n` 7-bit bytes, most significant first.
pub fn split7(value: u64, n: usize) -> Vec<u8> {
    (0..n).rev().map(|i| ((value >> (7 * i)) & 0x7F) as u8).collect()
}

/// Joins 7-bit bytes, most significant first.
pub fn join7(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |acc, &b| (acc << 7) | (b & 0x7F) as u64)
}

/// 14-bit "compound word".
pub fn u14(value: u16) -> [u8; 2] {
    debug_assert!(value < 1 << 14);
    [((value >> 7) & 0x7F) as u8, (value & 0x7F) as u8]
}

pub fn read_u14(bytes: &[u8]) -> Option<u16> {
    Some(join7(bytes.get(..2)?) as u16)
}

/// 28-bit "compound double word".
pub fn read_u28(bytes: &[u8]) -> Option<u32> {
    Some(join7(bytes.get(..4)?) as u32)
}

/// 56-bit "compound quad word".
pub fn read_u56(bytes: &[u8]) -> Option<u64> {
    Some(join7(bytes.get(..8)?))
}

/// Sign byte (0 = +, 1 = −) followed by a 7-bit magnitude.
pub fn signed7(value: i32) -> [u8; 2] {
    [(value < 0) as u8, (value.unsigned_abs() & 0x7F) as u8]
}

pub fn read_signed7(bytes: &[u8]) -> Option<i32> {
    let (&sign, &mag) = (bytes.first()?, bytes.get(1)?);
    let mag = (mag & 0x7F) as i32;
    Some(if sign != 0 { -mag } else { mag })
}

/// Sign byte followed by a 14-bit magnitude.
pub fn signed14(value: i32) -> [u8; 3] {
    let [hi, lo] = u14((value.unsigned_abs() & 0x3FFF) as u16);
    [(value < 0) as u8, hi, lo]
}

pub fn read_signed14(bytes: &[u8]) -> Option<i32> {
    let sign = *bytes.first()?;
    let mag = read_u14(bytes.get(1..)?)? as i32;
    Some(if sign != 0 { -mag } else { mag })
}

/// Null-terminated ASCII string.
pub fn string(s: &str) -> Vec<u8> {
    let mut out: Vec<u8> = s.bytes().map(|b| b & 0x7F).collect();
    out.push(0);
    out
}

/// Reads one null-terminated string; returns it and the remaining bytes.
pub fn read_string(bytes: &[u8]) -> (String, &[u8]) {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let s = String::from_utf8_lossy(&bytes[..end]).into_owned();
    (s, bytes.get(end + 1..).unwrap_or(&[]))
}

/// Reads consecutive null-terminated strings until the data ends.
pub fn read_strings(mut bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let (s, rest) = read_string(bytes);
        out.push(s);
        bytes = rest;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compound_words_round_trip() {
        assert_eq!(u14(705), [5, 65]); // the spec's own example
        assert_eq!(read_u14(&[5, 65]), Some(705));
        assert_eq!(read_u28(&[0, 2, 0x58, 0x44]), Some(44100));
        assert_eq!(split7(44100, 4), vec![0, 2, 0x58, 0x44]);
        assert_eq!(read_u56(&split7(1 << 40, 8)), Some(1 << 40));
    }

    #[test]
    fn signed_values() {
        assert_eq!(signed7(-4), [1, 4]);
        assert_eq!(read_signed7(&[1, 4]), Some(-4));
        assert_eq!(read_signed7(&[0, 36]), Some(36));
        assert_eq!(signed14(-9999), [1, 0x4E, 0x0F]);
        assert_eq!(read_signed14(&[0, 0x4E, 0x0F]), Some(9999));
    }

    #[test]
    fn strings() {
        assert_eq!(string("Kick"), b"Kick\0");
        assert_eq!(read_strings(b"A\0BC\0"), vec!["A", "BC"]);
        assert_eq!(read_string(b"AB"), ("AB".to_string(), &b""[..]));
    }
}
