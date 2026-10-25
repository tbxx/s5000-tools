//! Named fields at fixed offsets within a chunk.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    U8,
    /// Two's-complement byte.
    I8,
    U16Le,
    I16Le,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub offset: usize,
    pub kind: Kind,
}

impl Kind {
    pub fn width(self) -> usize {
        match self {
            Kind::U8 | Kind::I8 => 1,
            Kind::U16Le | Kind::I16Le => 2,
        }
    }

    pub fn range(self) -> (i32, i32) {
        match self {
            Kind::U8 => (0, 255),
            Kind::I8 => (-128, 127),
            Kind::U16Le => (0, 65535),
            Kind::I16Le => (-32768, 32767),
        }
    }
}

pub(crate) const fn f(name: &'static str, offset: usize, kind: Kind) -> Field {
    Field { name, offset, kind }
}

/// Reads a field, or `None` if the chunk is too short to contain it (older
/// file versions have shorter chunks).
pub fn read(data: &[u8], field: &Field) -> Option<i32> {
    let bytes = data.get(field.offset..field.offset + field.kind.width())?;
    Some(match field.kind {
        Kind::U8 => bytes[0] as i32,
        Kind::I8 => bytes[0] as i8 as i32,
        Kind::U16Le => u16::from_le_bytes([bytes[0], bytes[1]]) as i32,
        Kind::I16Le => i16::from_le_bytes([bytes[0], bytes[1]]) as i32,
    })
}

pub fn write(data: &mut [u8], field: &Field, value: i32) -> Result<(), crate::Error> {
    let (lo, hi) = field.kind.range();
    if value < lo || value > hi {
        return Err(crate::Error::OutOfRange {
            field: field.name,
            value,
        });
    }
    let bytes = data
        .get_mut(field.offset..field.offset + field.kind.width())
        .ok_or(crate::Error::FieldAbsent(field.name))?;
    match field.kind {
        Kind::U8 | Kind::I8 => bytes[0] = value as u8,
        Kind::U16Le | Kind::I16Le => bytes.copy_from_slice(&(value as u16).to_le_bytes()),
    }
    Ok(())
}

pub fn lookup(table: &'static [Field], name: &str) -> Option<&'static Field> {
    table.iter().find(|field| field.name == name)
}

/// A name stored as `<length> <bytes...>` in a fixed-size area. Bytes after
/// the length are stale memory and must not be interpreted.
pub(crate) fn read_name(data: &[u8], length_at: usize) -> Option<String> {
    let len = *data.get(length_at)? as usize;
    let bytes = data.get(length_at + 1..length_at + 1 + len)?;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

pub(crate) fn write_name(
    data: &mut [u8],
    length_at: usize,
    capacity: usize,
    name: &str,
) -> Result<(), crate::Error> {
    if !name.is_ascii() || name.len() > capacity {
        return Err(crate::Error::BadName(name.to_owned()));
    }
    let area = data
        .get_mut(length_at..length_at + 1 + capacity)
        .ok_or(crate::Error::FieldAbsent("name"))?;
    area.fill(0);
    area[0] = name.len() as u8;
    area[1..1 + name.len()].copy_from_slice(name.as_bytes());
    Ok(())
}
