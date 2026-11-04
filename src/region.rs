//! Bounded, raw-layer rectangular editing. These snapshots are in-memory data,
//! not the system clipboard, a selection mask, or a new project-file format.

use crate::{Project, MAX_DIMENSION};

/// An owned row-major rectangle of straight-alpha RGBA bytes. Zero-alpha RGB is
/// retained. Public fields are checked before use, including the pixel count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PixelRegion {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl PixelRegion {
    fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 || self.width > MAX_DIMENSION || self.height > MAX_DIMENSION
            || self.pixels.len() != (self.width * self.height) as usize {
            return Err("invalid pixel region".into());
        }
        Ok(())
    }
}

fn check_bounds(project: &Project, x: u32, y: u32, width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0
        || x.checked_add(width).filter(|&end| end <= project.width).is_none()
        || y.checked_add(height).filter(|&end| end <= project.height).is_none() {
        return Err("region must fit entirely inside the canvas".into());
    }
    Ok(())
}

impl Project {
    /// Copy one raw layer, including hidden pixels. The result owns its bytes
    /// and does not change the project. Both dimensions must be positive.
    pub fn copy_region(&self, layer: usize, x: u32, y: u32, width: u32, height: u32) -> Result<PixelRegion, String> {
        self.validate()?;
        check_bounds(self, x, y, width, height)?;
        let source = self.layers.get(layer).ok_or("layer does not exist")?;
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for row in y..y + height {
            let start = (row * self.width + x) as usize;
            pixels.extend_from_slice(&source.pixels[start..start + width as usize]);
        }
        Ok(PixelRegion { width, height, pixels })
    }

    /// Replace destination bytes, including transparent pixels; do not blend,
    /// resize, clip or create a layer. Failure leaves the project unchanged.
    pub fn paste_region(&mut self, layer: usize, x: u32, y: u32, region: &PixelRegion) -> Result<(), String> {
        self.validate()?;
        region.validate()?;
        check_bounds(self, x, y, region.width, region.height)?;
        let target = self.layers.get_mut(layer).ok_or("layer does not exist")?;
        for (row, pixels) in region.pixels.chunks_exact(region.width as usize).enumerate() {
            let start = ((y as usize + row) * self.width as usize) + x as usize;
            target.pixels[start..start + region.width as usize].copy_from_slice(pixels);
        }
        Ok(())
    }

    /// Clear a rectangle to transparent black, including hidden RGB bytes.
    pub fn clear_region(&mut self, layer: usize, x: u32, y: u32, width: u32, height: u32) -> Result<(), String> {
        self.rectangle(layer, x, y, width, height, [0; 4])
    }

    /// Snapshot the source before writing, so same-layer overlaps never smear.
    /// Source is [x, y, width, height]; destination is its new top-left [x, y].
    pub fn copy_region_to(&mut self, layer: usize, source: [u32; 4], to_layer: usize, destination: [u32; 2]) -> Result<(), String> {
        let region = self.copy_region(layer, source[0], source[1], source[2], source[3])?;
        self.paste_region(to_layer, destination[0], destination[1], &region)
    }
}
