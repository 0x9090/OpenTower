use std::fmt;

const BITMAP_CORE_HEADER_SIZE: usize = 12;
const BITMAP_INFO_HEADER_SIZE: usize = 40;
const BI_RGB: u32 = 0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DibImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl DibImage {
    pub fn decode_bmp(bytes: &[u8]) -> Result<Self, DibError> {
        if bytes.get(0..2) != Some(b"BM") {
            return Err(DibError::InvalidBitmapFileHeader);
        }
        let dib_offset = 14;
        Self::decode(bytes.get(dib_offset..).ok_or(DibError::UnexpectedEof)?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DibError> {
        let header_size = read_u32(bytes, 0)? as usize;
        let (
            width,
            signed_height,
            planes,
            bits_per_pixel,
            compression,
            colors_used,
            palette_entry_size,
        ) = match header_size {
            BITMAP_CORE_HEADER_SIZE => (
                u32::from(read_u16(bytes, 4)?),
                i32::from(read_u16(bytes, 6)?),
                read_u16(bytes, 8)?,
                read_u16(bytes, 10)?,
                BI_RGB,
                0,
                3,
            ),
            size if size >= BITMAP_INFO_HEADER_SIZE => (
                read_i32(bytes, 4)?
                    .try_into()
                    .map_err(|_| DibError::InvalidDimensions)?,
                read_i32(bytes, 8)?,
                read_u16(bytes, 12)?,
                read_u16(bytes, 14)?,
                read_u32(bytes, 16)?,
                read_u32(bytes, 32)?,
                4,
            ),
            size => return Err(DibError::UnsupportedHeaderSize(size as u32)),
        };

        if width == 0 || signed_height == 0 {
            return Err(DibError::InvalidDimensions);
        }
        if planes != 1 {
            return Err(DibError::InvalidPlaneCount(planes));
        }
        if compression != BI_RGB {
            return Err(DibError::UnsupportedCompression(compression));
        }
        if !matches!(bits_per_pixel, 1 | 4 | 8 | 24 | 32) {
            return Err(DibError::UnsupportedBitsPerPixel(bits_per_pixel));
        }

        let top_down = signed_height < 0;
        let height = signed_height.unsigned_abs();
        let palette_len = if bits_per_pixel <= 8 {
            if colors_used == 0 {
                1_u32 << bits_per_pixel
            } else {
                colors_used
            }
        } else {
            0
        };
        let palette_offset = header_size;
        let palette_bytes = usize::try_from(palette_len)
            .ok()
            .and_then(|count| count.checked_mul(palette_entry_size))
            .ok_or(DibError::IntegerOverflow)?;
        let pixel_offset = palette_offset
            .checked_add(palette_bytes)
            .ok_or(DibError::IntegerOverflow)?;
        if pixel_offset > bytes.len() {
            return Err(DibError::UnexpectedEof);
        }

        let row_bits = u64::from(width)
            .checked_mul(u64::from(bits_per_pixel))
            .ok_or(DibError::IntegerOverflow)?;
        let row_stride =
            usize::try_from(row_bits.div_ceil(32) * 4).map_err(|_| DibError::IntegerOverflow)?;
        let pixel_bytes = row_stride
            .checked_mul(height as usize)
            .ok_or(DibError::IntegerOverflow)?;
        if pixel_offset
            .checked_add(pixel_bytes)
            .ok_or(DibError::IntegerOverflow)?
            > bytes.len()
        {
            return Err(DibError::UnexpectedEof);
        }

        let output_len = usize::try_from(u64::from(width) * u64::from(height) * 4)
            .map_err(|_| DibError::IntegerOverflow)?;
        let mut rgba = vec![0_u8; output_len];
        for y in 0..height as usize {
            let source_y = if top_down { y } else { height as usize - 1 - y };
            let row_start = pixel_offset + source_y * row_stride;
            let row = &bytes[row_start..row_start + row_stride];
            for x in 0..width as usize {
                let color = match bits_per_pixel {
                    1 => palette_color(
                        bytes,
                        palette_offset,
                        palette_entry_size,
                        usize::from((row[x / 8] >> (7 - x % 8)) & 1),
                    )?,
                    4 => {
                        let packed = row[x / 2];
                        let index = if x % 2 == 0 {
                            packed >> 4
                        } else {
                            packed & 0x0f
                        };
                        palette_color(
                            bytes,
                            palette_offset,
                            palette_entry_size,
                            usize::from(index),
                        )?
                    }
                    8 => palette_color(
                        bytes,
                        palette_offset,
                        palette_entry_size,
                        usize::from(row[x]),
                    )?,
                    24 => {
                        let offset = x * 3;
                        [row[offset + 2], row[offset + 1], row[offset], 255]
                    }
                    32 => {
                        let offset = x * 4;
                        [
                            row[offset + 2],
                            row[offset + 1],
                            row[offset],
                            row[offset + 3],
                        ]
                    }
                    _ => unreachable!(),
                };
                let target = (y * width as usize + x) * 4;
                rgba[target..target + 4].copy_from_slice(&color);
            }
        }

        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// Decode the DIB payload stored by a Windows 3.x `RT_ICON` resource.
    /// Its declared height contains a color bitmap followed by a one-bit AND
    /// transparency mask, so ordinary DIB decoding would incorrectly make the
    /// icon twice as tall.
    pub fn decode_icon(bytes: &[u8]) -> Result<Self, DibError> {
        let header_size = read_u32(bytes, 0)? as usize;
        if header_size < BITMAP_INFO_HEADER_SIZE {
            return Err(DibError::UnsupportedHeaderSize(header_size as u32));
        }
        let width: u32 = read_i32(bytes, 4)?
            .try_into()
            .map_err(|_| DibError::InvalidDimensions)?;
        let stored_height = read_i32(bytes, 8)?;
        if width == 0 || stored_height <= 0 || stored_height % 2 != 0 {
            return Err(DibError::InvalidDimensions);
        }
        let height = (stored_height / 2) as u32;
        let bits_per_pixel = read_u16(bytes, 14)?;
        let colors_used = read_u32(bytes, 32)?;
        let palette_len = if bits_per_pixel <= 8 {
            if colors_used == 0 {
                1_u32 << bits_per_pixel
            } else {
                colors_used
            }
        } else {
            0
        };
        let pixel_offset = header_size
            .checked_add(
                usize::try_from(palette_len)
                    .map_err(|_| DibError::IntegerOverflow)?
                    .checked_mul(4)
                    .ok_or(DibError::IntegerOverflow)?,
            )
            .ok_or(DibError::IntegerOverflow)?;
        let color_stride =
            usize::try_from((u64::from(width) * u64::from(bits_per_pixel)).div_ceil(32) * 4)
                .map_err(|_| DibError::IntegerOverflow)?;
        let mask_offset = pixel_offset
            .checked_add(
                color_stride
                    .checked_mul(height as usize)
                    .ok_or(DibError::IntegerOverflow)?,
            )
            .ok_or(DibError::IntegerOverflow)?;
        let mask_stride = usize::try_from(u64::from(width).div_ceil(32) * 4)
            .map_err(|_| DibError::IntegerOverflow)?;
        let mask_bytes = mask_stride
            .checked_mul(height as usize)
            .ok_or(DibError::IntegerOverflow)?;
        if mask_offset
            .checked_add(mask_bytes)
            .ok_or(DibError::IntegerOverflow)?
            > bytes.len()
        {
            return Err(DibError::UnexpectedEof);
        }

        let mut color_dib = bytes.to_vec();
        color_dib[8..12].copy_from_slice(&(height as i32).to_le_bytes());
        let mut image = Self::decode(&color_dib)?;
        for y in 0..height as usize {
            let source_y = height as usize - 1 - y;
            let row = &bytes
                [mask_offset + source_y * mask_stride..mask_offset + (source_y + 1) * mask_stride];
            for x in 0..width as usize {
                if (row[x / 8] >> (7 - x % 8)) & 1 != 0 {
                    image.rgba[(y * width as usize + x) * 4 + 3] = 0;
                }
            }
        }
        Ok(image)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

fn palette_color(
    bytes: &[u8],
    palette_offset: usize,
    entry_size: usize,
    index: usize,
) -> Result<[u8; 4], DibError> {
    let offset = palette_offset
        .checked_add(
            index
                .checked_mul(entry_size)
                .ok_or(DibError::IntegerOverflow)?,
        )
        .ok_or(DibError::IntegerOverflow)?;
    let entry = bytes
        .get(offset..offset + entry_size)
        .ok_or(DibError::InvalidPaletteIndex(index))?;
    Ok([entry[2], entry[1], entry[0], 255])
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, DibError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(DibError::UnexpectedEof)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, DibError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(DibError::UnexpectedEof)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, DibError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(DibError::UnexpectedEof)?;
    Ok(i32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DibError {
    UnexpectedEof,
    InvalidBitmapFileHeader,
    UnsupportedHeaderSize(u32),
    InvalidDimensions,
    InvalidPlaneCount(u16),
    UnsupportedBitsPerPixel(u16),
    UnsupportedCompression(u32),
    InvalidPaletteIndex(usize),
    IntegerOverflow,
}

impl fmt::Display for DibError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of DIB data"),
            Self::InvalidBitmapFileHeader => f.write_str("file does not have a BMP header"),
            Self::UnsupportedHeaderSize(size) => write!(f, "unsupported DIB header size {size}"),
            Self::InvalidDimensions => f.write_str("DIB dimensions are invalid"),
            Self::InvalidPlaneCount(count) => write!(f, "DIB uses {count} color planes"),
            Self::UnsupportedBitsPerPixel(bits) => {
                write!(f, "unsupported DIB color depth: {bits} bits per pixel")
            }
            Self::UnsupportedCompression(kind) => {
                write!(f, "unsupported DIB compression type {kind}")
            }
            Self::InvalidPaletteIndex(index) => write!(f, "DIB palette index {index} is invalid"),
            Self::IntegerOverflow => f.write_str("integer overflow while decoding DIB"),
        }
    }
}

impl std::error::Error for DibError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_bottom_up_eight_bit_dib() {
        let mut bytes = vec![0_u8; 40 + 8 + 8];
        bytes[0..4].copy_from_slice(&(40_u32).to_le_bytes());
        bytes[4..8].copy_from_slice(&(2_i32).to_le_bytes());
        bytes[8..12].copy_from_slice(&(2_i32).to_le_bytes());
        bytes[12..14].copy_from_slice(&(1_u16).to_le_bytes());
        bytes[14..16].copy_from_slice(&(8_u16).to_le_bytes());
        bytes[32..36].copy_from_slice(&(2_u32).to_le_bytes());
        bytes[40..44].copy_from_slice(&[0, 0, 255, 0]);
        bytes[44..48].copy_from_slice(&[0, 255, 0, 0]);
        bytes[48..52].copy_from_slice(&[1, 0, 0, 0]);
        bytes[52..56].copy_from_slice(&[0, 1, 0, 0]);

        let image = DibImage::decode(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), (2, 2));
        assert_eq!(
            image.rgba(),
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 255, 0, 0, 255,
            ]
        );
    }

    #[test]
    fn decodes_bmp_wrapper() {
        let mut dib = vec![0_u8; 40 + 8];
        dib[0..4].copy_from_slice(&(40_u32).to_le_bytes());
        dib[4..8].copy_from_slice(&(1_i32).to_le_bytes());
        dib[8..12].copy_from_slice(&(1_i32).to_le_bytes());
        dib[12..14].copy_from_slice(&(1_u16).to_le_bytes());
        dib[14..16].copy_from_slice(&(24_u16).to_le_bytes());
        dib[40..43].copy_from_slice(&[3, 2, 1]);
        let mut bmp = vec![0_u8; 14];
        bmp[0..2].copy_from_slice(b"BM");
        bmp.extend_from_slice(&dib);
        assert_eq!(DibImage::decode_bmp(&bmp).unwrap().rgba(), &[1, 2, 3, 255]);
    }

    #[test]
    fn decodes_icon_color_plane_and_transparency_mask() {
        let mut icon = vec![0_u8; 40 + 8 + 4 + 4];
        icon[0..4].copy_from_slice(&(40_u32).to_le_bytes());
        icon[4..8].copy_from_slice(&(2_i32).to_le_bytes());
        icon[8..12].copy_from_slice(&(2_i32).to_le_bytes());
        icon[12..14].copy_from_slice(&(1_u16).to_le_bytes());
        icon[14..16].copy_from_slice(&(1_u16).to_le_bytes());
        icon[32..36].copy_from_slice(&(2_u32).to_le_bytes());
        icon[40..44].copy_from_slice(&[0, 0, 0, 0]);
        icon[44..48].copy_from_slice(&[0, 0, 255, 0]);
        icon[48] = 0b1000_0000;
        icon[52] = 0b0100_0000;

        let image = DibImage::decode_icon(&icon).unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
        assert_eq!(image.rgba(), &[255, 0, 0, 255, 0, 0, 0, 0]);
    }
}
