//! Host palette adapter; ANSI index conversion retained from Zed's terminal.rs.
// SPDX-License-Identifier: GPL-3.0-or-later
use alacritty_terminal::vte::ansi::Color;
use gpui::{Hsla, rgb};

#[derive(Clone, Copy, Default)]
pub struct Palette {
    pub light: bool,
}
impl Palette {
    pub fn color(&self, color: Color) -> Hsla {
        match color {
            Color::Spec(c) => {
                rgb((u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)).into()
            }
            Color::Indexed(i) => self.index(i as usize),
            Color::Named(i) => self.index(i as usize),
        }
    }
    pub fn index(&self, index: usize) -> Hsla {
        const DARK: [u32; 16] = [
            0x202124, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf,
            0x5c6370, 0xff7a85, 0xb5e890, 0xffd68a, 0x82c2ff, 0xe09dff, 0x7ed4df, 0xf4f4f4,
        ];
        const LIGHT: [u32; 16] = [
            0xf4f4f4, 0xb42318, 0x067647, 0x854d0e, 0x175cd3, 0x7a3e9d, 0x0e7490, 0x344054,
            0x98a2b3, 0x912018, 0x085d3a, 0x713b12, 0x1849a9, 0x5925dc, 0x155e75, 0x101828,
        ];
        let ansi = if self.light { LIGHT } else { DARK };
        let value = match index {
            0..=15 => ansi[index],
            16..=231 => {
                let i = index - 16;
                let r = i / 36;
                let g = i % 36 / 6;
                let b = i % 6;
                let channel = |v| if v == 0 { 0 } else { v * 40 + 55 };
                ((channel(r) << 16) | (channel(g) << 8) | channel(b)) as u32
            }
            232..=255 => ((index - 232) * 10 + 8) as u32 * 0x010101,
            257 => {
                if self.light {
                    0xffffff
                } else {
                    0x151619
                }
            }
            259..=266 => {
                return {
                    let mut c = self.index(index - 259);
                    c.l *= 0.66;
                    c
                };
            }
            _ => {
                if self.light {
                    0x202124
                } else {
                    0xd8d9de
                }
            }
        };
        rgb(value).into()
    }
}
