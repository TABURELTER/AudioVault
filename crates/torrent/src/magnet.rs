//! Lightweight magnet-link parsing used for validation and display before a
//! link is handed to the engine.

use percent_encoding::percent_decode_str;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagnetInfo {
    /// Lowercase hex SHA-1 info hash (40 chars).
    pub info_hash: String,
    pub display_name: Option<String>,
    pub trackers: Vec<String>,
}

/// Parses a `magnet:?xt=urn:btih:…` link. Accepts hex (40) and base32 (32) hashes.
pub fn parse_magnet(link: &str) -> Option<MagnetInfo> {
    let link = link.trim();
    let query = link
        .strip_prefix("magnet:?")
        .or_else(|| link.strip_prefix("MAGNET:?"))?;
    let mut info_hash = None;
    let mut display_name = None;
    let mut trackers = Vec::new();
    for pair in query.split('&') {
        let (k, v) = match pair.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        let v = percent_decode_str(&v.replace('+', " "))
            .decode_utf8_lossy()
            .into_owned();
        match k {
            "xt" => {
                if let Some(h) = v
                    .strip_prefix("urn:btih:")
                    .or_else(|| v.strip_prefix("urn:BTIH:"))
                {
                    info_hash = normalize_hash(h);
                }
            }
            "dn" => display_name = Some(v),
            "tr" => trackers.push(v),
            _ => {}
        }
    }
    Some(MagnetInfo {
        info_hash: info_hash?,
        display_name,
        trackers,
    })
}

/// Normalizes a hex or base32 info hash to lowercase hex.
pub fn normalize_hash(h: &str) -> Option<String> {
    let h = h.trim();
    if h.len() == 40 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(h.to_ascii_lowercase());
    }
    if h.len() == 32 {
        let bytes = base32_decode(h)?;
        if bytes.len() == 20 {
            return Some(bytes.iter().map(|b| format!("{b:02x}")).collect());
        }
    }
    None
}

fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let mut buf: u64 = 0;
    let mut bits = 0u32;
    for c in s.chars() {
        let v = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => c as u8 - b'A',
            c @ '2'..='7' => c as u8 - b'2' + 26,
            _ => return None,
        };
        buf = (buf << 5) | v as u64;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_magnet() {
        let m = parse_magnet(
            "magnet:?xt=urn:btih:C12FE1C06BBA254A9DC9F519B335AA7C1367A88A&dn=%D0%9C%D0%B5%D1%82%D1%80%D0%BE+2033&tr=udp%3A%2F%2Ftracker.example%3A80",
        )
        .unwrap();
        assert_eq!(m.info_hash, "c12fe1c06bba254a9dc9f519b335aa7c1367a88a");
        assert_eq!(m.display_name.as_deref(), Some("Метро 2033"));
        assert_eq!(m.trackers, vec!["udp://tracker.example:80"]);
    }

    #[test]
    fn parses_base32_magnet() {
        // base32 of 20 zero bytes is 32 'A's.
        let m = parse_magnet("magnet:?xt=urn:btih:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap();
        assert_eq!(m.info_hash, "0".repeat(40));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_magnet("http://example.com").is_none());
        assert!(parse_magnet("magnet:?dn=x").is_none());
        assert!(parse_magnet("magnet:?xt=urn:btih:xyz").is_none());
    }
}
