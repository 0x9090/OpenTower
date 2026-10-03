use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveInfo {
    pub format: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub data_bytes: u32,
}

pub fn trim_wave(bytes: &[u8]) -> Result<&[u8], WaveError> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(WaveError::InvalidHeader);
    }
    let riff_size = read_u32(bytes, 4)? as usize;
    let end = riff_size.checked_add(8).ok_or(WaveError::IntegerOverflow)?;
    bytes.get(..end).ok_or(WaveError::UnexpectedEof)
}

pub fn inspect_wave(bytes: &[u8]) -> Result<WaveInfo, WaveError> {
    let bytes = trim_wave(bytes)?;
    let mut cursor = 12;
    let mut format = None;
    let mut data_bytes = None;
    while cursor + 8 <= bytes.len() {
        let tag = &bytes[cursor..cursor + 4];
        let length = read_u32(bytes, cursor + 4)? as usize;
        let start = cursor + 8;
        let end = start
            .checked_add(length)
            .ok_or(WaveError::IntegerOverflow)?;
        let chunk = bytes.get(start..end).ok_or(WaveError::UnexpectedEof)?;
        if tag == b"fmt " {
            if chunk.len() < 16 {
                return Err(WaveError::InvalidFormatChunk);
            }
            format = Some((
                read_u16(chunk, 0)?,
                read_u16(chunk, 2)?,
                read_u32(chunk, 4)?,
                read_u16(chunk, 14)?,
            ));
        } else if tag == b"data" {
            data_bytes = Some(length as u32);
        }
        cursor = end + (length & 1);
    }
    let (format, channels, sample_rate, bits_per_sample) =
        format.ok_or(WaveError::MissingFormatChunk)?;
    Ok(WaveInfo {
        format,
        channels,
        sample_rate,
        bits_per_sample,
        data_bytes: data_bytes.ok_or(WaveError::MissingDataChunk)?,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, WaveError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(WaveError::UnexpectedEof)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, WaveError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(WaveError::UnexpectedEof)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaveError {
    UnexpectedEof,
    InvalidHeader,
    InvalidFormatChunk,
    MissingFormatChunk,
    MissingDataChunk,
    IntegerOverflow,
}

impl fmt::Display for WaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of WAVE data"),
            Self::InvalidHeader => f.write_str("resource is not a RIFF/WAVE file"),
            Self::InvalidFormatChunk => f.write_str("WAVE format chunk is malformed"),
            Self::MissingFormatChunk => f.write_str("WAVE format chunk is missing"),
            Self::MissingDataChunk => f.write_str("WAVE data chunk is missing"),
            Self::IntegerOverflow => f.write_str("integer overflow while parsing WAVE data"),
        }
    }
}

impl std::error::Error for WaveError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_resource_padding_and_reads_pcm_format() {
        let mut bytes = Vec::from(&b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x11\x2b\0\0\x11\x2b\0\0\x01\0\x08\0data\0\0\0\0padding"[..]);
        let trimmed = trim_wave(&bytes).unwrap();
        assert_eq!(trimmed.len(), 44);
        assert_eq!(
            inspect_wave(&bytes).unwrap(),
            WaveInfo {
                format: 1,
                channels: 1,
                sample_rate: 11_025,
                bits_per_sample: 8,
                data_bytes: 0,
            }
        );
        bytes[0] = b'X';
        assert_eq!(trim_wave(&bytes), Err(WaveError::InvalidHeader));
    }
}
