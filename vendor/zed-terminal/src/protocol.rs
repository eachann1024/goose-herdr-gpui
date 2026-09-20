//! Bounded extensions to the existing ANSI parser, on the same PTY read thread.
use alacritty_terminal::{
    Term,
    event::{Event, EventListener},
    term::TermMode,
    vte::ansi::Processor,
};
use std::sync::{Arc, Mutex};

use crate::{ZedListener, images::ImageStore};

fn advance_ansi(
    parser: &mut Processor,
    term: &mut Term<ZedListener>,
    bytes: &[u8],
    images: &Mutex<ImageStore>,
) {
    let alternate = term.mode().contains(TermMode::ALT_SCREEN);
    parser.advance(term, bytes);
    if alternate && !term.mode().contains(TermMode::ALT_SCREEN) {
        images
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear_alternate();
    }
}

const MAX_GRAPHICS_SEQUENCE: usize = 28 * 1024 * 1024;

#[derive(Default)]
enum State {
    #[default]
    Ground,
    Escape,
    Csi,
    Apc,
    Kitty {
        escaped: bool,
        discard: bool,
    },
    String {
        osc: bool,
        escaped: bool,
    },
    CsiPassthrough,
}

pub(crate) struct Protocol {
    state: State,
    pending: Vec<u8>,
    light: bool,
    subscribed: bool,
}

impl Protocol {
    pub fn new(light: bool) -> Self {
        Self {
            state: State::Ground,
            pending: Vec::new(),
            light,
            subscribed: false,
        }
    }

    fn report_theme(&self, listener: &ZedListener) {
        listener.send_event(Event::PtyWrite(format!(
            "\x1b[?997;{}n",
            if self.light { 2 } else { 1 }
        )));
    }

    pub fn set_theme(&mut self, light: bool, force: bool, listener: &ZedListener) {
        let changed = self.light != light;
        self.light = light;
        // Never inject an unsolicited terminal reply into a shell/readline session.
        if self.subscribed && (changed || force) {
            self.report_theme(listener);
        }
    }

    fn csi(&mut self, listener: &ZedListener) -> bool {
        match self.pending.as_slice() {
            b"\x1b[?996n" => self.report_theme(listener),
            b"\x1b[?2031h" => {
                self.subscribed = true;
                self.report_theme(listener);
            }
            b"\x1b[?2031l" => self.subscribed = false,
            b"\x1b[?2031$p" => listener.send_event(Event::PtyWrite(format!(
                "\x1b[?2031;{}$y",
                if self.subscribed { 1 } else { 2 }
            ))),
            b"\x1b[16t" => listener.send_event(Event::TextAreaSizeRequest(Arc::new(|size| {
                format!("\x1b[6;{};{}t", size.cell_height, size.cell_width)
            }))),
            _ => return false,
        }
        true
    }

    pub fn advance(
        &mut self,
        parser: &mut Processor,
        term: &mut Term<ZedListener>,
        bytes: &[u8],
        listener: &ZedListener,
        images: &Mutex<ImageStore>,
    ) {
        let mut plain = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            let state = std::mem::take(&mut self.state);
            match state {
                State::Ground => {
                    if byte == 0x1b {
                        self.pending.push(byte);
                        self.state = State::Escape;
                    } else {
                        plain.push(byte);
                    }
                }
                State::Escape => {
                    self.pending.push(byte);
                    match byte {
                        b'[' => self.state = State::Csi,
                        b'_' => self.state = State::Apc,
                        b']' | b'P' | b'^' | b'X' => {
                            plain.append(&mut self.pending);
                            self.state = State::String {
                                osc: byte == b']',
                                escaped: false,
                            };
                        }
                        0x1b => {
                            plain.push(0x1b);
                            self.pending.truncate(1);
                            self.state = State::Escape;
                        }
                        _ => {
                            if byte == b'c' {
                                self.subscribed = false;
                                images.lock().unwrap_or_else(|e| e.into_inner()).reset();
                            }
                            plain.append(&mut self.pending);
                        }
                    }
                }
                State::Csi => {
                    self.pending.push(byte);
                    if (0x40..=0x7e).contains(&byte) {
                        advance_ansi(parser, term, &plain, images);
                        plain.clear();
                        if !self.csi(listener) {
                            plain.append(&mut self.pending);
                        }
                        self.pending.clear();
                    } else if byte == 0x1b {
                        // An ESC cancels the unfinished CSI and starts a new sequence.
                        plain.extend_from_slice(&self.pending[..self.pending.len() - 1]);
                        self.pending.clear();
                        self.pending.push(byte);
                        self.state = State::Escape;
                    } else if byte == 0x18 || byte == 0x1a {
                        plain.append(&mut self.pending);
                    } else if self.pending.len() > 1024 {
                        plain.append(&mut self.pending);
                        self.state = State::CsiPassthrough;
                    } else {
                        self.state = State::Csi;
                    }
                }
                State::CsiPassthrough => {
                    plain.push(byte);
                    if !(0x40..=0x7e).contains(&byte) && byte != 0x18 && byte != 0x1a {
                        self.state = State::CsiPassthrough;
                    }
                }
                State::Apc => {
                    self.pending.push(byte);
                    if byte == b'G' {
                        advance_ansi(parser, term, &plain, images);
                        plain.clear();
                        self.state = State::Kitty {
                            escaped: false,
                            discard: false,
                        };
                    } else {
                        plain.append(&mut self.pending);
                        self.state = State::String {
                            osc: false,
                            escaped: byte == 0x1b,
                        };
                    }
                }
                State::Kitty {
                    escaped,
                    mut discard,
                } => {
                    if byte == 0x18 || byte == 0x1a {
                        self.pending.clear();
                        continue;
                    }
                    if !discard {
                        self.pending.push(byte);
                        if self.pending.len() > MAX_GRAPHICS_SEQUENCE {
                            self.pending = Vec::new();
                            discard = true;
                            log::warn!("Discarded oversized terminal graphics sequence");
                        }
                    }
                    if escaped && byte == b'\\' {
                        if !discard {
                            // Alacritty buffers ANSI during synchronized output. Apply the
                            // preceding cursor moves before placing graphics, then resume the
                            // frame's redraw suppression until ESU (or its existing timeout).
                            let synchronized = parser.sync_timeout().sync_timeout().is_some();
                            if synchronized {
                                parser.stop_sync(term);
                            }
                            let response = images
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .handle(&self.pending[3..self.pending.len() - 2], term);
                            if synchronized {
                                parser.advance(term, b"\x1b[?2026h");
                            }
                            if let Ok(response) = String::from_utf8(response) {
                                if !response.is_empty() {
                                    listener.send_event(Event::PtyWrite(response));
                                }
                            }
                        }
                        self.pending = Vec::new();
                    } else {
                        self.state = State::Kitty {
                            escaped: byte == 0x1b,
                            discard,
                        };
                    }
                }
                State::String { osc, escaped } => {
                    // An OSC/DCS payload is opaque: do not interpret nested theme/graphics bytes.
                    plain.push(byte);
                    if !(escaped && byte == b'\\')
                        && !(osc && byte == 7)
                        && byte != 0x18
                        && byte != 0x1a
                    {
                        self.state = State::String {
                            osc,
                            escaped: byte == 0x1b,
                        };
                    }
                }
            }
        }
        advance_ansi(parser, term, &plain, images);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerminalBounds;
    use alacritty_terminal::term::Config;
    use futures::channel::mpsc::unbounded;

    #[test]
    fn graphics_in_synchronized_frames_use_the_preceding_cursor_position() {
        let bytes = b"\x1b[?2026h\x1b[4;7H\x1b_Ga=T,f=32,s=1,v=1,i=91,c=1,r=1,C=1,q=2;/wAA/w==\x1b\\\x1b[1;1H\x1b[?2026l";
        for split in 0..=bytes.len() {
            let (tx, _) = unbounded();
            let listener = ZedListener(tx);
            let mut term = Term::new(
                Config::default(),
                &TerminalBounds::default(),
                listener.clone(),
            );
            let mut parser = Processor::new();
            let mut protocol = Protocol::new(false);
            let images = Mutex::new(ImageStore::default());
            protocol.advance(&mut parser, &mut term, &bytes[..split], &listener, &images);
            protocol.advance(&mut parser, &mut term, &bytes[split..], &listener, &images);
            let placements = images.lock().unwrap().placements(&term, 9., 18.);
            assert_eq!(placements.len(), 1, "split={split}");
            assert_eq!((placements[0].line, placements[0].column), (3., 6.));
            assert!(parser.sync_timeout().sync_timeout().is_none());
        }
    }

    #[test]
    fn two_graphics_placements_keep_distinct_rows() {
        let bytes = b"\x1b[1;1H\x1b_Ga=T,f=32,s=1,v=1,i=1,c=1,r=1,C=1,q=2;/wAA/w==\x1b\\\x1b[6;1H\x1b_Ga=T,f=32,s=1,v=1,i=2,c=1,r=1,C=1,q=2;/wAA/w==\x1b\\";
        let (tx, _) = unbounded();
        let listener = ZedListener(tx);
        let mut term = Term::new(
            Config::default(),
            &TerminalBounds::default(),
            listener.clone(),
        );
        let mut parser = Processor::new();
        let mut protocol = Protocol::new(false);
        let images = Mutex::new(ImageStore::default());
        protocol.advance(&mut parser, &mut term, bytes, &listener, &images);
        let mut placements = images.lock().unwrap().placements(&term, 9., 18.);
        placements.sort_by(|a, b| a.line.total_cmp(&b.line));
        assert_eq!(placements.len(), 2);
        assert_eq!((placements[0].line, placements[0].column), (0., 0.));
        assert_eq!((placements[1].line, placements[1].column), (5., 0.));
    }

    #[test]
    fn fragmented_theme_queries_and_subscription_do_not_become_input() {
        let sequence = b"before\x1b[?2031h\x1b[?996nafter";
        for split in 0..=sequence.len() {
            let (tx, mut rx) = unbounded();
            let listener = ZedListener(tx);
            let mut term = Term::new(
                Config::default(),
                &TerminalBounds::default(),
                listener.clone(),
            );
            let mut parser = Processor::new();
            let mut protocol = Protocol::new(false);
            let images = Mutex::new(ImageStore::default());
            protocol.advance(
                &mut parser,
                &mut term,
                &sequence[..split],
                &listener,
                &images,
            );
            protocol.advance(
                &mut parser,
                &mut term,
                &sequence[split..],
                &listener,
                &images,
            );
            protocol.set_theme(true, false, &listener);
            protocol.set_theme(true, true, &listener);
            protocol.advance(&mut parser, &mut term, b"\x1b[?2031l", &listener, &images);
            protocol.set_theme(false, true, &listener);
            let mut replies = Vec::new();
            while let Ok(event) = rx.try_recv() {
                if let Event::PtyWrite(reply) = event {
                    replies.push(reply);
                }
            }
            assert_eq!(
                replies,
                [
                    "\x1b[?997;1n",
                    "\x1b[?997;1n",
                    "\x1b[?997;2n",
                    "\x1b[?997;2n"
                ]
            );
            assert_eq!(term.grid().cursor.point.column.0, 11);
        }
    }

    #[test]
    fn opaque_strings_and_unsubscribed_shells_never_receive_theme_notifications() {
        let (tx, mut rx) = unbounded();
        let listener = ZedListener(tx);
        let mut term = Term::new(
            Config::default(),
            &TerminalBounds::default(),
            listener.clone(),
        );
        let mut parser = Processor::new();
        let mut protocol = Protocol::new(true);
        let images = Mutex::new(ImageStore::default());
        for &byte in b"\x1b]0;title\x1b[?996n\x07\x1bP\x1b[?2031h\x1b\\" {
            protocol.advance(&mut parser, &mut term, &[byte], &listener, &images);
        }
        protocol.set_theme(false, true, &listener);
        while let Ok(event) = rx.try_recv() {
            assert!(!matches!(event, Event::PtyWrite(_)));
        }
    }
}
