//! What the terminal sent, turned into keys, one whole sequence at a time.
//!
//! Escape sequences can arrive in pieces; the decoder keeps what it has not
//! understood yet and waits for the rest. A lone escape byte is ambiguous (the
//! start of a sequence, or the Escape key), so it is held until the caller says
//! that nothing more is coming.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    BigLeft,
    BigRight,
    Home,
    End,
    Delete,
    Enter,
    Escape,
    Backspace,
    Tab,
    BackTab,
    Char(char),
    /// control-letter, lower case
    Ctrl(char),
    /// left button went down at this cell (0-based)
    Press(usize, usize),
    Drag(usize, usize),
    Release(usize, usize),
    Wheel(usize, usize, i32),
    FocusIn,
    FocusOut,
    None,
}

/// Decode as many whole keys as `bytes` holds. Returns them and how many bytes were used.
pub fn decode(bytes: &[u8]) -> (Vec<Key>, usize) {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b {
            if i + 1 == bytes.len() {
                break; // maybe the start of a sequence
            }
            if bytes[i + 1] == b'[' || bytes[i + 1] == b'O' {
                let mut j = i + 2;
                while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                    j += 1;
                }
                if j >= bytes.len() {
                    break;
                }
                let body = std::str::from_utf8(&bytes[i + 2..j]).unwrap_or("");
                keys.push(sequence(body, bytes[j] as char));
                i = j + 1;
                continue;
            }
            // escape followed by something else: the Escape key, then that
            keys.push(Key::Escape);
            i += 1;
            continue;
        }
        // one character, possibly several bytes
        let need = match b {
            0x00..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => 1,
        };
        if i + need > bytes.len() {
            break;
        }
        keys.push(match b {
            b'\t' => Key::Tab,
            b'\r' | b'\n' => Key::Enter,
            0x7f | 0x08 => Key::Backspace,
            1..=26 => Key::Ctrl((b - 1 + b'a') as char),
            0x20..=0x7e => Key::Char(b as char),
            _ => std::str::from_utf8(&bytes[i..i + need]).ok().and_then(|s| s.chars().next()).map_or(Key::None, Key::Char),
        });
        i += need;
    }
    (keys, i)
}

fn sequence(body: &str, end: char) -> Key {
    match (body, end) {
        (b, 'M') | (b, 'm') if b.starts_with('<') => {
            let p: Vec<usize> = b[1..].split(';').filter_map(|v| v.parse().ok()).collect();
            if p.len() != 3 {
                return Key::None;
            }
            let (x, y) = (p[1].saturating_sub(1), p[2].saturating_sub(1));
            if end == 'm' && p[0] & 3 == 0 && p[0] & 64 == 0 {
                Key::Release(x, y)
            } else if p[0] & 64 != 0 {
                Key::Wheel(x, y, if p[0] & 1 == 0 { 1 } else { -1 })
            } else if p[0] & 3 == 0 {
                if p[0] & 32 != 0 {
                    Key::Drag(x, y)
                } else {
                    Key::Press(x, y)
                }
            } else {
                Key::None
            }
        }
        ("", 'I') => Key::FocusIn,
        ("", 'O') => Key::FocusOut,
        ("", 'Z') => Key::BackTab,
        ("", 'A') => Key::Up,
        ("", 'B') => Key::Down,
        ("", 'C') => Key::Right,
        ("", 'D') => Key::Left,
        ("1;2C", _) | ("1;5C", 'C') => Key::BigRight,
        ("1;2D", _) | ("1;5D", 'D') => Key::BigLeft,
        (b, 'C') if b.ends_with(";2") || b.ends_with(";5") => Key::BigRight,
        (b, 'D') if b.ends_with(";2") || b.ends_with(";5") => Key::BigLeft,
        ("5", '~') => Key::BigRight,
        ("6", '~') => Key::BigLeft,
        ("", 'H') | ("1", '~') | ("7", '~') => Key::Home,
        ("", 'F') | ("4", '~') | ("8", '~') => Key::End,
        ("3", '~') => Key::Delete,
        // SS3 keys: F1 is help
        ("", 'P') => Key::Char('?'),
        _ => Key::None,
    }
}

#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
}

impl Decoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Key> {
        self.pending.extend_from_slice(bytes);
        let (keys, used) = decode(&self.pending);
        self.pending.drain(..used);
        if self.pending.len() > 256 {
            self.pending.clear();
        }
        keys
    }

    /// Nothing more is coming: a held escape byte was the Escape key.
    pub fn flush(&mut self) -> Vec<Key> {
        if self.pending == [0x1b] {
            self.pending.clear();
            return vec![Key::Escape];
        }
        Vec::new()
    }

    /// Is a lone escape waiting to find out what it is?
    pub fn holding_escape(&self) -> bool {
        self.pending == [0x1b]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Vec<Key> {
        decode(bytes).0
    }

    #[test]
    fn mouse_parser_distinguishes_press_drag_release_and_wheel() {
        assert_eq!(parse(b"\x1b[<0;20;8M"), vec![Key::Press(19, 7)]);
        assert_eq!(parse(b"\x1b[<32;24;8M"), vec![Key::Drag(23, 7)]);
        assert_eq!(parse(b"\x1b[<0;24;8m"), vec![Key::Release(23, 7)]);
        assert_eq!(parse(b"\x1b[<64;24;8M"), vec![Key::Wheel(23, 7, 1)]);
        assert_eq!(parse(b"\x1b[<65;24;8M"), vec![Key::Wheel(23, 7, -1)]);
        // other buttons do nothing
        assert_eq!(parse(b"\x1b[<2;24;8M"), vec![Key::None]);
        assert_eq!(parse(b"2\t\x1b[Zp"), vec![Key::Char('2'), Key::Tab, Key::BackTab, Key::Char('p')]);
    }

    #[test]
    fn partial_escape_sequences_survive_terminal_reads() {
        let mut decoder = Decoder::default();
        assert!(decoder.feed(b"\x1b[<0;2").is_empty());
        assert_eq!(decoder.feed(b"0;8M\x1b["), vec![Key::Press(19, 7)]);
        assert_eq!(decoder.feed(b"C"), vec![Key::Right]);
        // a multi-byte character split across reads
        assert!(decoder.feed(&"é".as_bytes()[..1]).is_empty());
        assert_eq!(decoder.feed(&"é".as_bytes()[1..]), vec![Key::Char('é')]);
    }

    #[test]
    fn a_lone_escape_is_the_escape_key_once_nothing_follows() {
        let mut decoder = Decoder::default();
        assert!(decoder.feed(b"\x1b").is_empty());
        assert!(decoder.holding_escape());
        assert_eq!(decoder.flush(), vec![Key::Escape]);
        assert!(!decoder.holding_escape());
        // an escape in front of a plain key is both
        assert_eq!(decoder.feed(b"\x1bq"), vec![Key::Escape, Key::Char('q')]);
        // and it is not mistaken for the start of a mouse report that never completes
        assert!(decoder.feed(b"\x1b[<0;1").is_empty());
        assert!(!decoder.holding_escape());
    }

    #[test]
    fn text_and_editing_keys() {
        assert_eq!(parse(b"21:30\r"), vec![Key::Char('2'), Key::Char('1'), Key::Char(':'), Key::Char('3'), Key::Char('0'), Key::Enter]);
        assert_eq!(parse(b"\x7f\x13\x03"), vec![Key::Backspace, Key::Ctrl('s'), Key::Ctrl('c')]);
        assert_eq!(parse(b"\x1b[3~\x1b[H\x1b[F\x1b[I\x1b[O"), vec![Key::Delete, Key::Home, Key::End, Key::FocusIn, Key::FocusOut]);
    }

    #[test]
    fn garbage_never_grows_without_bound() {
        let mut decoder = Decoder::default();
        for _ in 0..100 {
            decoder.feed(b"\x1b[<0;1;1;1;1;1;1;1;");
        }
        assert!(decoder.feed(b"") .is_empty());
    }
}
