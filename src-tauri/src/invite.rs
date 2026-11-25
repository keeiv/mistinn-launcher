//! Invites to a friend's room, as the 霧宿 mod writes them: `MIST-` and Crockford base32 in groups
//! of five. The bytes hold a format number, the room key (8), flags (bit 0: the host had
//! `.e4mc.link` cut off), the mod version and the host (each a length byte and UTF-8), the port
//! (2) and the first two bytes of the SHA-256 of everything before them. The launcher only reads
//! where the room is and which mod version it needs; the key stays in the code, which is handed to
//! the game as it is.

use serde::Serialize;
use sha2::{Digest, Sha256};

/// How every invite starts.
const PREFIX: &str = "MIST";
/// The relay's domain, which the code leaves out to stay short.
const RELAY_SUFFIX: &str = ".e4mc.link";
/// Crockford's base32 digits.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// The room key's length in bytes.
const KEY_LENGTH: usize = 8;

/// A friend's room, read from an invite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Invite {
    /// The code without spaces, upper case, as the game takes it.
    pub code: String,
    pub host: String,
    pub port: u16,
    /// The 霧宿 mod version the host runs.
    pub version: String,
}

impl Invite {
    /// `host:port`, with an IPv6 host in brackets.
    pub fn address(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Whether the room is reached through the relay.
    pub fn relay(&self) -> bool {
        self.host.ends_with(RELAY_SUFFIX)
    }
}

/// Reads an invite, or `None` when the text is not one, was cut short or mistyped.
pub fn parse(text: &str) -> Option<Invite> {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    // lookalike letters as the digits they stand for, as the mod reads them
    let body: String = clean
        .strip_prefix(PREFIX)?
        .chars()
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    let bytes = unbase32(&body)?;
    if bytes.len() < 1 + KEY_LENGTH + 1 + 1 + 1 + 2 + 2 || bytes[0] != 1 {
        return None;
    }
    let (data, sum) = bytes.split_at(bytes.len() - 2);
    let digest = Sha256::digest(data);
    if digest[..2] != *sum {
        return None;
    }
    let mut at = 1 + KEY_LENGTH;
    let relay = data[at] & 1 != 0;
    at += 1;
    let version_length = data[at] as usize;
    at += 1;
    let version = std::str::from_utf8(data.get(at..at + version_length)?).ok()?.to_string();
    at += version_length;
    let host_length = *data.get(at)? as usize;
    at += 1;
    if host_length == 0 || at + host_length + 2 != data.len() {
        return None;
    }
    let mut host = std::str::from_utf8(&data[at..at + host_length]).ok()?.to_ascii_lowercase();
    at += host_length;
    let port = u16::from_be_bytes([data[at], data[at + 1]]);
    if relay {
        host.push_str(RELAY_SUFFIX);
    }
    let plain_host = host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    if port == 0 || !plain_host || !version.chars().all(|c| c.is_ascii_graphic()) {
        return None;
    }
    let code = format!("{PREFIX}{body}");
    Some(Invite { code, host, port, version })
}

/// The bytes of a base32 text, or `None` for a character outside the alphabet.
fn unbase32(text: &str) -> Option<Vec<u8>> {
    let mut out = vec![];
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for c in text.chars() {
        let value = u32::try_from(ALPHABET.iter().position(|&a| char::from(a) == c)?).ok()?;
        buffer = (buffer << 5 | value) & 0xFFFF;
        bits += 5;
        if bits >= 8 {
            out.push(u8::try_from((buffer >> (bits - 8)) & 0xFF).ok()?);
            bits -= 8;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mod's own test checks that it writes this very code (`MistinnRoomTest`).
    const EXAMPLE: &str = "MIST-040G4-0R40M-30E20-10MR2-WC9E6-4772X-B9CNT-2TVVM-EHJQ4-BK1E1-HXVJC-Y";

    #[test]
    fn reads_the_mods_example_invite() {
        let invite = parse(EXAMPLE).expect("the example is an invite");
        assert_eq!(invite.host, "quiet-otter.ap.e4mc.link");
        assert_eq!(invite.port, 25565);
        assert_eq!(invite.version, "0.1.1");
        assert!(invite.relay());
        assert_eq!(invite.address(), "quiet-otter.ap.e4mc.link:25565");
        assert_eq!(invite.code, EXAMPLE.replace('-', ""));
    }

    #[test]
    fn spaces_case_and_lookalike_letters_do_not_matter() {
        let messy = EXAMPLE.to_lowercase().replace('-', " ");
        assert_eq!(parse(&messy).map(|i| i.host), Some("quiet-otter.ap.e4mc.link".to_string()));
        assert_eq!(parse(&EXAMPLE.replace('0', "O")), parse(EXAMPLE));
    }

    #[test]
    fn a_typo_or_a_cut_is_caught() {
        let mut typo = EXAMPLE.to_string();
        typo.pop();
        typo.push('Z');
        assert_eq!(parse(&typo), None);
        assert_eq!(parse(&EXAMPLE[..EXAMPLE.len() - 6]), None);
        assert_eq!(parse("MIST-HELLO"), None);
        assert_eq!(parse("example.net:25565"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn a_direct_invite_keeps_its_address() {
        // written by the mod for 127.0.0.1:40000, version 0.1.1, key 1..8
        let invite = parse(&direct_example()).expect("a direct invite");
        assert_eq!(invite.address(), "127.0.0.1:40000");
        assert!(!invite.relay());
    }

    /// Encodes like the mod does, for a direct room.
    fn direct_example() -> String {
        let mut data = vec![1u8, 1, 2, 3, 4, 5, 6, 7, 8, 0];
        data.push(5);
        data.extend_from_slice(b"0.1.1");
        data.push(9);
        data.extend_from_slice(b"127.0.0.1");
        data.extend_from_slice(&40000u16.to_be_bytes());
        let digest = Sha256::digest(&data);
        data.extend_from_slice(&digest[..2]);
        let mut text = String::new();
        let (mut buffer, mut bits) = (0u32, 0);
        for b in data {
            buffer = (buffer << 8 | u32::from(b)) & 0xFFFF;
            bits += 8;
            while bits >= 5 {
                text.push(ALPHABET[((buffer >> (bits - 5)) & 31) as usize] as char);
                bits -= 5;
            }
        }
        if bits > 0 {
            text.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
        }
        format!("MIST-{text}")
    }
}
