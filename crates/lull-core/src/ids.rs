//! Identifier generation and validation.
//!
//! IDs are a one-letter prefix plus 6 characters from `[a-z0-9]`.
//! Lowercase only because macOS and Windows file systems are case-insensitive.

use std::fmt;

pub const ID_LEN: usize = 6;
const ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdKind {
    Card,
    Lane,
    Board,
    Mirror,
    Trash,
}

impl IdKind {
    pub fn prefix(self) -> char {
        match self {
            IdKind::Card => 'c',
            IdKind::Lane => 'k',
            IdKind::Board => 'b',
            IdKind::Mirror => 'm',
            IdKind::Trash => 't',
        }
    }

    pub fn from_prefix(c: char) -> Option<Self> {
        Some(match c {
            'c' => IdKind::Card,
            'k' => IdKind::Lane,
            'b' => IdKind::Board,
            'm' => IdKind::Mirror,
            't' => IdKind::Trash,
            _ => return None,
        })
    }
}

impl fmt::Display for IdKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.prefix())
    }
}

/// Fill a buffer with random bytes from the OS. Falls back to a time-seeded
/// xorshift only if the OS source is unavailable (never expected in practice).
pub fn random_bytes(buf: &mut [u8]) {
    if getrandom::fill(buf).is_err() {
        let mut x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            | 1;
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
    }
}

/// Random lowercase alphanumeric string of `len` characters (unbiased).
pub fn random_suffix(len: usize) -> String {
    let mut out = String::with_capacity(len);
    let mut buf = [0u8; 32];
    while out.len() < len {
        random_bytes(&mut buf);
        for &b in &buf {
            // 252 = 36 * 7: reject to avoid modulo bias.
            if b < 252 && out.len() < len {
                out.push(ALPHABET[(b % 36) as usize] as char);
            }
        }
    }
    out
}

/// Generate a new ID of the given kind, retrying while `taken` reports a collision.
pub fn new_id(kind: IdKind, taken: impl Fn(&str) -> bool) -> String {
    loop {
        let id = format!("{}{}", kind.prefix(), random_suffix(ID_LEN));
        if !taken(&id) {
            return id;
        }
    }
}

/// True when `s` is a syntactically valid ID of the given kind.
pub fn is_id(s: &str, kind: IdKind) -> bool {
    let b = s.as_bytes();
    b.len() == ID_LEN + 1
        && b[0] == kind.prefix() as u8
        && b[1..].iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// Parse the kind of any valid ID.
pub fn kind_of(s: &str) -> Option<IdKind> {
    let kind = IdKind::from_prefix(s.chars().next()?)?;
    is_id(s, kind).then_some(kind)
}

/// Short random token used for attachment names (`<cardId>.<token>.<ext>`).
pub fn attachment_token() -> String {
    random_suffix(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_ids() {
        for kind in [IdKind::Card, IdKind::Lane, IdKind::Board] {
            for _ in 0..200 {
                let id = new_id(kind, |_| false);
                assert!(is_id(&id, kind), "{id}");
                assert_eq!(kind_of(&id), Some(kind));
            }
        }
    }

    #[test]
    fn retries_on_collision() {
        let calls = std::cell::Cell::new(0);
        let id = new_id(IdKind::Card, |_| {
            calls.set(calls.get() + 1);
            calls.get() < 3
        });
        assert!(is_id(&id, IdKind::Card));
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn rejects_invalid() {
        assert!(!is_id("c12345", IdKind::Card));
        assert!(!is_id("c1234567", IdKind::Card));
        assert!(!is_id("C123456", IdKind::Card));
        assert!(!is_id("k123456", IdKind::Card));
        assert!(!is_id("c12345_", IdKind::Card));
        assert!(is_id("c0a9zz1", IdKind::Card));
        assert_eq!(kind_of("x123456"), None);
    }

    #[test]
    fn suffix_is_roughly_uniform() {
        let mut counts = [0usize; 36];
        let s = random_suffix(36_000);
        for c in s.bytes() {
            let i = ALPHABET.iter().position(|&a| a == c).unwrap();
            counts[i] += 1;
        }
        assert!(counts.iter().all(|&n| n > 700 && n < 1300), "{counts:?}");
    }
}
