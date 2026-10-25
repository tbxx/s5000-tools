//! Level meters (docs/protocol.md §12).

pub const REQUEST: u8 = 0x50;
pub const REPLY_LEN: usize = 32;
/// Reported for silence; levels are dB below full scale.
pub const SILENT: u8 = 0x60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meters {
    /// Record input L, R: dB below full scale (96 = silent).
    pub inputs: [u8; 2],
    /// Outputs 1–16, as above. Outputs without sockets still report routed signal.
    pub outputs: [u8; 16],
}

impl Meters {
    pub fn parse(reply: &[u8]) -> Option<Self> {
        if reply.len() < REPLY_LEN || reply[..3] != [0x50, 0x40, 0x83] {
            return None;
        }
        Some(Self {
            inputs: [reply[12], reply[13]],
            outputs: reply[16..32].try_into().unwrap(),
        })
    }

    /// Level in dBFS, or `None` for silence.
    pub fn db(raw: u8) -> Option<i32> {
        (raw < SILENT).then(|| -(raw as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_levels() {
        let mut r = [0x60u8; 32];
        r[..3].copy_from_slice(&[0x50, 0x40, 0x83]);
        r[12] = 0x02;
        r[16] = 0x0E;
        let m = Meters::parse(&r).unwrap();
        assert_eq!(Meters::db(m.inputs[0]), Some(-2));
        assert_eq!(Meters::db(m.inputs[1]), None);
        assert_eq!(Meters::db(m.outputs[0]), Some(-14));
    }
}
