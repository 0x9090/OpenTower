use simtower_formats::{DibImage, NeFile, inspect_wave, trim_wave};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

const RT_BITMAP: u16 = 2;
const RT_ICON: u16 = 3;
const SIMTOWER_RAW_BITMAP_TYPE: u16 = 32_514;
const SIMTOWER_PALETTE_TYPE: u16 = 32_515;
const SIMTOWER_BASE_PALETTE_ID: u16 = 1_000;
const SIMTOWER_WAVE_TYPE: u16 = 32_522;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("simtower-inspect: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let path = args
        .next()
        .map(PathBuf::from)
        .ok_or("usage: simtower-inspect <SIMTOWER.EXE> [list | extract <output-directory>]")?;
    let command = args.next().unwrap_or_else(|| "list".into());
    let bytes = fs::read(&path)?;
    let ne = NeFile::parse(&bytes)?;

    match command.to_string_lossy().as_ref() {
        "list" => list(&path, &ne),
        "extract" => {
            let output = args
                .next()
                .map(PathBuf::from)
                .ok_or("extract requires an output directory")?;
            extract(&ne, &output)?;
        }
        other => return Err(format!("unknown command {other:?}").into()),
    }
    Ok(())
}

fn list(path: &Path, ne: &NeFile<'_>) {
    println!("file\t{}", path.display());
    println!("ne_header\t{:#x}", ne.header_offset());
    println!("segments\t{}", ne.segment_count());
    println!("resources\t{}", ne.resources().len());
    println!("type\tid\toffset\tlength\tflags\tdetails");
    for resource in ne.resources() {
        let details = match resource.resource_type.as_ordinal() {
            Some(RT_BITMAP) => DibImage::decode(ne.resource_data(resource))
                .map(|image| format!("{}x{} RGBA", image.width(), image.height()))
                .unwrap_or_else(|error| format!("invalid DIB: {error}")),
            Some(SIMTOWER_WAVE_TYPE) => inspect_wave(ne.resource_data(resource))
                .map(|wave| {
                    format!(
                        "WAVE format={} channels={} rate={} bits={} data={}",
                        wave.format,
                        wave.channels,
                        wave.sample_rate,
                        wave.bits_per_sample,
                        wave.data_bytes
                    )
                })
                .unwrap_or_else(|error| format!("invalid WAVE: {error}")),
            _ => String::new(),
        };
        println!(
            "{}\t{}\t{:#x}\t{}\t{:#06x}\t{}",
            resource.resource_type,
            resource.id,
            resource.file_offset,
            resource.length,
            resource.flags,
            details
        );
    }
}

fn extract(ne: &NeFile<'_>, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bitmap_dir = output.join("bitmaps");
    let icon_dir = output.join("icons");
    let raw_bitmap_dir = output.join("raw-bitmaps");
    let sound_dir = output.join("sounds");
    fs::create_dir_all(&bitmap_dir)?;
    fs::create_dir_all(&icon_dir)?;
    fs::create_dir_all(&raw_bitmap_dir)?;
    fs::create_dir_all(&sound_dir)?;

    let palette_resource = ne
        .resources()
        .iter()
        .find(|resource| {
            resource.resource_type.as_ordinal() == Some(SIMTOWER_PALETTE_TYPE)
                && resource.id.as_ordinal() == Some(SIMTOWER_BASE_PALETTE_ID)
        })
        .ok_or("missing SimTower base palette")?;
    let palette = ne.resource_data(palette_resource);

    let mut bitmap_count = 0;
    let mut icon_count = 0;
    let mut sound_count = 0;
    for resource in ne.resources() {
        let Some(id) = resource.id.as_ordinal() else {
            continue;
        };
        match resource.resource_type.as_ordinal() {
            Some(RT_BITMAP) => {
                let image = DibImage::decode(ne.resource_data(resource))?;
                fs::write(bitmap_dir.join(format!("{id}.bmp")), encode_bmp(&image)?)?;
                bitmap_count += 1;
            }
            Some(RT_ICON) => {
                let image = DibImage::decode_icon(ne.resource_data(resource))?;
                fs::write(icon_dir.join(format!("{id}.bmp")), encode_bmp(&image)?)?;
                icon_count += 1;
            }
            Some(SIMTOWER_RAW_BITMAP_TYPE) => {
                let (width, height, rgba) = decode_raw_bitmap(ne.resource_data(resource), palette)?;
                fs::write(
                    raw_bitmap_dir.join(format!("{id}.bmp")),
                    encode_rgba_bmp(width, height, &rgba)?,
                )?;
                bitmap_count += 1;
            }
            Some(SIMTOWER_WAVE_TYPE) => {
                let wave = trim_wave(ne.resource_data(resource))?;
                fs::write(sound_dir.join(format!("{id}.wav")), wave)?;
                sound_count += 1;
            }
            _ => {}
        }
    }

    println!(
        "extracted {bitmap_count} bitmaps, {icon_count} icons, and {sound_count} sounds to {}",
        output.display()
    );
    Ok(())
}

fn encode_bmp(image: &DibImage) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    encode_rgba_bmp(image.width(), image.height(), image.rgba())
}

fn encode_rgba_bmp(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let pixel_bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("BMP dimensions overflow")?;
    if rgba.len() != pixel_bytes as usize {
        return Err("RGBA byte count does not match dimensions".into());
    }
    let file_size = 54_u32
        .checked_add(pixel_bytes)
        .ok_or("BMP file size overflows")?;
    let mut bmp = Vec::with_capacity(file_size as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&(54_u32).to_le_bytes());
    bmp.extend_from_slice(&(40_u32).to_le_bytes());
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    bmp.extend_from_slice(&(-(height as i32)).to_le_bytes());
    bmp.extend_from_slice(&(1_u16).to_le_bytes());
    bmp.extend_from_slice(&(32_u16).to_le_bytes());
    bmp.extend_from_slice(&(0_u32).to_le_bytes());
    bmp.extend_from_slice(&pixel_bytes.to_le_bytes());
    bmp.extend_from_slice(&(2_835_i32).to_le_bytes());
    bmp.extend_from_slice(&(2_835_i32).to_le_bytes());
    bmp.extend_from_slice(&(0_u32).to_le_bytes());
    bmp.extend_from_slice(&(0_u32).to_le_bytes());
    for pixel in rgba.chunks_exact(4) {
        bmp.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(bmp)
}

fn decode_raw_bitmap(
    bytes: &[u8],
    palette: &[u8],
) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    const CELL_WIDTH: usize = 8;
    const HEIGHT: usize = 36;
    const PALETTE_ENTRY_BYTES: usize = 8;
    if palette.len() < 256 * PALETTE_ENTRY_BYTES {
        return Err("SimTower palette is truncated".into());
    }
    let cell_count = bytes.len() / (CELL_WIDTH * HEIGHT);
    if cell_count == 0 {
        return Err("raw bitmap contains no complete cells".into());
    }
    let width = cell_count * CELL_WIDTH;
    let mut rgba = vec![0_u8; width * HEIGHT * 4];
    for (index, color_index) in bytes.iter().copied().enumerate() {
        let source_x = index % CELL_WIDTH;
        let source_y = index / CELL_WIDTH;
        let destination_x = source_x + (source_y / HEIGHT) * CELL_WIDTH;
        let destination_y = source_y % HEIGHT;
        if destination_x >= width {
            continue;
        }
        let palette_offset = usize::from(color_index) * PALETTE_ENTRY_BYTES;
        let destination = (destination_y * width + destination_x) * 4;
        rgba[destination] = palette[palette_offset + 2];
        rgba[destination + 1] = palette[palette_offset + 4];
        rgba[destination + 2] = palette[palette_offset + 6];
        rgba[destination + 3] = 255;
    }
    Ok((width as u32, HEIGHT as u32, rgba))
}
