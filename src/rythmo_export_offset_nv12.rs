use super::offset_geometry::PixelRect;

pub(crate) fn crop(
    source: &[u8],
    source_width: u32,
    output_width: u32,
    padded_height: u32,
    crop_left: i64,
) -> Vec<u8> {
    let output_y = output_width as usize * padded_height as usize;
    let source_y = source_width as usize * padded_height as usize;
    let mut output = vec![128; output_y * 3 / 2];
    output[..output_y].fill(16);
    if source.len() < source_y * 3 / 2 {
        return output;
    }
    for y in 0..padded_height as usize {
        for x in 0..output_width as i64 {
            let sx = x + crop_left;
            if (0..source_width as i64).contains(&sx) {
                output[y * output_width as usize + x as usize] =
                    source[y * source_width as usize + sx as usize];
            }
        }
    }
    let chroma_left = crop_left.div_euclid(2) * 2;
    for y in 0..(padded_height / 2) as usize {
        for x in (0..output_width as usize).step_by(2) {
            let sx = x as i64 + chroma_left;
            if x + 1 >= output_width as usize || sx < 0 || sx + 1 >= source_width as i64 {
                continue;
            }
            let src = source_y + y * source_width as usize + sx as usize;
            let dst = output_y + y * output_width as usize + x;
            output[dst..dst + 2].copy_from_slice(&source[src..src + 2]);
        }
    }
    output
}

pub(crate) fn copy_rect(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    padded_height: u32,
    rect: PixelRect,
) {
    let rect = rect.clipped(width, height);
    let y_size = width as usize * padded_height as usize;
    for y in rect.top..rect.bottom {
        let start = y as usize * width as usize + rect.left as usize;
        let end = y as usize * width as usize + rect.right as usize;
        destination[start..end].copy_from_slice(&source[start..end]);
    }
    let left = (rect.left.max(0) / 2 * 2) as usize;
    let right = (((rect.right.max(0) + 1) / 2 * 2) as usize).min(width as usize);
    let top = (rect.top.max(0) / 2) as usize;
    let bottom = (((rect.bottom.max(0) + 1) / 2) as usize).min(padded_height as usize / 2);
    for y in top..bottom {
        let start = y_size + y * width as usize + left;
        let end = y_size + y * width as usize + right;
        if end <= source.len() && end <= destination.len() {
            destination[start..end].copy_from_slice(&source[start..end]);
        }
    }
}

pub(crate) fn copy_intersection(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    padded_height: u32,
    a: PixelRect,
    b: PixelRect,
) {
    let rect = PixelRect {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    if rect.right > rect.left && rect.bottom > rect.top {
        copy_rect(source, destination, width, height, padded_height, rect);
    }
}

pub(crate) fn restore_playhead(
    source: &[u8],
    destination: &mut [u8],
    width: u32,
    height: u32,
    padded_height: u32,
    area: PixelRect,
    column: PixelRect,
) {
    let rect = PixelRect {
        left: area.left.max(column.left),
        top: area.top.max(column.top),
        right: area.right.min(column.right),
        bottom: area.bottom.min(column.bottom),
    }
    .clipped(width, height);
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            let index = y as usize * width as usize + x as usize;
            // Solid GPU playhead red converts to Y=95 in the NV12 shader.
            if destination.get(index) == Some(&95) {
                destination[index] = source[index];
            }
        }
    }
    let y_size = width as usize * padded_height as usize;
    let left = (rect.left.max(0) / 2 * 2) as usize;
    let right = (((rect.right.max(0) + 1) / 2 * 2) as usize).min(width as usize);
    let top = (rect.top.max(0) / 2) as usize;
    let bottom = (((rect.bottom.max(0) + 1) / 2) as usize).min(padded_height as usize / 2);
    for y in top..bottom {
        let start = y_size + y * width as usize + left;
        let end = y_size + y * width as usize + right;
        if end <= source.len() && end <= destination.len() {
            destination[start..end].copy_from_slice(&source[start..end]);
        }
    }
}
