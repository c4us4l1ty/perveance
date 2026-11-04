//! Small, deliberately limited pixel-art prototype shared by the GUI and CLI.
//! It does not execute AI-generated code or implement a sandbox.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Write};
use std::path::Path;

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_DIMENSION: u32 = 256;
// Must accommodate the worst valid 256×256×32 RGBA project as JSON, including
// escaped layer names, not merely its much smaller in-memory pixel payload.
const MAX_PROJECT_BYTES: u64 = 40 * 1024 * 1024;
pub const MAX_PNG_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    /// Straight-alpha RGBA pixels, in row-major order.
    pub pixels: Vec<[u8; 4]>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    /// Bottom to top.
    pub layers: Vec<Layer>,
}

/// Undo/redo snapshots for the local editor. Each stack is limited by count and
/// estimated pixel/name payload, not a precise process-memory ceiling.
#[derive(Default)]
pub struct History {
    undo: Vec<HistorySnapshot>,
    redo: Vec<HistorySnapshot>,
}

struct HistorySnapshot {
    project: Project,
    selected_layer: Option<usize>,
}

const HISTORY_MAX_SNAPSHOTS: usize = 20;
const HISTORY_MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;

impl History {
    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }

    /// Call immediately before a successful edit. New edits discard the redo branch.
    pub fn checkpoint(&mut self, project: &Project) { self.checkpoint_inner(project, None); }

    /// Also retain the editor's selected layer so layer reorder undo restores selection.
    pub fn checkpoint_with_selection(&mut self, project: &Project, selected_layer: usize) {
        self.checkpoint_inner(project, Some(selected_layer));
    }

    fn checkpoint_inner(&mut self, project: &Project, selected_layer: Option<usize>) {
        self.redo.clear();
        Self::push_bounded(&mut self.undo, HistorySnapshot { project: project.clone(), selected_layer });
    }

    /// Apply a discrete edit transactionally. No-ops and errors do not discard redo.
    pub fn apply<F>(&mut self, project: &mut Project, edit: F) -> Result<bool, String>
    where F: FnOnce(&mut Project) -> Result<(), String> {
        self.apply_inner(project, None, edit)
    }

    pub fn apply_with_selection<F>(&mut self, project: &mut Project, selected_layer: usize, edit: F) -> Result<bool, String>
    where F: FnOnce(&mut Project) -> Result<(), String> {
        self.apply_inner(project, Some(selected_layer), edit)
    }

    fn apply_inner<F>(&mut self, project: &mut Project, selected_layer: Option<usize>, edit: F) -> Result<bool, String>
    where F: FnOnce(&mut Project) -> Result<(), String> {
        let mut candidate = project.clone();
        edit(&mut candidate)?;
        candidate.validate()?;
        if candidate == *project { return Ok(false); }
        self.checkpoint_inner(project, selected_layer);
        *project = candidate;
        Ok(true)
    }

    pub fn undo(&mut self, project: &mut Project) -> bool {
        Self::restore(&mut self.undo, &mut self.redo, project, None)
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        Self::restore(&mut self.redo, &mut self.undo, project, None)
    }

    pub fn undo_with_selection(&mut self, project: &mut Project, selected_layer: &mut usize) -> bool {
        Self::restore(&mut self.undo, &mut self.redo, project, Some(selected_layer))
    }

    pub fn redo_with_selection(&mut self, project: &mut Project, selected_layer: &mut usize) -> bool {
        Self::restore(&mut self.redo, &mut self.undo, project, Some(selected_layer))
    }

    fn restore(source: &mut Vec<HistorySnapshot>, destination: &mut Vec<HistorySnapshot>, project: &mut Project, selected_layer: Option<&mut usize>) -> bool {
        match source.pop() {
            Some(snapshot) => {
                let current_selection = selected_layer.as_deref().copied();
                let current_project = std::mem::replace(project, snapshot.project);
                Self::push_bounded(destination, HistorySnapshot { project: current_project, selected_layer: current_selection });
                if let (Some(selection), Some(previous)) = (selected_layer, snapshot.selected_layer) {
                    *selection = previous.min(project.layers.len().saturating_sub(1));
                }
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) { self.undo.clear(); self.redo.clear(); }

    fn push_bounded(stack: &mut Vec<HistorySnapshot>, snapshot: HistorySnapshot) {
        stack.push(snapshot);
        while stack.len() > 1 && (stack.len() > HISTORY_MAX_SNAPSHOTS || stack.iter().map(|entry| Self::estimated_payload(&entry.project)).sum::<usize>() > HISTORY_MAX_PAYLOAD_BYTES) {
            stack.remove(0);
        }
    }

    fn estimated_payload(project: &Project) -> usize {
        project.layers.iter().map(|layer| layer.pixels.len().saturating_mul(4).saturating_add(layer.name.len())).sum()
    }
}

/// Axis of reflection across the full layer's canvas bounds.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlipAxis { Horizontal, Vertical }

/// A bounded, typed operation accepted by the headless interface.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    AddLayer,
    DuplicateLayer { layer: usize },
    ClearLayer { layer: usize },
    FlipLayer { layer: usize, axis: FlipAxis },
    SetVisibility { layer: usize, visible: bool },
    RenameLayer { layer: usize, name: String },
    MoveLayer { from: usize, to: usize },
    RemoveLayer { layer: usize },
    Pixel { layer: usize, x: u32, y: u32, color: [u8; 4] },
    Line { layer: usize, from: [u32; 2], to: [u32; 2], color: [u8; 4] },
    Rectangle { layer: usize, x: u32, y: u32, width: u32, height: u32, color: [u8; 4] },
    RectangleOutline { layer: usize, x: u32, y: u32, width: u32, height: u32, color: [u8; 4] },
    Ellipse { layer: usize, x: u32, y: u32, width: u32, height: u32, filled: bool, color: [u8; 4] },
    Fill { layer: usize, x: u32, y: u32, color: [u8; 4] },
}

pub const MAX_OPERATIONS: usize = 1024;

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

    /// Open a static PNG as a new project. Pixel metadata and color profiles are not retained.
    pub fn from_png(path: &Path) -> Result<Self, String> {
        let image = decode_png(path)?;
        let mut project = Self::new(image.width, image.height)?;
        project.layers[0].name = "Imported PNG".into();
        project.layers[0].pixels = image.pixels;
        Ok(project)
    }

    /// Import a same-sized static PNG as a new topmost layer. Failure leaves the project unchanged.
    pub fn import_png_layer(&mut self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if self.layers.len() >= 32 { return Err("layer limit reached".into()); }
        let image = decode_png(path)?;
        if image.width != self.width || image.height != self.height {
            return Err(format!("PNG is {}×{} but the canvas is {}×{}", image.width, image.height, self.width, self.height));
        }
        self.layers.push(Layer { name: "Imported PNG".into(), visible: true, pixels: image.pixels });
        Ok(())
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

    /// Insert an exact pixel/visibility copy immediately above the source layer.
    /// The copy's name is suffixed without breaking the 256-byte UTF-8 limit.
    pub fn duplicate_layer(&mut self, layer: usize) -> Result<usize, String> {
        self.validate()?;
        let source = self.layers.get(layer).ok_or("layer does not exist")?;
        if self.layers.len() >= 32 { return Err("layer limit reached".into()); }
        let mut copy = source.clone();
        const SUFFIX: &str = " copy";
        if copy.name.len() + SUFFIX.len() > 256 {
            let mut end = 256 - SUFFIX.len();
            while !copy.name.is_char_boundary(end) { end -= 1; }
            copy.name.truncate(end);
        }
        copy.name.push_str(SUFFIX);
        self.layers.insert(layer + 1, copy);
        Ok(layer + 1)
    }

    /// Clear all pixels of a layer, including RGB of transparent pixels.
    pub fn clear_layer(&mut self, layer: usize) -> Result<(), String> {
        self.validate()?;
        let target = self.layers.get_mut(layer).ok_or("layer does not exist")?;
        target.pixels.fill([0; 4]);
        Ok(())
    }

    /// Reflect an entire layer in its canvas bounds; preserve exact RGBA bytes.
    pub fn flip_layer(&mut self, layer: usize, axis: FlipAxis) -> Result<(), String> {
        self.validate()?;
        let target = self.layers.get_mut(layer).ok_or("layer does not exist")?;
        let width = self.width as usize;
        let height = self.height as usize;
        match axis {
            FlipAxis::Horizontal => {
                for row in target.pixels.chunks_exact_mut(width) { row.reverse(); }
            }
            FlipAxis::Vertical => {
                for y in 0..height / 2 {
                    for x in 0..width { target.pixels.swap(y * width + x, (height - 1 - y) * width + x); }
                }
            }
        }
        Ok(())
    }

    pub fn rename_layer(&mut self, layer: usize, name: &str) -> Result<(), String> {
        self.validate()?;
        if name.trim().is_empty() || name.len() > 256 { return Err("layer name must be 1 to 256 bytes and not blank".into()); }
        let target = self.layers.get_mut(layer).ok_or("layer does not exist")?;
        target.name = name.to_owned();
        Ok(())
    }

    /// Move a layer to an index in the resulting bottom-to-top ordering.
    pub fn move_layer(&mut self, from: usize, to: usize) -> Result<(), String> {
        self.validate()?;
        if from >= self.layers.len() || to >= self.layers.len() { return Err("layer does not exist".into()); }
        if from != to {
            let layer = self.layers.remove(from);
            self.layers.insert(to, layer);
        }
        Ok(())
    }

    pub fn remove_layer(&mut self, layer: usize) -> Result<(), String> {
        self.validate()?;
        if layer >= self.layers.len() { return Err("layer does not exist".into()); }
        if self.layers.len() == 1 { return Err("cannot remove the last layer".into()); }
        self.layers.remove(layer);
        Ok(())
    }

    /// Fill a rectangle, entirely inside the canvas, with exclusive right/bottom edges.
    pub fn rectangle(&mut self, layer: usize, x: u32, y: u32, width: u32, height: u32, color: [u8; 4]) -> Result<(), String> {
        self.validate()?;
        let right = x.checked_add(width).ok_or("rectangle out of bounds")?;
        let bottom = y.checked_add(height).ok_or("rectangle out of bounds")?;
        if layer >= self.layers.len() || width == 0 || height == 0 || right > self.width || bottom > self.height {
            return Err("rectangle or layer is outside the project".into());
        }
        for row in y..bottom {
            let start = (row * self.width + x) as usize;
            let end = start + width as usize;
            self.layers[layer].pixels[start..end].fill(color);
        }
        Ok(())
    }

    /// Draw a one-pixel rectangle border, including degenerate one-pixel boxes.
    pub fn rectangle_outline(&mut self, layer: usize, x: u32, y: u32, width: u32, height: u32, color: [u8; 4]) -> Result<(), String> {
        self.check_shape_bounds(layer, x, y, width, height)?;
        let pixels = &mut self.layers[layer].pixels;
        let stride = self.width as usize;
        for dy in 0..height {
            for dx in 0..width {
                if dx == 0 || dy == 0 || dx + 1 == width || dy + 1 == height {
                    pixels[(y + dy) as usize * stride + (x + dx) as usize] = color;
                }
            }
        }
        Ok(())
    }

    /// Raster ellipse using pixel centers; outlines are its four-connected edge.
    pub fn ellipse(&mut self, layer: usize, x: u32, y: u32, width: u32, height: u32, filled: bool, color: [u8; 4]) -> Result<(), String> {
        self.check_shape_bounds(layer, x, y, width, height)?;
        let (w, h) = (i64::from(width), i64::from(height));
        let inside = |px: i64, py: i64| -> bool {
            if px < 0 || py < 0 || px >= w || py >= h { return false; }
            let dx = 2 * px + 1 - w;
            let dy = 2 * py + 1 - h;
            dx * dx * h * h + dy * dy * w * w <= w * w * h * h
        };
        let stride = self.width as usize;
        let pixels = &mut self.layers[layer].pixels;
        for dy in 0..height {
            for dx in 0..width {
                let (px, py) = (i64::from(dx), i64::from(dy));
                if inside(px, py) && (filled || !inside(px - 1, py) || !inside(px + 1, py) || !inside(px, py - 1) || !inside(px, py + 1)) {
                    pixels[(y + dy) as usize * stride + (x + dx) as usize] = color;
                }
            }
        }
        Ok(())
    }

    fn check_shape_bounds(&self, layer: usize, x: u32, y: u32, width: u32, height: u32) -> Result<(), String> {
        self.validate()?;
        let right = x.checked_add(width).ok_or("shape out of bounds")?;
        let bottom = y.checked_add(height).ok_or("shape out of bounds")?;
        if layer >= self.layers.len() || width == 0 || height == 0 || right > self.width || bottom > self.height {
            return Err("shape or layer is outside the project".into());
        }
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

    /// Draw an inclusive, one-pixel-wide Bresenham line on a single layer.
    pub fn draw_line(&mut self, layer: usize, from: [u32; 2], to: [u32; 2], color: [u8; 4]) -> Result<(), String> {
        self.validate()?;
        if layer >= self.layers.len() || from[0] >= self.width || from[1] >= self.height || to[0] >= self.width || to[1] >= self.height {
            return Err("line endpoint or layer is outside the project".into());
        }
        let (mut x, mut y) = (from[0] as i32, from[1] as i32);
        let (end_x, end_y) = (to[0] as i32, to[1] as i32);
        let dx = (end_x - x).abs();
        let dy = -(end_y - y).abs();
        let sx = if x < end_x { 1 } else { -1 };
        let sy = if y < end_y { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            self.layers[layer].pixels[(y as u32 * self.width + x as u32) as usize] = color;
            if x == end_x && y == end_y { break; }
            let twice = 2 * error;
            if twice >= dy { error += dy; x += sx; }
            if twice <= dx { error += dx; y += sy; }
        }
        Ok(())
    }

    /// Fill a four-connected region of identical RGBA pixels in the selected layer.
    pub fn fill(&mut self, layer: usize, x: u32, y: u32, color: [u8; 4]) -> Result<(), String> {
        self.validate()?;
        if layer >= self.layers.len() || x >= self.width || y >= self.height {
            return Err("fill point or layer is outside the project".into());
        }
        let width = self.width as usize;
        let height = self.height as usize;
        let pixels = &mut self.layers[layer].pixels;
        let index = y as usize * width + x as usize;
        let original = pixels[index];
        if original == color { return Ok(()); }
        pixels[index] = color;
        let mut pending = vec![index];
        while let Some(i) = pending.pop() {
            let px = i % width;
            let py = i / width;
            let mut enqueue = |next: usize| {
                if pixels[next] == original { pixels[next] = color; pending.push(next); }
            };
            if px > 0 { enqueue(i - 1); }
            if px + 1 < width { enqueue(i + 1); }
            if py > 0 { enqueue(i - width); }
            if py + 1 < height { enqueue(i + width); }
        }
        Ok(())
    }

    /// All-or-nothing in memory: a failing operation leaves the original unchanged.
    pub fn apply_operations(&mut self, operations: &[Operation]) -> Result<(), String> {
        self.validate()?;
        if operations.len() > MAX_OPERATIONS { return Err("too many operations".into()); }
        let mut candidate = self.clone();
        for (index, operation) in operations.iter().enumerate() {
            let result = match operation {
                Operation::AddLayer => candidate.add_layer(),
                Operation::DuplicateLayer { layer } => candidate.duplicate_layer(*layer).map(|_| ()),
                Operation::ClearLayer { layer } => candidate.clear_layer(*layer),
                Operation::FlipLayer { layer, axis } => candidate.flip_layer(*layer, *axis),
                Operation::SetVisibility { layer, visible } => match candidate.layers.get_mut(*layer) {
                    Some(item) => { item.visible = *visible; Ok(()) },
                    None => Err("layer does not exist".into()),
                },
                Operation::RenameLayer { layer, name } => candidate.rename_layer(*layer, name),
                Operation::MoveLayer { from, to } => candidate.move_layer(*from, *to),
                Operation::RemoveLayer { layer } => candidate.remove_layer(*layer),
                Operation::Pixel { layer, x, y, color } => candidate.set_pixel(*layer, *x, *y, *color),
                Operation::Line { layer, from, to, color } => candidate.draw_line(*layer, *from, *to, *color),
                Operation::Rectangle { layer, x, y, width, height, color } => candidate.rectangle(*layer, *x, *y, *width, *height, *color),
                Operation::RectangleOutline { layer, x, y, width, height, color } => candidate.rectangle_outline(*layer, *x, *y, *width, *height, *color),
                Operation::Ellipse { layer, x, y, width, height, filled, color } => candidate.ellipse(*layer, *x, *y, *width, *height, *filled, *color),
                Operation::Fill { layer, x, y, color } => candidate.fill(*layer, *x, *y, *color),
            };
            result.map_err(|error| format!("operation {index}: {error}"))?;
        }
        *self = candidate;
        Ok(())
    }

    /// Read a raw layer pixel, even if its layer is hidden, preserving all RGBA bytes.
    pub fn sample_layer(&self, layer: usize, x: u32, y: u32) -> Result<[u8; 4], String> {
        self.validate()?;
        if x >= self.width || y >= self.height { return Err("sample point is outside canvas".into()); }
        let layer = self.layers.get(layer).ok_or("layer does not exist")?;
        Ok(layer.pixels[(y * self.width + x) as usize])
    }

    /// Sample the visible composite exactly as exported to PNG, without the checkerboard.
    pub fn sample_composite(&self, x: u32, y: u32) -> Result<[u8; 4], String> {
        self.validate()?;
        if x >= self.width || y >= self.height { return Err("sample point is outside canvas".into()); }
        let index = (y * self.width + x) as usize;
        let mut output = [0; 4];
        for layer in self.layers.iter().filter(|layer| layer.visible) {
            blend(&mut output, layer.pixels[index]);
        }
        Ok(output)
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
        if bytes.len() as u64 > MAX_PROJECT_BYTES { return Err("project exceeds size limit".into()); }
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

struct DecodedPng {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 4]>,
}

/// Decode only a bounded single-frame PNG to straight-alpha RGBA8.
fn decode_png(path: &Path) -> Result<DecodedPng, String> {
    let file = File::open(path).map_err(|e| format!("open PNG: {e}"))?;
    let mut bytes = Vec::new();
    BufReader::new(file).take(MAX_PNG_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("read PNG: {e}"))?;
    if bytes.len() as u64 > MAX_PNG_BYTES { return Err("PNG exceeds input size limit".into()); }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits { bytes: 8 * 1024 * 1024 });
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("invalid PNG header: {e}"))?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(format!("PNG dimensions must be between 1 and {MAX_DIMENSION}"));
    }
    if info.animation_control.is_some() { return Err("animated PNG is not supported".into()); }
    let max_output = (width as usize) * (height as usize) * 4;
    if reader.output_buffer_size() > max_output { return Err("PNG decoded output is too large".into()); }
    let mut raw = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut raw).map_err(|e| format!("decode PNG: {e}"))?;
    if frame.width != width || frame.height != height || frame.bit_depth != png::BitDepth::Eight {
        return Err("unsupported PNG frame or bit depth".into());
    }
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("could not expand indexed PNG".into()),
    };
    let pixel_count = width as usize * height as usize;
    if frame.buffer_size() != pixel_count * channels { return Err("invalid PNG pixel buffer".into()); }
    let pixels = raw[..frame.buffer_size()].chunks_exact(channels).map(|p| match channels {
        1 => [p[0], p[0], p[0], 255],
        2 => [p[0], p[0], p[0], p[1]],
        3 => [p[0], p[1], p[2], 255],
        4 => [p[0], p[1], p[2], p[3]],
        _ => unreachable!(),
    }).collect();
    reader.finish().map_err(|e| format!("finish PNG: {e}"))?;
    if reader.info().animation_control.is_some() { return Err("animated PNG is not supported".into()); }
    Ok(DecodedPng { width, height, pixels })
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| format!("create temporary output: {e}"))?;
    staged.write_all(bytes).map_err(|e| format!("write temporary output: {e}"))?;
    staged.as_file_mut().sync_all().map_err(|e| format!("sync temporary output: {e}"))?;
    // Never replace an existing file. The tempfile is created in the target's
    // directory so the library can use a same-filesystem no-clobber installation.
    // Filesystem/platform guarantees vary; directory fsync is not performed.
    staged.persist_noclobber(path).map_err(|e| format!("create output: {}", e.error))?;
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
    fn lines_fill_and_batches_are_correct_and_atomic() {
        let mut p = Project::new(5, 5).unwrap();
        let red = [255, 0, 0, 255];
        let blue = [0, 0, 255, 255];
        p.draw_line(0, [2, 0], [2, 4], red).unwrap();
        p.fill(0, 0, 0, blue).unwrap();
        assert_eq!(p.layers[0].pixels[0], blue);
        assert_eq!(p.layers[0].pixels[4], [0; 4]);
        assert_eq!(p.layers[0].pixels[22], red);
        let old = p.clone();
        let ops = [Operation::AddLayer, Operation::Pixel { layer: 99, x: 0, y: 0, color: red }];
        assert!(p.apply_operations(&ops).is_err());
        assert_eq!(p, old);
        assert!(p.draw_line(0, [0, 0], [5, 0], red).is_err());
        assert_eq!(p, old);
    }
    #[test]
    fn layer_edits_and_rectangles_validate_before_mutating() {
        let mut p = Project::new(4, 3).unwrap();
        p.add_layer().unwrap();
        p.rename_layer(1, "Foreground").unwrap();
        p.rectangle(1, 1, 1, 2, 2, [10, 20, 30, 255]).unwrap();
        assert_eq!(p.layers[1].pixels[5], [10, 20, 30, 255]);
        assert_eq!(p.layers[1].pixels[10], [10, 20, 30, 255]);
        assert_eq!(p.layers[1].pixels[3], [0; 4]);
        let old = p.clone();
        assert!(p.rectangle(1, u32::MAX, 0, 2, 1, [1; 4]).is_err());
        assert!(p.rectangle(1, 3, 2, 2, 1, [1; 4]).is_err());
        assert!(p.rename_layer(0, "  ").is_err());
        assert_eq!(p, old);
        p.move_layer(1, 0).unwrap();
        assert_eq!(p.layers[0].name, "Foreground");
        p.remove_layer(1).unwrap();
        assert!(p.remove_layer(0).is_err());
        assert_eq!(p.layers.len(), 1);
    }
    #[test]
    fn unknown_project_data_is_rejected_instead_of_silently_discarded() {
        let p = Project::new(1, 1).unwrap();
        let mut value = serde_json::to_value(p).unwrap();
        value["future_capability"] = serde_json::json!({"important": true});
        assert!(serde_json::from_value::<Project>(value).is_err());
    }
    #[test]
    fn invalid_layer_operations_roll_back_the_entire_batch() {
        let mut p = Project::new(2, 2).unwrap();
        let original = p.clone();
        let operations = [Operation::AddLayer, Operation::RenameLayer { layer: 1, name: "".into() }];
        assert!(p.apply_operations(&operations).is_err());
        assert_eq!(p, original);
    }
    #[test]
    fn history_undo_redo_and_branching_are_consistent() {
        let mut project = Project::new(1, 1).unwrap();
        let mut history = History::default();
        assert!(!history.can_undo());
        history.checkpoint(&project);
        project.set_pixel(0, 0, 0, [1, 2, 3, 255]).unwrap();
        assert!(history.undo(&mut project));
        assert_eq!(project.layers[0].pixels[0], [0; 4]);
        assert!(history.can_redo());
        assert!(history.redo(&mut project));
        assert_eq!(project.layers[0].pixels[0], [1, 2, 3, 255]);
        assert!(history.undo(&mut project));
        history.checkpoint(&project);
        project.set_pixel(0, 0, 0, [9, 8, 7, 255]).unwrap();
        assert!(!history.can_redo());
        history.clear();
        assert!(!history.can_undo());
    }
    #[test]
    fn history_transaction_preserves_redo_on_failure_or_noop() {
        let mut p = Project::new(1, 1).unwrap();
        let mut history = History::default();
        history.checkpoint(&p);
        p.set_pixel(0, 0, 0, [1, 2, 3, 255]).unwrap();
        assert!(history.undo(&mut p));
        assert!(history.can_redo());
        assert!(!history.apply(&mut p, |candidate| candidate.set_pixel(0, 0, 0, [0; 4])).unwrap());
        assert!(history.can_redo());
        assert!(history.apply(&mut p, |candidate| candidate.set_pixel(99, 0, 0, [3; 4])).is_err());
        assert!(history.can_redo());
        assert!(history.apply(&mut p, |candidate| candidate.set_pixel(0, 0, 0, [9; 4])).unwrap());
        assert!(!history.can_redo());
    }
    #[test]
    fn undo_redo_restore_selected_layer_after_reorder() {
        let mut p = Project::new(1, 1).unwrap();
        p.add_layer().unwrap();
        p.rename_layer(1, "Selected").unwrap();
        let mut selection = 1;
        let mut history = History::default();
        history.apply_with_selection(&mut p, selection, |project| project.move_layer(1, 0)).unwrap();
        selection = 0;
        assert!(history.undo_with_selection(&mut p, &mut selection));
        assert_eq!(selection, 1);
        assert_eq!(p.layers[selection].name, "Selected");
        assert!(history.redo_with_selection(&mut p, &mut selection));
        assert_eq!(selection, 0);
        assert_eq!(p.layers[selection].name, "Selected");
    }
    #[test]
    fn history_limits_snapshot_count() {
        let mut project = Project::new(1, 1).unwrap();
        let mut history = History::default();
        for i in 0..30 {
            history.checkpoint(&project);
            project.set_pixel(0, 0, 0, [i, 0, 0, 255]).unwrap();
        }
        let mut count = 0;
        while history.undo(&mut project) { count += 1; }
        assert_eq!(count, 20);
        assert!(history.can_redo());
    }
    #[test]
    fn rectangle_and_ellipse_masks_are_symmetric_and_respect_bounds() {
        let ink = [1, 2, 3, 255];
        for width in 1..=10 {
            for height in 1..=10 {
                let mut p = Project::new(width, height).unwrap();
                p.ellipse(0, 0, 0, width, height, true, ink).unwrap();
                let pixels = &p.layers[0].pixels;
                for y in 0..height {
                    for x in 0..width {
                        let at = |x, y| pixels[(y * width + x) as usize];
                        assert_eq!(at(x, y), at(width - 1 - x, y));
                        assert_eq!(at(x, y), at(x, height - 1 - y));
                    }
                }
                assert!(pixels.contains(&ink));
            }
        }
        let mut p = Project::new(5, 5).unwrap();
        p.ellipse(0, 0, 0, 5, 5, false, ink).unwrap();
        assert_eq!(p.layers[0].pixels[12], [0; 4]);
        assert_eq!(p.layers[0].pixels[2], ink);
        p.rectangle_outline(0, 1, 1, 3, 3, ink).unwrap();
        assert_eq!(p.layers[0].pixels[12], [0; 4]);
        let original = p.clone();
        assert!(p.ellipse(0, 4, 4, 2, 2, true, ink).is_err());
        assert!(p.rectangle_outline(0, u32::MAX, 0, 2, 1, ink).is_err());
        assert_eq!(p, original);
    }
    #[test]
    fn sampling_matches_composite_and_preserves_raw_hidden_pixels() {
        let mut p = Project::new(2, 1).unwrap();
        p.set_pixel(0, 0, 0, [20, 40, 60, 200]).unwrap();
        p.add_layer().unwrap();
        p.set_pixel(1, 0, 0, [200, 120, 50, 100]).unwrap();
        p.set_pixel(1, 1, 0, [7, 8, 9, 0]).unwrap();
        assert_eq!(p.sample_composite(0, 0).unwrap().as_slice(), &p.rgba().unwrap()[0..4]);
        assert_eq!(p.sample_layer(1, 1, 0).unwrap(), [7, 8, 9, 0]);
        p.layers[1].visible = false;
        assert_eq!(p.sample_layer(1, 0, 0).unwrap(), [200, 120, 50, 100]);
        assert_eq!(p.sample_composite(0, 0).unwrap(), [20, 40, 60, 200]);
        assert!(p.sample_composite(2, 0).is_err());
        assert!(p.sample_layer(2, 0, 0).is_err());
    }
    #[test]
    fn duplicate_flip_and_clear_preserve_layer_data_and_validate() {
        let mut p = Project::new(3, 2).unwrap();
        p.layers[0].name = "é".repeat(128);
        p.layers[0].visible = false;
        for (i, pixel) in p.layers[0].pixels.iter_mut().enumerate() { *pixel = [i as u8, 9, 8, 0]; }
        let original = p.layers[0].clone();
        p.add_layer().unwrap();
        assert_eq!(p.duplicate_layer(0).unwrap(), 1);
        assert_eq!(p.layers[1].pixels, original.pixels);
        assert!(!p.layers[1].visible);
        assert!(p.layers[1].name.ends_with(" copy"));
        assert!(p.layers[1].name.len() <= 256);
        p.flip_layer(1, FlipAxis::Horizontal).unwrap();
        assert_eq!(p.layers[1].pixels.iter().map(|p| p[0]).collect::<Vec<_>>(), [2, 1, 0, 5, 4, 3]);
        p.flip_layer(1, FlipAxis::Vertical).unwrap();
        assert_eq!(p.layers[1].pixels.iter().map(|p| p[0]).collect::<Vec<_>>(), [5, 4, 3, 2, 1, 0]);
        p.flip_layer(1, FlipAxis::Vertical).unwrap();
        p.flip_layer(1, FlipAxis::Horizontal).unwrap();
        assert_eq!(p.layers[1].pixels, original.pixels);
        p.clear_layer(1).unwrap();
        assert!(p.layers[1].pixels.iter().all(|pixel| *pixel == [0; 4]));
        assert_eq!(p.layers[0], original);
        let before = p.clone();
        assert!(p.duplicate_layer(99).is_err());
        assert!(p.flip_layer(99, FlipAxis::Horizontal).is_err());
        assert!(p.clear_layer(99).is_err());
        assert_eq!(p, before);
        while p.layers.len() < 32 { p.add_layer().unwrap(); }
        let full = p.clone();
        assert!(p.duplicate_layer(0).is_err());
        assert_eq!(p, full);
    }

    #[test]
    fn layer_transforms_are_atomic_in_batches_and_noops_preserve_redo() {
        let mut p = Project::new(2, 2).unwrap();
        let mut history = History::default();
        history.apply(&mut p, |p| p.set_pixel(0, 0, 0, [1, 2, 3, 4])).unwrap();
        assert!(history.undo(&mut p));
        assert!(!history.apply(&mut p, |p| p.clear_layer(0)).unwrap());
        assert!(history.can_redo());
        let before = p.clone();
        let ops = [Operation::DuplicateLayer { layer: 0 }, Operation::FlipLayer { layer: 99, axis: FlipAxis::Vertical }];
        assert!(p.apply_operations(&ops).is_err());
        assert_eq!(p, before);
        assert!(serde_json::from_str::<Operation>(r#"{"op":"flip_layer","layer":0,"axis":"diagonal"}"#).is_err());
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
