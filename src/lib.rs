//! Small, deliberately limited pixel-art prototype shared by the GUI and CLI.
//! It does not execute AI-generated code or implement a sandbox.

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_DIMENSION: u32 = 256;
const MAX_PROJECT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    /// Straight-alpha RGBA pixels, in row-major order.
    pub pixels: Vec<[u8; 4]>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    /// Bottom to top.
    pub layers: Vec<Layer>,
}

impl Project {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(format!("dimensions must be between 1 and {MAX_DIMENSION}"));
        }
        Ok(Self {
            version: FORMAT_VERSION,
            width,
            height,
            layers: vec![Layer { name: "Layer 1".into(), visible: true, pixels: vec![[0; 4]; (width * height) as usize] }],
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != FORMAT_VERSION { return Err("unsupported project version".into()); }
        if self.width == 0 || self.height == 0 || self.width > MAX_DIMENSION || self.height > MAX_DIMENSION {
            return Err("invalid canvas dimensions".into());
        }
        if self.layers.is_empty() || self.layers.len() > 32 { return Err("project must contain 1 to 32 layers".into()); }
        let len = (self.width * self.height) as usize;
        if self.layers.iter().any(|layer| layer.pixels.len() != len || layer.name.len() > 256) {
            return Err("invalid layer data".into());
        }
        Ok(())
    }

    pub fn add_layer(&mut self) -> Result<(), String> {
        self.validate()?;
        if self.layers.len() >= 32 { return Err("layer limit reached".into()); }
        self.layers.push(Layer {
            name: format!("Layer {}", self.layers.len() + 1), visible: true,
            pixels: vec![[0; 4]; (self.width * self.height) as usize],
        });
        Ok(())
    }

    pub fn set_pixel(&mut self, layer: usize, x: u32, y: u32, color: [u8; 4]) -> Result<(), String> {
        self.validate()?;
        if x >= self.width || y >= self.height { return Err("pixel is outside canvas".into()); }
        let index = (y * self.width + x) as usize;
        let target = self.layers.get_mut(layer).ok_or("layer does not exist")?;
        target.pixels[index] = color;
        Ok(())
    }

    pub fn rgba(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut output = vec![0; (self.width * self.height * 4) as usize];
        for layer in self.layers.iter().filter(|layer| layer.visible) {
            for (pixel, dest) in layer.pixels.iter().zip(output.chunks_exact_mut(4)) {
                blend(dest, *pixel);
            }
        }
        Ok(output)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("open project: {e}"))?;
        let mut bytes = Vec::new();
        BufReader::new(file).take(MAX_PROJECT_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("read project: {e}"))?;
        if bytes.len() as u64 > MAX_PROJECT_BYTES { return Err("project exceeds size limit".into()); }
        let project: Project = serde_json::from_slice(&bytes).map_err(|e| format!("parse project: {e}"))?;
        project.validate()?;
        Ok(project)
    }

    /// Create a new file only. This prototype never silently overwrites artwork.
    pub fn save_new(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| format!("serialize project: {e}"))?;
        write_new(path, &bytes)
    }

    pub fn export_png_new(&self, path: &Path) -> Result<(), String> {
        let pixels = self.rgba()?;
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(|e| format!("PNG header: {e}"))?;
            writer.write_image_data(&pixels).map_err(|e| format!("PNG pixels: {e}"))?;
            writer.finish().map_err(|e| format!("PNG finish: {e}"))?;
        }
        write_new(path, &bytes)
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file = fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| format!("create output: {e}"))?;
    let result = (|| { let mut output = BufWriter::new(file); output.write_all(bytes)?; output.flush() })();
    if let Err(error) = result {
        // Remove our incomplete new file, not a pre-existing file.
        let _ = fs::remove_file(path);
        return Err(format!("write output: {error}"));
    }
    Ok(())
}

fn blend(dest: &mut [u8], src: [u8; 4]) {
    let sa = src[3] as u32;
    let da = dest[3] as u32;
    let alpha_numerator = sa * 255 + da * (255 - sa);
    if alpha_numerator == 0 { dest.copy_from_slice(&[0; 4]); return; }
    for channel in 0..3 {
        let numerator = src[channel] as u32 * sa * 255 + dest[channel] as u32 * da * (255 - sa);
        dest[channel] = ((numerator + alpha_numerator / 2) / alpha_numerator) as u8;
    }
    dest[3] = ((alpha_numerator + 127) / 255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_and_invalid_data_are_rejected() {
        assert!(Project::new(0, 5).is_err());
        assert!(Project::new(257, 5).is_err());
        let mut p = Project::new(2, 2).unwrap();
        p.layers[0].pixels.pop();
        assert!(p.validate().is_err());
    }
    #[test]
    fn layers_composite_and_visibility_is_respected() {
        let mut p = Project::new(1, 1).unwrap();
        p.set_pixel(0, 0, 0, [255, 0, 0, 255]).unwrap();
        p.add_layer().unwrap();
        p.set_pixel(1, 0, 0, [0, 0, 255, 128]).unwrap();
        assert_eq!(p.rgba().unwrap(), vec![127, 0, 128, 255]);
        p.layers[1].visible = false;
        assert_eq!(p.rgba().unwrap(), vec![255, 0, 0, 255]);
    }
    #[test]
    fn project_round_trip_and_png_are_valid() {
        let mut p = Project::new(2, 2).unwrap();
        p.set_pixel(0, 1, 1, [1, 2, 3, 128]).unwrap();
        let json = serde_json::to_vec(&p).unwrap();
        assert_eq!(serde_json::from_slice::<Project>(&json).unwrap(), p);
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, p.width, p.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&p.rgba().unwrap()).unwrap();
        }
        let decoder = png::Decoder::new(bytes.as_slice());
        let mut reader = decoder.read_info().unwrap();
        let mut decoded = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut decoded).unwrap();
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(&decoded[..info.buffer_size()], p.rgba().unwrap());
    }
}
