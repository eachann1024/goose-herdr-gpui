// SPDX-License-Identifier: GPL-3.0-or-later
// Extracted from Zed 69e2130295c2649963eb639fc70b4f2ee8ea1624. See SOURCES.md.
use crate::{IndexedCell, TerminalBounds, palette::Palette};
use alacritty_terminal::{
    index::Point as AlacPoint,
    term::cell::Flags,
    vte::ansi::{Color as AnsiColor, Color::Named, NamedColor},
};
use gpui::*;
use itertools::Itertools;
use std::{mem, ops::RangeInclusive, time::Instant};
/// A batched text run that combines multiple adjacent cells with the same style
#[derive(Debug)]
pub struct BatchedTextRun {
    pub start_point: AlacPoint<i32, i32>,
    pub text: String,
    pub cell_count: usize,
    pub style: TextRun,
    pub font_size: AbsoluteLength,
}

impl BatchedTextRun {
    fn new_from_char(
        start_point: AlacPoint<i32, i32>,
        c: char,
        style: TextRun,
        font_size: AbsoluteLength,
    ) -> Self {
        let mut text = String::with_capacity(100); // Pre-allocate for typical line length
        text.push(c);
        BatchedTextRun {
            start_point,
            text,
            cell_count: 1,
            style,
            font_size,
        }
    }

    fn can_append(&self, other_style: &TextRun) -> bool {
        self.style.font == other_style.font
            && self.style.color == other_style.color
            && self.style.background_color == other_style.background_color
            && self.style.underline == other_style.underline
            && self.style.strikethrough == other_style.strikethrough
    }

    fn append_char(&mut self, c: char) {
        self.append_char_internal(c, true);
    }

    fn append_zero_width_chars(&mut self, chars: &[char]) {
        for &c in chars {
            self.append_char_internal(c, false);
        }
    }

    fn append_char_internal(&mut self, c: char, counts_cell: bool) {
        self.text.push(c);
        if counts_cell {
            self.cell_count += 1;
        }
        self.style.len += c.len_utf8();
    }

    pub fn paint(
        &self,
        origin: Point<Pixels>,
        dimensions: &TerminalBounds,
        window: &mut Window,
        cx: &mut App,
    ) {
        let pos = Point::new(
            origin.x + self.start_point.column as f32 * dimensions.cell_width,
            origin.y + self.start_point.line as f32 * dimensions.line_height,
        );

        let _ = window
            .text_system()
            .shape_line(
                self.text.clone().into(),
                self.font_size.to_pixels(window.rem_size()),
                std::slice::from_ref(&self.style),
                Some(dimensions.cell_width),
            )
            .paint(pos, dimensions.line_height, window, cx);
    }
}

#[derive(Clone, Debug, Default)]
pub struct LayoutRect {
    point: AlacPoint<i32, i32>,
    num_of_cells: usize,
    color: Hsla,
}

impl LayoutRect {
    fn new(point: AlacPoint<i32, i32>, num_of_cells: usize, color: Hsla) -> LayoutRect {
        LayoutRect {
            point,
            num_of_cells,
            color,
        }
    }

    pub fn paint(&self, origin: Point<Pixels>, dimensions: &TerminalBounds, window: &mut Window) {
        let position = {
            let alac_point = self.point;
            point(
                (origin.x + alac_point.column as f32 * dimensions.cell_width).floor(),
                origin.y + alac_point.line as f32 * dimensions.line_height,
            )
        };
        let size = point(
            (dimensions.cell_width * self.num_of_cells as f32).ceil(),
            dimensions.line_height,
        )
        .into();

        window.paint_quad(fill(Bounds::new(position, size), self.color));
    }
}

/// Represents a rectangular region with a specific background color
#[derive(Debug, Clone)]
struct BackgroundRegion {
    start_line: i32,
    start_col: i32,
    end_line: i32,
    end_col: i32,
    color: Hsla,
}

impl BackgroundRegion {
    fn new(line: i32, col: i32, color: Hsla) -> Self {
        BackgroundRegion {
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col,
            color,
        }
    }

    /// Check if this region can be merged with another region
    fn can_merge_with(&self, other: &BackgroundRegion) -> bool {
        if self.color != other.color {
            return false;
        }

        // Check if regions are adjacent horizontally
        if self.start_line == other.start_line && self.end_line == other.end_line {
            return self.end_col + 1 == other.start_col || other.end_col + 1 == self.start_col;
        }

        // Check if regions are adjacent vertically with same column span
        if self.start_col == other.start_col && self.end_col == other.end_col {
            return self.end_line + 1 == other.start_line || other.end_line + 1 == self.start_line;
        }

        false
    }

    /// Merge this region with another region
    fn merge_with(&mut self, other: &BackgroundRegion) {
        self.start_line = self.start_line.min(other.start_line);
        self.start_col = self.start_col.min(other.start_col);
        self.end_line = self.end_line.max(other.end_line);
        self.end_col = self.end_col.max(other.end_col);
    }
}

/// Merge background regions to minimize the number of rectangles
fn merge_background_regions(regions: Vec<BackgroundRegion>) -> Vec<BackgroundRegion> {
    if regions.is_empty() {
        return regions;
    }

    let mut merged = regions;
    let mut changed = true;

    // Keep merging until no more merges are possible
    while changed {
        changed = false;
        let mut i = 0;

        while i < merged.len() {
            let mut j = i + 1;
            while j < merged.len() {
                if merged[i].can_merge_with(&merged[j]) {
                    let other = merged.remove(j);
                    merged[i].merge_with(&other);
                    changed = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }

    merged
}

/// The GPUI element that paints the terminal.
/// We need to keep a reference to the model for mouse events, do we need it for any other terminal stuff, or can we move that to connection?
pub struct TerminalElement;
impl TerminalElement {
    pub fn layout_grid(
        grid: impl Iterator<Item = IndexedCell>,
        start_line_offset: i32,
        text_style: &TextStyle,
        hyperlink: Option<(HighlightStyle, &RangeInclusive<AlacPoint>)>,
        minimum_contrast: f32,
        theme: &Palette,
    ) -> (Vec<LayoutRect>, Vec<BatchedTextRun>) {
        let start_time = Instant::now();

        // Pre-allocate with estimated capacity to reduce reallocations
        let estimated_cells = grid.size_hint().0;
        let estimated_runs = estimated_cells / 10; // Estimate ~10 cells per run
        let estimated_regions = estimated_cells / 20; // Estimate ~20 cells per background region

        let mut batched_runs = Vec::with_capacity(estimated_runs);
        let mut cell_count = 0;

        // Collect background regions for efficient merging
        let mut background_regions: Vec<BackgroundRegion> = Vec::with_capacity(estimated_regions);
        let mut current_batch: Option<BatchedTextRun> = None;

        // First pass: collect all cells and their backgrounds
        let linegroups = grid.into_iter().chunk_by(|i| i.point.line);
        for (line_index, (_, line)) in linegroups.into_iter().enumerate() {
            let alac_line = start_line_offset + line_index as i32;

            // Flush any existing batch at line boundaries
            if let Some(batch) = current_batch.take() {
                batched_runs.push(batch);
            }

            let mut previous_cell_had_extras = false;

            for cell in line {
                let mut fg = cell.fg;
                let mut bg = cell.bg;
                if cell.flags.contains(Flags::INVERSE) {
                    mem::swap(&mut fg, &mut bg);
                }

                // Collect background regions (skip default background)
                if !matches!(bg, Named(NamedColor::Background)) {
                    let color = theme.color(bg);
                    let col = cell.point.column.0 as i32;

                    // Try to extend the last region if it's on the same line with the same color
                    if let Some(last_region) = background_regions.last_mut() {
                        if last_region.color == color
                            && last_region.start_line == alac_line
                            && last_region.end_line == alac_line
                            && last_region.end_col + 1 == col
                        {
                            last_region.end_col = col;
                        } else {
                            background_regions.push(BackgroundRegion::new(alac_line, col, color));
                        }
                    } else {
                        background_regions.push(BackgroundRegion::new(alac_line, col, color));
                    }
                }
                // Skip wide character spacers - they're just placeholders for the second cell of wide characters
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }

                // Skip spaces that follow cells with extras (emoji variation sequences)
                if cell.c == ' ' && previous_cell_had_extras {
                    previous_cell_had_extras = false;
                    continue;
                }
                // Update tracking for next iteration
                previous_cell_had_extras =
                    matches!(cell.zerowidth(), Some(chars) if !chars.is_empty());

                //Layout current cell text
                {
                    if !is_blank(&cell) && !cell.flags.contains(Flags::HIDDEN) {
                        cell_count += 1;
                        let cell_style = TerminalElement::cell_style(
                            &cell,
                            fg,
                            bg,
                            theme,
                            text_style,
                            hyperlink,
                            minimum_contrast,
                        );

                        let cell_point = AlacPoint::new(alac_line, cell.point.column.0 as i32);
                        let zero_width_chars = cell.zerowidth();

                        // Try to batch with existing run
                        if let Some(ref mut batch) = current_batch {
                            if batch.can_append(&cell_style)
                                && batch.start_point.line == cell_point.line
                                && batch.start_point.column + batch.cell_count as i32
                                    == cell_point.column
                            {
                                batch.append_char(cell.c);
                                if let Some(chars) = zero_width_chars {
                                    batch.append_zero_width_chars(chars);
                                }
                            } else {
                                // Flush current batch and start new one
                                let old_batch = current_batch.take().unwrap();
                                batched_runs.push(old_batch);
                                let mut new_batch = BatchedTextRun::new_from_char(
                                    cell_point,
                                    cell.c,
                                    cell_style,
                                    text_style.font_size,
                                );
                                if let Some(chars) = zero_width_chars {
                                    new_batch.append_zero_width_chars(chars);
                                }
                                current_batch = Some(new_batch);
                            }
                        } else {
                            // Start new batch
                            let mut new_batch = BatchedTextRun::new_from_char(
                                cell_point,
                                cell.c,
                                cell_style,
                                text_style.font_size,
                            );
                            if let Some(chars) = zero_width_chars {
                                new_batch.append_zero_width_chars(chars);
                            }
                            current_batch = Some(new_batch);
                        }
                    };
                }
            }
        }

        // Flush any remaining batch
        if let Some(batch) = current_batch {
            batched_runs.push(batch);
        }

        // Second pass: merge background regions and convert to layout rects
        let region_count = background_regions.len();
        let merged_regions = merge_background_regions(background_regions);
        let mut rects = Vec::with_capacity(merged_regions.len() * 2); // Estimate 2 rects per merged region

        // Convert merged regions to layout rects
        // Since LayoutRect only supports single-line rectangles, we need to split multi-line regions
        for region in merged_regions {
            for line in region.start_line..=region.end_line {
                rects.push(LayoutRect::new(
                    AlacPoint::new(line, region.start_col),
                    (region.end_col - region.start_col + 1) as usize,
                    region.color,
                ));
            }
        }

        let layout_time = start_time.elapsed();
        log::debug!(
            "Terminal layout_grid: {} cells processed, {} batched runs created, {} rects (from {} merged regions), layout took {:?}",
            cell_count,
            batched_runs.len(),
            rects.len(),
            region_count,
            layout_time
        );

        (rects, batched_runs)
    }

    /// Converts the Alacritty cell styles to GPUI text styles and background color.
    fn cell_style(
        indexed: &IndexedCell,
        fg: alacritty_terminal::vte::ansi::Color,
        bg: alacritty_terminal::vte::ansi::Color,
        colors: &Palette,
        text_style: &TextStyle,
        hyperlink: Option<(HighlightStyle, &RangeInclusive<AlacPoint>)>,
        _minimum_contrast: f32,
    ) -> TextRun {
        let flags = indexed.cell.flags;
        let mut fg = colors.color(fg);
        let _bg = colors.color(bg);

        // Ghostty uses (175/255) as the multiplier (~0.69), Alacritty uses 0.66, Kitty
        // uses 0.75. We're using 0.7 because it's pretty well in the middle of that.
        if flags.intersects(Flags::DIM) {
            fg.a *= 0.7;
        }

        let underline = (flags.intersects(Flags::ALL_UNDERLINES)
            || indexed.cell.hyperlink().is_some())
        .then(|| UnderlineStyle {
            color: Some(fg),
            thickness: Pixels::from(1.0),
            wavy: flags.contains(Flags::UNDERCURL),
        });

        let strikethrough = flags
            .intersects(Flags::STRIKEOUT)
            .then(|| StrikethroughStyle {
                color: Some(fg),
                thickness: Pixels::from(1.0),
            });

        let weight = if flags.intersects(Flags::BOLD) {
            FontWeight::BOLD
        } else {
            text_style.font_weight
        };

        let style = if flags.intersects(Flags::ITALIC) {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };

        let mut result = TextRun {
            len: indexed.c.len_utf8(),
            color: fg,
            background_color: None,
            font: Font {
                weight,
                style,
                ..text_style.font()
            },
            underline,
            strikethrough,
        };

        if let Some((style, range)) = hyperlink
            && range.contains(&indexed.point)
        {
            if let Some(underline) = style.underline {
                result.underline = Some(underline);
            }

            if let Some(color) = style.color {
                result.color = color;
            }
        }

        result
    }
}
pub fn is_blank(cell: &IndexedCell) -> bool {
    if cell.c != ' ' {
        return false;
    }

    if cell.bg != AnsiColor::Named(NamedColor::Background) {
        return false;
    }

    if cell.hyperlink().is_some() {
        return false;
    }

    if cell
        .flags
        .intersects(Flags::ALL_UNDERLINES | Flags::INVERSE | Flags::STRIKEOUT)
    {
        return false;
    }

    true
}
