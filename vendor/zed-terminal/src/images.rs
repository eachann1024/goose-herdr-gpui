// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal Kitty graphics support for terminal image previews.
// ponytail: only direct, uncompressed transmissions are supported; add the remaining Kitty
// transports/compression only with bounded streaming decoders and a matching renderer contract.
// ponytail: retain preview coordinates through text redraw; complex reflow without a surviving
// grid marker needs a fresh placement from the application (Herdr replays on full redraw).

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    io::Cursor,
    sync::Arc,
};

use alacritty_terminal::{
    grid::Dimensions,
    index::{Column, Point},
    term::{Term, TermMode, cell::Hyperlink},
    vte::ansi::Handler,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gpui::RenderImage;
use image::{Frame, ImageFormat, ImageReader, Limits, RgbaImage};

use crate::ZedListener;

const MAX_ENCODED_BYTES: usize = 20 * 1024 * 1024;
const MAX_RGBA_BYTES: usize = 64 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 128 * 1024 * 1024;
const MAX_PLACEMENTS: usize = 1024;
const MAX_CHUNK_BYTES: usize = 4096;
// GPUI 0.2.2's bucketed atlas is created at max(image, 1024). An exact-size
// allocation can fail, so keep preview bitmaps strictly inside the default tile.
const MAX_ATLAS_EDGE: u32 = 1022;

pub struct ImagePlacement {
    pub image: Arc<RenderImage>,
    pub column: f32,
    pub line: f32,
    pub columns: f32,
    pub rows: f32,
}

#[derive(Default)]
pub struct ImageStore {
    inner: RefCell<Store>,
}

#[derive(Default)]
struct Store {
    images: HashMap<u32, DecodedImage>,
    placements: HashMap<u64, Placement>,
    placement_ids: HashMap<(u32, u32), u64>,
    pending: Option<Pending>,
    next_image_id: u32,
    next_anchor_id: u64,
    cache_bytes: usize,
}

struct Pending {
    fields: Fields,
    encoded: Vec<u8>,
}

struct DecodedImage {
    width: u32,
    height: u32,
    bgra: Arc<Vec<u8>>,
}

struct Placement {
    image: Arc<RenderImage>,
    image_id: u32,
    bytes: usize,
    columns: Option<u32>,
    rows: Option<u32>,
    width: u32,
    height: u32,
    point: Point,
    alternate: bool,
}

#[derive(Default, Clone)]
struct Fields {
    action: u8,
    delete: Option<u8>,
    image_id: Option<u32>,
    placement_id: Option<u32>,
    format: Option<u32>,
    transport: Option<u8>,
    more: bool,
    quiet: u8,
    columns: Option<u32>,
    rows: Option<u32>,
    keep_cursor: bool,
    x: Option<u32>,
    y: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    pixel_width: Option<u32>,
    pixel_height: Option<u32>,
}

impl ImageStore {
    /// Handles the bytes after `ESC_G` (without the terminating ST).
    pub fn handle(&mut self, body: &[u8], term: &mut Term<ZedListener>) -> Vec<u8> {
        let mut store = self.inner.borrow_mut();
        let Some((mut fields, payload)) = parse(body) else {
            store.pending = None;
            // Malformed controls cannot reliably identify a request or its quiet flag.
            return Vec::new();
        };
        let continuation = fields.action == 0 && store.pending.is_some();
        let more = fields.more;
        if fields.action != 0 {
            store.pending = None;
        }
        let mut packet = if continuation {
            store.pending.take().unwrap()
        } else {
            if fields.action == 0 {
                fields.action = b't';
            }
            Pending {
                fields,
                encoded: Vec::new(),
            }
        };
        if (more || continuation) && payload.len() > MAX_CHUNK_BYTES
            || packet.encoded.len().saturating_add(payload.len()) > MAX_ENCODED_BYTES
        {
            return response(
                packet.fields.image_id.unwrap_or(0),
                packet.fields.quiet,
                false,
                "E2BIG",
            );
        }
        packet.encoded.extend_from_slice(payload);
        if more {
            if !matches!(packet.fields.action, b'T' | b't') {
                return response(
                    packet.fields.image_id.unwrap_or(0),
                    packet.fields.quiet,
                    false,
                    "EINVAL",
                );
            }
            store.pending = Some(packet);
            return Vec::new();
        }
        let fields = packet.fields;
        let image_id = fields.image_id.unwrap_or_else(|| store.next_image_id());
        let result = match fields.action {
            b'q' => query(&fields),
            b'd' => store.delete(&fields),
            b'p' => store.place(image_id, &fields, term),
            b'T' | b't' => store.transmit(image_id, &fields, &packet.encoded, term),
            _ => Err("EINVAL"),
        };
        response(
            image_id,
            fields.quiet,
            result.is_ok(),
            result.err().unwrap_or("OK"),
        )
    }

    pub fn reset(&mut self) {
        *self.inner.get_mut() = Store::default();
    }
    pub fn clear_alternate(&mut self) {
        let store = self.inner.get_mut();
        let ids: Vec<_> = store
            .placements
            .iter()
            .filter_map(|(&id, p)| p.alternate.then_some(id))
            .collect();
        for id in ids {
            store.remove_placement(id);
        }
    }

    pub fn placements(
        &self,
        term: &Term<ZedListener>,
        cell_width: f32,
        line_height: f32,
    ) -> Vec<ImagePlacement> {
        if !cell_width.is_finite()
            || !line_height.is_finite()
            || cell_width <= 0.
            || line_height <= 0.
        {
            return Vec::new();
        }
        let mut store = self.inner.borrow_mut();
        if store.placements.is_empty() {
            return Vec::new();
        }
        let offset = term.grid().display_offset() as i32;
        let rows = term.grid().screen_lines();
        let columns = term.grid().columns();
        let above = store
            .placements
            .values()
            .map(|placement| placement.rows.unwrap_or(4096))
            .max()
            .unwrap_or(1) as i32;
        let first_line = (-offset - above).max(term.grid().topmost_line().0);
        let last_line = rows as i32 - 1 - offset;
        let alternate = term.mode().contains(TermMode::ALT_SCREEN);
        let mut found = HashSet::new();
        let mut result = Vec::new();
        for line in first_line..=last_line {
            for column in 0..columns {
                let cell = &term.grid()[Point::new(line.into(), Column(column))];
                let Some(id) = image_anchor(cell.hyperlink().as_ref().map(Hyperlink::uri)) else {
                    continue;
                };
                let Some(placement) = store.placements.get_mut(&id) else {
                    continue;
                };
                if placement.alternate != alternate {
                    continue;
                }
                placement.point = Point::new(line.into(), Column(column));
                found.insert(id);
                result.push(ImagePlacement {
                    image: placement.image.clone(),
                    column: column as f32,
                    line: (line + offset) as f32,
                    columns: placement
                        .columns
                        .unwrap_or_else(|| placement.width.div_ceil(cell_width.ceil() as u32))
                        .clamp(1, 4096) as f32,
                    rows: placement
                        .rows
                        .unwrap_or_else(|| placement.height.div_ceil(line_height.ceil() as u32))
                        .clamp(1, 4096) as f32,
                });
            }
        }
        for (&id, placement) in &store.placements {
            if found.contains(&id) || placement.alternate != alternate {
                continue;
            }
            let line = placement.point.line.0 + offset;
            if line >= -above && line < rows as i32 {
                result.push(ImagePlacement {
                    image: placement.image.clone(),
                    column: placement.point.column.0 as f32,
                    line: line as f32,
                    columns: placement
                        .columns
                        .unwrap_or_else(|| placement.width.div_ceil(cell_width.ceil() as u32))
                        .clamp(1, 4096) as f32,
                    rows: placement
                        .rows
                        .unwrap_or_else(|| placement.height.div_ceil(line_height.ceil() as u32))
                        .clamp(1, 4096) as f32,
                });
            }
        }
        result
    }
}

impl Store {
    fn next_image_id(&mut self) -> u32 {
        self.next_image_id = self.next_image_id.wrapping_add(1).max(1);
        self.next_image_id
    }

    fn next_anchor_id(&mut self) -> u64 {
        self.next_anchor_id = self.next_anchor_id.wrapping_add(1).max(1);
        self.next_anchor_id
    }

    fn transmit(
        &mut self,
        image_id: u32,
        fields: &Fields,
        encoded: &[u8],
        term: &mut Term<ZedListener>,
    ) -> Result<(), &'static str> {
        if fields.transport.unwrap_or(b'd') != b'd' || encoded.len() > MAX_ENCODED_BYTES {
            return Err("EINVAL");
        }
        if !self.images.contains_key(&image_id) && self.images.len() >= MAX_PLACEMENTS {
            return Err("ENOSPC");
        }
        let bytes = STANDARD.decode(encoded).map_err(|_| "EBADMSG")?;
        let image = decode(fields, &bytes)?;
        self.remove_image(image_id);
        self.cache_bytes = self.cache_bytes.saturating_add(image.bgra.len());
        self.images.insert(image_id, image);
        if self.cache_bytes > MAX_CACHE_BYTES {
            self.remove_image(image_id);
            return Err("ENOSPC");
        }
        if fields.action == b'T' {
            self.place(image_id, fields, term)?;
        }
        Ok(())
    }

    fn place(
        &mut self,
        image_id: u32,
        fields: &Fields,
        term: &mut Term<ZedListener>,
    ) -> Result<(), &'static str> {
        let point = term.grid().cursor.point;
        let old_anchor = image_anchor(term.grid()[point].hyperlink().as_ref().map(Hyperlink::uri));
        if self.placements.len() >= MAX_PLACEMENTS && old_anchor.is_none() {
            return Err("ENOSPC");
        }
        let source = self.images.get(&image_id).ok_or("ENOENT")?;
        let (image, bytes) = render_crop(source, fields)?;
        if self.cache_bytes.saturating_add(bytes) > MAX_CACHE_BYTES {
            return Err("ENOSPC");
        }
        let columns = fields.columns.map(|value| value.clamp(1, 4096));
        let rows = fields.rows.map(|value| value.clamp(1, 4096));
        let (crop_width, crop_height) = crop_dimensions(source, fields)?;
        if let Some(old_anchor) = old_anchor {
            self.remove_placement(old_anchor);
        }
        if let Some(old_anchor) = fields
            .placement_id
            .and_then(|id| self.placement_ids.get(&(image_id, id)).copied())
        {
            self.remove_placement(old_anchor);
        }
        let anchor_id = self.next_anchor_id();
        term.grid_mut()[point].set_hyperlink(Some(Hyperlink::new(
            Some(anchor_id),
            format!("goose-image:{anchor_id}"),
        )));
        self.cache_bytes += bytes;
        self.placements.insert(
            anchor_id,
            Placement {
                image,
                image_id,
                bytes,
                columns,
                rows,
                width: crop_width,
                height: crop_height,
                point,
                alternate: term.mode().contains(TermMode::ALT_SCREEN),
            },
        );
        if let Some(id) = fields.placement_id {
            self.placement_ids.insert((image_id, id), anchor_id);
        }
        if !fields.keep_cursor {
            advance_cursor(term, columns.unwrap_or(1), rows.unwrap_or(1));
        }
        Ok(())
    }

    fn delete(&mut self, fields: &Fields) -> Result<(), &'static str> {
        // Lowercase selectors preserve uploads for Herdr/Pi placement-only redraws.
        match fields.delete.unwrap_or(b'a') {
            b'i' => {
                let image_id = fields.image_id.ok_or("EINVAL")?;
                if let Some(placement_id) = fields.placement_id {
                    self.placement_ids
                        .get(&(image_id, placement_id))
                        .copied()
                        .map(|id| self.remove_placement(id))
                        .ok_or("EINVAL")
                } else {
                    let ids: Vec<_> = self
                        .placements
                        .iter()
                        .filter_map(|(&id, placement)| {
                            (placement.image_id == image_id).then_some(id)
                        })
                        .collect();
                    for id in ids {
                        self.remove_placement(id);
                    }
                    Ok(())
                }
            }
            b'I' => fields
                .image_id
                .map(|id| self.remove_image(id))
                .ok_or("EINVAL"),
            b'P' | b'p' => fields
                .image_id
                .zip(fields.placement_id)
                .and_then(|key| self.placement_ids.get(&key).copied())
                .map(|id| self.remove_placement(id))
                .ok_or("EINVAL"),
            b'a' => {
                let freed: usize = self
                    .placements
                    .values()
                    .map(|placement| placement.bytes)
                    .sum();
                self.placements.clear();
                self.placement_ids.clear();
                self.cache_bytes = self.cache_bytes.saturating_sub(freed);
                Ok(())
            }
            b'A' => {
                self.images.clear();
                self.placements.clear();
                self.placement_ids.clear();
                self.cache_bytes = 0;
                Ok(())
            }
            _ => Err("EINVAL"),
        }
    }

    fn remove_placement(&mut self, anchor_id: u64) {
        if let Some(placement) = self.placements.remove(&anchor_id) {
            self.cache_bytes = self.cache_bytes.saturating_sub(placement.bytes);
        }
        self.placement_ids.retain(|_, id| *id != anchor_id);
    }

    fn remove_image(&mut self, image_id: u32) {
        if let Some(image) = self.images.remove(&image_id) {
            self.cache_bytes = self.cache_bytes.saturating_sub(image.bgra.len());
        }
        let removed: Vec<_> = self
            .placements
            .iter()
            .filter_map(|(&id, placement)| (placement.image_id == image_id).then_some(id))
            .collect();
        for id in removed {
            self.remove_placement(id);
        }
    }
}

fn parse(body: &[u8]) -> Option<(Fields, &[u8])> {
    let separator = body.iter().position(|byte| *byte == b';');
    let (controls, payload) = match separator {
        Some(index) => (&body[..index], &body[index + 1..]),
        None => (body, &[][..]),
    };
    let mut fields = Fields::default();
    for item in controls.split(|byte| *byte == b',') {
        let separator = item.iter().position(|byte| *byte == b'=')?;
        let (key, value) = (&item[..separator], &item[separator + 1..]);
        let value = std::str::from_utf8(value).ok()?;
        match key {
            b"a" if value.len() == 1 => fields.action = value.as_bytes()[0],
            b"d" if value.len() == 1 => fields.delete = Some(value.as_bytes()[0]),
            b"i" => fields.image_id = Some(value.parse().ok()?),
            b"p" => fields.placement_id = Some(value.parse().ok()?),
            b"f" => fields.format = Some(value.parse().ok()?),
            b"t" if value.len() == 1 => fields.transport = Some(value.as_bytes()[0]),
            b"m" if matches!(value, "0" | "1") => fields.more = value == "1",
            b"q" if matches!(value, "0" | "1" | "2") => fields.quiet = value.parse().ok()?,
            b"c" => fields.columns = Some(value.parse().ok()?),
            b"r" => fields.rows = Some(value.parse().ok()?),
            b"C" if matches!(value, "0" | "1") => fields.keep_cursor = value == "1",
            b"x" => fields.x = Some(value.parse().ok()?),
            b"y" => fields.y = Some(value.parse().ok()?),
            b"w" => fields.width = Some(value.parse().ok()?),
            b"h" => fields.height = Some(value.parse().ok()?),
            b"s" => fields.pixel_width = Some(value.parse().ok()?),
            b"v" => fields.pixel_height = Some(value.parse().ok()?),
            b"U" | b"o" | b"a" | b"d" | b"t" | b"m" | b"q" | b"C" => return None,
            b"z" => {}
            _ => {}
        }
    }
    Some((fields, payload))
}

fn query(fields: &Fields) -> Result<(), &'static str> {
    if fields.transport.unwrap_or(b'd') != b'd' {
        return Err("ENOTSUP");
    }
    match fields.format {
        Some(24 | 32 | 100) => Ok(()),
        _ => Err("EINVAL"),
    }
}

fn decode(fields: &Fields, bytes: &[u8]) -> Result<DecodedImage, &'static str> {
    match fields.format {
        Some(24) => decode_rgb(fields, bytes),
        Some(32) => decode_rgba(fields, bytes),
        Some(100) => decode_encoded(bytes),
        _ => Err("EINVAL"),
    }
}

fn decode_rgb(fields: &Fields, bytes: &[u8]) -> Result<DecodedImage, &'static str> {
    let (width, height) = (
        fields.pixel_width.ok_or("EINVAL")?,
        fields.pixel_height.ok_or("EINVAL")?,
    );
    let pixels = rgba_bytes(width, height)?;
    if bytes.len() != pixels / 4 * 3 {
        return Err("EBADMSG");
    }
    let mut bgra = Vec::with_capacity(pixels);
    for rgb in bytes.chunks_exact(3) {
        bgra.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
    }
    Ok(DecodedImage {
        width,
        height,
        bgra: Arc::new(bgra),
    })
}

fn decode_rgba(fields: &Fields, bytes: &[u8]) -> Result<DecodedImage, &'static str> {
    let (width, height) = (
        fields.pixel_width.ok_or("EINVAL")?,
        fields.pixel_height.ok_or("EINVAL")?,
    );
    let pixels = rgba_bytes(width, height)?;
    if bytes.len() != pixels {
        return Err("EBADMSG");
    }
    let mut bgra = bytes.to_vec();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(DecodedImage {
        width,
        height,
        bgra: Arc::new(bgra),
    })
}

fn decode_encoded(bytes: &[u8]) -> Result<DecodedImage, &'static str> {
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_RGBA_BYTES as u64);
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "EBADMSG")?;
    reader.limits(limits);
    match reader.format() {
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP) => {}
        _ => return Err("EINVAL"),
    }
    let image = fit_atlas(reader.decode().map_err(|_| "EBADMSG")?.into_rgba8())?;
    let (width, height) = image.dimensions();
    let bytes_len = rgba_bytes(width, height)?;
    if image.len() != bytes_len {
        return Err("E2BIG");
    }
    let mut bgra = image.into_raw();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(DecodedImage {
        width,
        height,
        bgra: Arc::new(bgra),
    })
}

fn fit_atlas(image: RgbaImage) -> Result<RgbaImage, &'static str> {
    let (width, height) = image.dimensions();
    if width <= MAX_ATLAS_EDGE && height <= MAX_ATLAS_EDGE {
        return Ok(image);
    }
    let scale = MAX_ATLAS_EDGE as f32 / width.max(height) as f32;
    let width = ((width as f32) * scale).round().max(1.) as u32;
    let height = ((height as f32) * scale).round().max(1.) as u32;
    rgba_bytes(width, height)?;
    Ok(image::imageops::resize(
        &image,
        width,
        height,
        image::imageops::FilterType::Triangle,
    ))
}

fn crop_dimensions(source: &DecodedImage, fields: &Fields) -> Result<(u32, u32), &'static str> {
    let x = fields.x.unwrap_or(0).min(source.width);
    let y = fields.y.unwrap_or(0).min(source.height);
    let width = fields
        .width
        .unwrap_or(source.width.saturating_sub(x))
        .min(source.width.saturating_sub(x));
    let height = fields
        .height
        .unwrap_or(source.height.saturating_sub(y))
        .min(source.height.saturating_sub(y));
    if width == 0 || height == 0 {
        return Err("EINVAL");
    }
    Ok((width, height))
}

fn render_crop(
    source: &DecodedImage,
    fields: &Fields,
) -> Result<(Arc<RenderImage>, usize), &'static str> {
    let x = fields.x.unwrap_or(0).min(source.width);
    let y = fields.y.unwrap_or(0).min(source.height);
    let (width, height) = crop_dimensions(source, fields)?;
    let bytes = rgba_bytes(width, height)?;
    let mut cropped = Vec::with_capacity(bytes);
    for row in y..y + height {
        let start = ((row * source.width + x) * 4) as usize;
        cropped.extend_from_slice(&source.bgra[start..start + width as usize * 4]);
    }
    let buffer = RgbaImage::from_raw(width, height, cropped).ok_or("EBADMSG")?;
    Ok((Arc::new(RenderImage::new([Frame::new(buffer)])), bytes))
}

fn rgba_bytes(width: u32, height: u32) -> Result<usize, &'static str> {
    if width == 0 || height == 0 {
        return Err("EINVAL");
    }
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("E2BIG")?;
    (bytes <= MAX_RGBA_BYTES).then_some(bytes).ok_or("E2BIG")
}

fn advance_cursor(term: &mut Term<ZedListener>, columns: u32, rows: u32) {
    let width = term.grid().columns();
    if width == 0 {
        return;
    }
    for _ in 1..rows {
        Handler::linefeed(term);
    }
    let total = term
        .grid()
        .cursor
        .point
        .column
        .0
        .saturating_add(columns as usize);
    for _ in 0..total / width {
        Handler::linefeed(term);
    }
    term.grid_mut().cursor.point.column = Column(total % width);
    term.grid_mut().cursor.input_needs_wrap = false;
}

fn image_anchor(uri: Option<&str>) -> Option<u64> {
    uri?.strip_prefix("goose-image:")?.parse().ok()
}

fn response(image_id: u32, quiet: u8, ok: bool, message: &str) -> Vec<u8> {
    if quiet == 2 || (quiet == 1 && ok) {
        return Vec::new();
    }
    let status = if ok { "OK" } else { message };
    format!("\x1b_Gi={image_id};{status}\x1b\\").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_chunks_defaults_redraw_and_screen_isolation() {
        use alacritty_terminal::vte::ansi::Processor;
        let (tx, _) = futures::channel::mpsc::unbounded();
        let mut term = Term::new(
            Default::default(),
            &crate::TerminalBounds::default(),
            ZedListener(tx),
        );
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255])))
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let encoded = STANDARD.encode(png.into_inner());
        let mut store = ImageStore::default();
        // Pi omits t=d and sends only m on all continuation packets.
        for packet in [
            format!("a=T,f=100,q=2,C=1,c=1,r=1,i=91,m=1;{}", &encoded[..4]),
            format!("m=1;{}", &encoded[4..8]),
            format!("m=0;{}", &encoded[8..]),
        ] {
            assert!(store.handle(packet.as_bytes(), &mut term).is_empty());
        }
        assert_eq!(term.grid().cursor.point.column, Column(0));
        let placements = store.placements(&term, 9., 18.);
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].image.as_bytes(0).unwrap(), &[0, 0, 255, 255]);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, b"\x1b[1;1H ");
        assert_eq!(store.placements(&term, 9., 18.).len(), 1);
        parser.advance(&mut term, b"\x1b[?1049h");
        assert!(store.placements(&term, 9., 18.).is_empty());
        parser.advance(&mut term, b"\x1b[?1049l");
        assert_eq!(store.placements(&term, 9., 18.).len(), 1);
        store.handle(b"a=d,d=a,q=2;", &mut term);
        assert!(store.placements(&term, 9., 18.).is_empty());
        assert_eq!(
            store.handle(b"a=p,i=91,C=1,q=0;", &mut term),
            b"\x1b_Gi=91;OK\x1b\\"
        );
        assert!(
            String::from_utf8(store.handle(b"a=q,t=f,f=100,i=5;ignored", &mut term))
                .unwrap()
                .contains("ENOTSUP")
        );
        assert!(rgba_bytes(0, 1).is_err());
        assert!(rgba_bytes(u32::MAX, u32::MAX).is_err());
        store.reset();
        assert!(store.placements(&term, 9., 18.).is_empty());
    }

    fn encode_image(format: ImageFormat, pixel: [u8; 4]) -> String {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, image::Rgba(pixel)))
            .write_to(&mut bytes, format)
            .unwrap();
        STANDARD.encode(bytes.into_inner())
    }

    fn place_encoded(store: &mut ImageStore, term: &mut Term<ZedListener>, id: u32, encoded: &str) {
        let chunk = encoded.len().min(4);
        for packet in [
            format!(
                "a=T,f=100,q=2,C=1,c=8,r=4,i={id},m=1;{}",
                &encoded[..chunk]
            ),
            format!("m=0;{}", &encoded[chunk..]),
        ] {
            assert!(store.handle(packet.as_bytes(), term).is_empty());
        }
    }

    #[test]
    fn second_preview_keeps_jpeg_and_prior_png() {
        use alacritty_terminal::vte::ansi::Processor;
        let (tx, _) = futures::channel::mpsc::unbounded();
        let mut term = Term::new(
            Default::default(),
            &crate::TerminalBounds::default(),
            ZedListener(tx),
        );
        let mut store = ImageStore::default();
        let png = encode_image(ImageFormat::Png, [255, 0, 0, 255]);
        let jpeg = encode_image(ImageFormat::Jpeg, [0, 255, 0, 255]);
        place_encoded(&mut store, &mut term, 11, &png);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, b"\x1b[6;1H");
        place_encoded(&mut store, &mut term, 12, &jpeg);
        let placements = store.placements(&term, 9., 18.);
        assert_eq!(placements.len(), 2);
        assert_eq!((placements[0].line, placements[0].column), (0., 0.));
        assert_eq!((placements[1].line, placements[1].column), (5., 0.));
        let fitted = fit_atlas(RgbaImage::from_pixel(
            MAX_ATLAS_EDGE + 80,
            MAX_ATLAS_EDGE + 40,
            image::Rgba([1, 2, 3, 255]),
        ))
        .unwrap();
        assert!(fitted.width() <= MAX_ATLAS_EDGE);
        assert!(fitted.height() <= MAX_ATLAS_EDGE);
    }

    #[test]
    fn bridge_upload_place_delete_and_reuse_placement_ids() {
        use alacritty_terminal::term::Config;
        use futures::channel::mpsc::unbounded;

        let (tx, _) = unbounded();
        let mut term = Term::new(
            Config::default(),
            &crate::TerminalBounds::default(),
            ZedListener(tx),
        );
        let mut store = ImageStore::default();
        let upload = |store: &mut ImageStore, term: &mut Term<ZedListener>, id, rgba: &[u8]| {
            store.handle(
                format!("a=t,f=32,t=d,i={id},s=1,v=1,q=0;{}", STANDARD.encode(rgba)).as_bytes(),
                term,
            )
        };
        assert_eq!(
            upload(&mut store, &mut term, 7, &[255, 0, 0, 255]),
            b"\x1b_Gi=7;OK\x1b\\"
        );
        assert_eq!(
            store.handle(b"a=p,i=7,p=3,c=1,r=1,C=1,q=0;", &mut term),
            b"\x1b_Gi=7;OK\x1b\\"
        );
        term.grid_mut().cursor.point.column = Column(1);
        assert_eq!(
            upload(&mut store, &mut term, 8, &[0, 255, 0, 255]),
            b"\x1b_Gi=8;OK\x1b\\"
        );
        assert_eq!(
            store.handle(b"a=p,i=8,p=3,c=1,r=1,C=1,q=0;", &mut term),
            b"\x1b_Gi=8;OK\x1b\\"
        );
        assert_eq!(store.inner.borrow().placements.len(), 2); // p is scoped by image id.
        assert_eq!(
            store.handle(b"a=d,d=i,i=7,p=3,q=0;", &mut term),
            b"\x1b_Gi=7;OK\x1b\\"
        );
        assert_eq!(
            store.handle(b"a=p,i=7,p=3,c=1,r=1,C=1,q=0;", &mut term),
            b"\x1b_Gi=7;OK\x1b\\"
        );
        let decoded = decode(
            &parse(b"a=t,f=32,s=1,v=1;/wAA/w==").unwrap().0,
            &STANDARD.decode(b"/wAA/w==").unwrap(),
        )
        .unwrap();
        assert_eq!(&*decoded.bgra, &[0, 0, 255, 255]);
    }
}
