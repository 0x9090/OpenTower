use std::fmt;

const DOS_MAGIC: &[u8; 2] = b"MZ";
const NE_MAGIC: &[u8; 2] = b"NE";

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub enum ResourceIdentifier {
    Ordinal(u16),
    Name(String),
}

impl ResourceIdentifier {
    pub fn as_ordinal(&self) -> Option<u16> {
        match self {
            Self::Ordinal(value) => Some(*value),
            Self::Name(_) => None,
        }
    }

    pub fn standard_type_name(&self) -> Option<&'static str> {
        let Self::Ordinal(value) = self else {
            return None;
        };
        match value {
            1 => Some("CURSOR"),
            2 => Some("BITMAP"),
            3 => Some("ICON"),
            4 => Some("MENU"),
            5 => Some("DIALOG"),
            6 => Some("STRING"),
            7 => Some("FONTDIR"),
            8 => Some("FONT"),
            9 => Some("ACCELERATOR"),
            10 => Some("RCDATA"),
            11 => Some("MESSAGETABLE"),
            12 => Some("GROUP_CURSOR"),
            14 => Some("GROUP_ICON"),
            16 => Some("VERSION"),
            _ => None,
        }
    }
}

impl fmt::Display for ResourceIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ordinal(value) => {
                if let Some(name) = self.standard_type_name() {
                    write!(f, "{name}({value})")
                } else {
                    write!(f, "#{value}")
                }
            }
            Self::Name(value) => f.write_str(value),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resource {
    pub resource_type: ResourceIdentifier,
    pub id: ResourceIdentifier,
    pub file_offset: u64,
    pub length: u64,
    pub flags: u16,
}

#[derive(Clone, Debug)]
pub struct NeFile<'a> {
    bytes: &'a [u8],
    header_offset: usize,
    segment_count: u16,
    resources: Vec<Resource>,
}

impl<'a> NeFile<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, NeError> {
        if bytes.get(0..2) != Some(DOS_MAGIC) {
            return Err(NeError::InvalidDosMagic);
        }
        let header_offset = read_u32(bytes, 0x3c)? as usize;
        if bytes.get(header_offset..header_offset + 2) != Some(NE_MAGIC) {
            return Err(NeError::InvalidNeMagic);
        }

        let segment_count = read_u16(bytes, header_offset + 0x1c)?;
        let resource_table_relative = read_u16(bytes, header_offset + 0x24)? as usize;
        let resident_names_relative = read_u16(bytes, header_offset + 0x26)? as usize;
        let resource_table = header_offset
            .checked_add(resource_table_relative)
            .ok_or(NeError::IntegerOverflow)?;
        let resource_table_end = header_offset
            .checked_add(resident_names_relative)
            .ok_or(NeError::IntegerOverflow)?;
        if resource_table > resource_table_end || resource_table_end > bytes.len() {
            return Err(NeError::InvalidResourceTable);
        }

        let shift = u32::from(read_u16(bytes, resource_table)?);
        if shift > 31 {
            return Err(NeError::InvalidResourceShift(shift));
        }
        let mut cursor = resource_table + 2;
        let mut resources = Vec::new();
        loop {
            let raw_type = read_u16_bounded(bytes, cursor, resource_table_end)?;
            cursor += 2;
            if raw_type == 0 {
                break;
            }
            let count = read_u16_bounded(bytes, cursor, resource_table_end)?;
            cursor += 2;
            cursor = cursor.checked_add(4).ok_or(NeError::IntegerOverflow)?;
            if cursor > resource_table_end {
                return Err(NeError::InvalidResourceTable);
            }
            let resource_type =
                parse_identifier(bytes, resource_table, resource_table_end, raw_type)?;

            for _ in 0..count {
                let raw_offset = read_u16_bounded(bytes, cursor, resource_table_end)?;
                let raw_length = read_u16_bounded(bytes, cursor + 2, resource_table_end)?;
                let flags = read_u16_bounded(bytes, cursor + 4, resource_table_end)?;
                let raw_id = read_u16_bounded(bytes, cursor + 6, resource_table_end)?;
                cursor = cursor.checked_add(12).ok_or(NeError::IntegerOverflow)?;

                let file_offset = u64::from(raw_offset) << shift;
                let length = u64::from(raw_length) << shift;
                let end = file_offset
                    .checked_add(length)
                    .ok_or(NeError::IntegerOverflow)?;
                if end > bytes.len() as u64 {
                    return Err(NeError::ResourceOutsideFile {
                        offset: file_offset,
                        length,
                    });
                }
                resources.push(Resource {
                    resource_type: resource_type.clone(),
                    id: parse_identifier(bytes, resource_table, resource_table_end, raw_id)?,
                    file_offset,
                    length,
                    flags,
                });
            }
        }

        Ok(Self {
            bytes,
            header_offset,
            segment_count,
            resources,
        })
    }

    pub fn header_offset(&self) -> usize {
        self.header_offset
    }

    pub fn segment_count(&self) -> u16 {
        self.segment_count
    }

    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }

    pub fn resource_data(&self, resource: &Resource) -> &'a [u8] {
        let start = resource.file_offset as usize;
        let end = start + resource.length as usize;
        &self.bytes[start..end]
    }
}

fn parse_identifier(
    bytes: &[u8],
    table_start: usize,
    table_end: usize,
    raw: u16,
) -> Result<ResourceIdentifier, NeError> {
    if raw & 0x8000 != 0 {
        return Ok(ResourceIdentifier::Ordinal(raw & 0x7fff));
    }
    let offset = table_start
        .checked_add(usize::from(raw))
        .ok_or(NeError::IntegerOverflow)?;
    let length = usize::from(*bytes.get(offset).ok_or(NeError::InvalidResourceName)?);
    let start = offset + 1;
    let end = start.checked_add(length).ok_or(NeError::IntegerOverflow)?;
    if end > table_end {
        return Err(NeError::InvalidResourceName);
    }
    let name = std::str::from_utf8(&bytes[start..end])
        .map_err(|_| NeError::InvalidResourceName)?
        .to_owned();
    Ok(ResourceIdentifier::Name(name))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, NeError> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or(NeError::UnexpectedEof)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u16_bounded(bytes: &[u8], offset: usize, end: usize) -> Result<u16, NeError> {
    if offset.checked_add(2).ok_or(NeError::IntegerOverflow)? > end {
        return Err(NeError::InvalidResourceTable);
    }
    read_u16(bytes, offset)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, NeError> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or(NeError::UnexpectedEof)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NeError {
    UnexpectedEof,
    InvalidDosMagic,
    InvalidNeMagic,
    InvalidResourceTable,
    InvalidResourceName,
    InvalidResourceShift(u32),
    IntegerOverflow,
    ResourceOutsideFile { offset: u64, length: u64 },
}

impl fmt::Display for NeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of file"),
            Self::InvalidDosMagic => f.write_str("file does not have an MZ header"),
            Self::InvalidNeMagic => f.write_str("file does not have an NE header"),
            Self::InvalidResourceTable => f.write_str("NE resource table is malformed"),
            Self::InvalidResourceName => f.write_str("NE resource name is malformed"),
            Self::InvalidResourceShift(value) => {
                write!(f, "NE resource alignment shift {value} is invalid")
            }
            Self::IntegerOverflow => f.write_str("integer overflow while parsing NE file"),
            Self::ResourceOutsideFile { offset, length } => write!(
                f,
                "resource at offset {offset:#x} with length {length:#x} extends beyond the file"
            ),
        }
    }
}

impl std::error::Error for NeError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ne() -> Vec<u8> {
        let mut bytes = vec![0_u8; 0x300];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&(0x40_u32).to_le_bytes());
        bytes[0x40..0x42].copy_from_slice(b"NE");
        bytes[0x5c..0x5e].copy_from_slice(&(3_u16).to_le_bytes());
        bytes[0x64..0x66].copy_from_slice(&(0x40_u16).to_le_bytes());
        bytes[0x66..0x68].copy_from_slice(&(0x60_u16).to_le_bytes());

        let table = 0x80;
        bytes[table..table + 2].copy_from_slice(&(4_u16).to_le_bytes());
        bytes[table + 2..table + 4].copy_from_slice(&(0x8002_u16).to_le_bytes());
        bytes[table + 4..table + 6].copy_from_slice(&(1_u16).to_le_bytes());
        let name_info = table + 10;
        bytes[name_info..name_info + 2].copy_from_slice(&(0x20_u16).to_le_bytes());
        bytes[name_info + 2..name_info + 4].copy_from_slice(&(0x02_u16).to_le_bytes());
        bytes[name_info + 4..name_info + 6].copy_from_slice(&(0x30_u16).to_le_bytes());
        bytes[name_info + 6..name_info + 8].copy_from_slice(&(0x8007_u16).to_le_bytes());
        bytes[name_info + 12..name_info + 14].copy_from_slice(&(0_u16).to_le_bytes());
        bytes
    }

    #[test]
    fn parses_ordinal_resource_entries() {
        let bytes = sample_ne();
        let ne = NeFile::parse(&bytes).unwrap();
        assert_eq!(ne.segment_count(), 3);
        assert_eq!(ne.resources().len(), 1);
        assert_eq!(
            ne.resources()[0].resource_type,
            ResourceIdentifier::Ordinal(2)
        );
        assert_eq!(ne.resources()[0].id, ResourceIdentifier::Ordinal(7));
        assert_eq!(ne.resources()[0].file_offset, 0x200);
        assert_eq!(ne.resources()[0].length, 0x20);
    }

    #[test]
    fn rejects_non_ne_files() {
        assert_eq!(
            NeFile::parse(&[0; 64]).unwrap_err(),
            NeError::InvalidDosMagic
        );
    }
}
