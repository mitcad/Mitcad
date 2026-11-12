// SPDX-License-Identifier: MIT
//! Scaling a preview down to a thumbnail's size (mitcad#1539): each pixel
//! the average of the pixels it covers, the colours weighted by their
//! alpha, so that a transparent pixel's colour does not bleed into its
//! neighbours.

use crate::png_read::Rgba;

/// `image` scaled down to fit `size` pixels a side, keeping its aspect
/// ratio (at least 1 pixel a side); as it is when it fits already.
pub(crate) fn fit(image: &Rgba, size: u32) -> Rgba {
    let longest = image.width.max(image.height);
    if longest <= size {
        return image.clone();
    }
    let scaled = |side: u32| {
        let side =
            (u64::from(side) * u64::from(size) + u64::from(longest) / 2) / u64::from(longest);
        side.clamp(1, u64::from(size)) as u32
    };
    let (width, height) = (scaled(image.width), scaled(image.height));
    let (from_w, from_h) = (image.width as usize, image.height as usize);
    let (to_w, to_h) = (width as usize, height as usize);
    let mut pixels = Vec::with_capacity(to_w * to_h * 4);
    for y in 0..to_h {
        let y0 = y * from_h / to_h;
        let y1 = ((y + 1) * from_h / to_h).max(y0 + 1);
        for x in 0..to_w {
            let x0 = x * from_w / to_w;
            let x1 = ((x + 1) * from_w / to_w).max(x0 + 1);
            let mut colour = [0u64; 3];
            let mut alpha = 0u64;
            for row in y0..y1 {
                let start = (row * from_w + x0) * 4;
                let end = (row * from_w + x1) * 4;
                for pixel in image.pixels[start..end].as_chunks::<4>().0 {
                    let a = u64::from(pixel[3]);
                    for (sum, &c) in colour.iter_mut().zip(&pixel[..3]) {
                        *sum += u64::from(c) * a;
                    }
                    alpha += a;
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as u64;
            for sum in colour {
                pixels.push((sum + alpha / 2).checked_div(alpha).map_or(0, |v| v as u8));
            }
            pixels.push(((alpha + count / 2) / count) as u8);
        }
    }
    Rgba {
        width,
        height,
        pixels,
    }
}
