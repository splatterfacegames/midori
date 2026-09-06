use super::common::{MIN_SCREENSHOT_LUMINANCE_RANGE, MIN_SCREENSHOT_SIZE, Verifier, file_nonempty};
use flate2::read::ZlibDecoder;
use std::fs;
use std::io::Read;
use std::path::Path;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug)]
pub struct PngError(String);

impl std::fmt::Display for PngError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PngError {}

fn error(message: impl Into<String>) -> PngError {
    PngError(message.into())
}

#[derive(Clone, Debug)]
struct PngHeader {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    compression: u8,
    filter_method: u8,
    interlace: u8,
}

fn read_u32_be(value: &[u8]) -> u32 {
    u32::from_be_bytes([value[0], value[1], value[2], value[3]])
}

pub fn png_dimensions(path: &Path) -> Option<(u32, u32)> {
    let data = fs::read(path).ok()?;
    if data.len() < 24 || &data[..8] != PNG_SIGNATURE || &data[12..16] != b"IHDR" {
        return None;
    }
    Some((read_u32_be(&data[16..20]), read_u32_be(&data[20..24])))
}

fn read_png_chunks(path: &Path) -> Result<(PngHeader, Vec<u8>, Option<Vec<[u8; 3]>>), PngError> {
    let data = fs::read(path).map_err(|cause| error(format!("could not read PNG: {cause}")))?;
    if data.len() < 24 || &data[..8] != PNG_SIGNATURE {
        return Err(error("missing PNG signature"));
    }

    let mut offset = 8usize;
    let mut header = None;
    let mut idat = Vec::new();
    let mut palette = None;
    while offset.checked_add(8).is_some_and(|end| end <= data.len()) {
        let length = usize::try_from(read_u32_be(&data[offset..offset + 4]))
            .map_err(|_| error("PNG chunk length overflows usize"))?;
        let kind = &data[offset + 4..offset + 8];
        let chunk_start = offset + 8;
        let chunk_end = chunk_start
            .checked_add(length)
            .ok_or_else(|| error("PNG chunk length overflows"))?;
        let crc_end = chunk_end
            .checked_add(4)
            .ok_or_else(|| error("PNG chunk length overflows"))?;
        if crc_end > data.len() {
            return Err(error("truncated PNG chunk"));
        }
        let chunk = &data[chunk_start..chunk_end];
        match kind {
            b"IHDR" => {
                if length != 13 {
                    return Err(error("invalid IHDR length"));
                }
                header = Some(PngHeader {
                    width: read_u32_be(&chunk[0..4]),
                    height: read_u32_be(&chunk[4..8]),
                    bit_depth: chunk[8],
                    color_type: chunk[9],
                    compression: chunk[10],
                    filter_method: chunk[11],
                    interlace: chunk[12],
                });
            }
            b"PLTE" => {
                if length % 3 != 0 {
                    return Err(error("invalid palette length"));
                }
                let mut values = Vec::with_capacity(length / 3);
                for rgb in chunk.chunks_exact(3) {
                    values.push([rgb[0], rgb[1], rgb[2]]);
                }
                palette = Some(values);
            }
            b"IDAT" => idat.extend_from_slice(chunk),
            b"IEND" => break,
            _ => {}
        }
        offset = crc_end;
    }
    let header = header.ok_or_else(|| error("missing IHDR chunk"))?;
    if idat.is_empty() {
        return Err(error("missing IDAT data"));
    }
    Ok((header, idat, palette))
}

fn paeth_predictor(left: u8, up: u8, up_left: u8) -> u8 {
    let estimate = i32::from(left) + i32::from(up) - i32::from(up_left);
    let distance_left = (estimate - i32::from(left)).abs();
    let distance_up = (estimate - i32::from(up)).abs();
    let distance_up_left = (estimate - i32::from(up_left)).abs();
    if distance_left <= distance_up && distance_left <= distance_up_left {
        left
    } else if distance_up <= distance_up_left {
        up
    } else {
        up_left
    }
}

fn unfilter_png_row(
    filter_type: u8,
    row: &mut [u8],
    previous: &[u8],
    bytes_per_pixel: usize,
) -> Result<(), PngError> {
    if filter_type == 0 {
        return Ok(());
    }
    if !(1..=4).contains(&filter_type) {
        return Err(error(format!("unsupported PNG filter type {filter_type}")));
    }
    for index in 0..row.len() {
        let left = if index >= bytes_per_pixel {
            row[index - bytes_per_pixel]
        } else {
            0
        };
        let up = previous.get(index).copied().unwrap_or(0);
        let up_left = if index >= bytes_per_pixel {
            previous.get(index - bytes_per_pixel).copied().unwrap_or(0)
        } else {
            0
        };
        let predictor = match filter_type {
            1 => left,
            2 => up,
            3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
            4 => paeth_predictor(left, up, up_left),
            _ => unreachable!(),
        };
        row[index] = row[index].wrapping_add(predictor);
    }
    Ok(())
}

pub fn png_luminance_stats(path: &Path) -> Result<(u32, u32, u64), PngError> {
    let (header, idat, palette) = read_png_chunks(path)?;
    if header.compression != 0 || header.filter_method != 0 {
        return Err(error("unsupported PNG compression or filter method"));
    }
    if header.interlace != 0 {
        return Err(error("interlaced PNG screenshots are not supported"));
    }
    if header.bit_depth != 8 {
        return Err(error("only 8-bit PNG screenshots are supported"));
    }
    let channels = match header.color_type {
        0 => 1usize,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        value => return Err(error(format!("unsupported PNG color type {value}"))),
    };
    if header.color_type == 3 && palette.is_none() {
        return Err(error("indexed PNG is missing a palette"));
    }
    let width = usize::try_from(header.width).map_err(|_| error("PNG width overflows usize"))?;
    let height = usize::try_from(header.height).map_err(|_| error("PNG height overflows usize"))?;
    let row_size = width
        .checked_mul(channels)
        .ok_or_else(|| error("PNG row size overflows"))?;
    let mut decoder = ZlibDecoder::new(idat.as_slice());
    let mut raw = Vec::new();
    decoder
        .read_to_end(&mut raw)
        .map_err(|cause| error(format!("invalid PNG image data: {cause}")))?;
    let expected_size = row_size
        .checked_add(1)
        .and_then(|size| size.checked_mul(height))
        .ok_or_else(|| error("PNG image size overflows"))?;
    if raw.len() < expected_size {
        return Err(error(format!(
            "truncated PNG image data: {} bytes, expected {expected_size}",
            raw.len()
        )));
    }

    let step_x = std::cmp::max(1, width / 64);
    let step_y = std::cmp::max(1, height / 64);
    let mut luminance_min = 255u32;
    let mut luminance_max = 0u32;
    let mut sample_count = 0u64;
    let mut previous = vec![0u8; row_size];
    for y in 0..height {
        let offset = y * (row_size + 1);
        let filter_type = raw[offset];
        let mut row = raw[offset + 1..offset + 1 + row_size].to_vec();
        unfilter_png_row(filter_type, &mut row, &previous, channels)?;
        previous = row.clone();
        if y % step_y != 0 {
            continue;
        }
        for x in (0..width).step_by(step_x) {
            let index = x * channels;
            let luminance = match header.color_type {
                0 | 4 => u32::from(row[index]),
                3 => {
                    let palette_index = usize::from(row[index]);
                    let palette = palette.as_ref().expect("indexed palette checked above");
                    let rgb = palette
                        .get(palette_index)
                        .ok_or_else(|| error("indexed PNG references a missing palette color"))?;
                    (u32::from(rgb[0]) + u32::from(rgb[1]) + u32::from(rgb[2])) / 3
                }
                _ => {
                    (u32::from(row[index]) + u32::from(row[index + 1]) + u32::from(row[index + 2]))
                        / 3
                }
            };
            luminance_min = luminance_min.min(luminance);
            luminance_max = luminance_max.max(luminance);
            sample_count += 1;
        }
    }
    Ok((luminance_min, luminance_max, sample_count))
}

pub fn check_png_artifact(verifier: &mut Verifier, name: &str, path: &Path) {
    if !file_nonempty(path) {
        match fs::metadata(path) {
            Ok(metadata) if !metadata.is_file() => {
                verifier.missing(name, format!("{path:?} is missing or empty"))
            }
            _ => verifier.missing(name, format!("{path:?} is missing or empty")),
        }
        return;
    }
    let Some((width, height)) = png_dimensions(path) else {
        verifier.fail(name, format!("{path:?} is not a valid PNG"));
        return;
    };
    if width < MIN_SCREENSHOT_SIZE || height < MIN_SCREENSHOT_SIZE {
        verifier.fail(
            name,
            format!(
                "{path:?} is {width}x{height}, below {MIN_SCREENSHOT_SIZE}x{MIN_SCREENSHOT_SIZE}"
            ),
        );
        return;
    }
    let (luminance_min, luminance_max, sample_count) = match png_luminance_stats(path) {
        Ok(value) => value,
        Err(error) => {
            verifier.fail(
                name,
                format!("{path:?} cannot be inspected for visual content: {error}"),
            );
            return;
        }
    };
    let luminance_range = luminance_max.saturating_sub(luminance_min);
    if sample_count < 2 || luminance_range < MIN_SCREENSHOT_LUMINANCE_RANGE {
        verifier.fail(
            name,
            format!(
                "{path:?} appears blank or placeholder-like: luminance range {luminance_range}, required at least {MIN_SCREENSHOT_LUMINANCE_RANGE}"
            ),
        );
        return;
    }
    verifier.pass(
        name,
        format!("{path:?} is a valid {width}x{height} PNG with luminance range {luminance_range}"),
    );
}
