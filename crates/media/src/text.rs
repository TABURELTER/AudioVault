//! Text decoding helpers for untrusted metadata (mixed encodings are common
//! in Russian audiobook releases: cp1251 inside ID3v1/Latin-1 frames, cp866
//! in old NFO files, UTF-16 with BOM, …).

use encoding_rs::{Encoding, IBM866, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1251};

/// Repair cp1251 text that was decoded as Latin-1 ("Ìåòðî 2033" → "Метро 2033").
/// Returns the input unchanged when it does not look like such mojibake.
pub fn repair_mojibake(s: &str) -> String {
    if s.is_empty() || s.chars().any(|c| c as u32 > 0xFF) {
        return s.to_string();
    }
    let high = s.chars().filter(|c| (0xC0..=0xFF).contains(&(*c as u32))).count();
    let letters = s.chars().filter(|c| c.is_alphabetic()).count();
    // Require that most letters are in the Latin-1 upper half (Russian text in cp1251
    // maps entirely to 0xC0..0xFF); genuine Western text has few such letters.
    if high < 2 || high * 10 < letters * 6 {
        return s.to_string();
    }
    let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
    let (decoded, _, had_errors) = WINDOWS_1251.decode(&bytes);
    if had_errors { s.to_string() } else { decoded.into_owned() }
}

/// Normalize a tag value: trim, drop NULs/control chars, repair mojibake.
pub fn clean_tag(s: &str) -> Option<String> {
    let cleaned: String = s.chars().filter(|c| !c.is_control() || *c == '\n').collect();
    let cleaned = repair_mojibake(cleaned.trim());
    let t = cleaned.trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

/// Decode bytes of a small text file (NFO/TXT/CUE) with BOM sniffing and a
/// UTF-8 → cp1251 → cp866 heuristic.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some((enc, bom_len)) = Encoding::for_bom(bytes) {
        return enc.decode_without_bom_handling(&bytes[bom_len..]).0.into_owned();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // Heuristic UTF-16 without BOM: many zero bytes at odd/even positions.
    if bytes.len() >= 4 {
        let zeros_odd = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
        let zeros_even = bytes.iter().step_by(2).filter(|b| **b == 0).count();
        if zeros_odd * 3 > bytes.len() {
            return UTF_16LE.decode_without_bom_handling(bytes).0.into_owned();
        }
        if zeros_even * 3 > bytes.len() {
            return UTF_16BE.decode_without_bom_handling(bytes).0.into_owned();
        }
    }
    let cp1251 = WINDOWS_1251.decode_without_bom_handling(bytes).0;
    let cp866 = IBM866.decode_without_bom_handling(bytes).0;
    if cyrillic_quality(&cp866) > cyrillic_quality(&cp1251) { cp866.into_owned() } else { cp1251.into_owned() }
}

/// Score how "natural" decoded Russian text looks: lowercase Cyrillic letters
/// dominate in real text, box-drawing characters indicate a wrong code page.
fn cyrillic_quality(s: &str) -> i64 {
    let mut score = 0i64;
    for c in s.chars() {
        match c {
            'а'..='я' | 'ё' => score += 2,
            'А'..='Я' | 'Ё' => score += 1,
            '\u{2500}'..='\u{259F}' => score -= 3,
            _ => {}
        }
    }
    score
}

/// Decode an XML document whose encoding is declared in its prolog
/// (`<?xml version="1.0" encoding="windows-1251"?>`), defaulting to UTF-8.
pub fn decode_xml(bytes: &[u8]) -> String {
    if let Some((enc, bom_len)) = Encoding::for_bom(bytes) {
        return enc.decode_without_bom_handling(&bytes[bom_len..]).0.into_owned();
    }
    let head_len = bytes.len().min(256);
    let head = String::from_utf8_lossy(&bytes[..head_len]).to_ascii_lowercase();
    let declared = head.find("encoding=").and_then(|i| {
        let rest = &head[i + 9..];
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let end = rest[1..].find(quote)?;
        Some(rest[1..1 + end].to_string())
    });
    let enc = declared.and_then(|label| Encoding::for_label(label.trim().as_bytes())).unwrap_or(UTF_8);
    if enc == UTF_8 && std::str::from_utf8(bytes).is_err() {
        // Mislabelled file; fall back to the heuristic decoder.
        return decode_text(bytes);
    }
    enc.decode_without_bom_handling(bytes).0.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mojibake_repaired() {
        let (bytes, _, _) = WINDOWS_1251.encode("Метро 2033");
        let latin1: String = bytes.iter().map(|b| *b as char).collect();
        assert_eq!(repair_mojibake(&latin1), "Метро 2033");
        assert_eq!(repair_mojibake("Café crème"), "Café crème", "western text untouched");
        assert_eq!(repair_mojibake("Метро"), "Метро");
    }

    #[test]
    fn decode_text_variants() {
        let (cp1251, _, _) = WINDOWS_1251.encode("Чтец: Иванов");
        assert_eq!(decode_text(&cp1251), "Чтец: Иванов");
        let (cp866, _, _) = IBM866.encode("Чтец: Иванов");
        assert_eq!(decode_text(&cp866), "Чтец: Иванов");
        assert_eq!(decode_text("utf8 ок".as_bytes()), "utf8 ок");
    }

    #[test]
    fn xml_declared_encoding() {
        let mut doc = b"<?xml version=\"1.0\" encoding=\"windows-1251\"?><a>".to_vec();
        doc.extend_from_slice(&WINDOWS_1251.encode("Книга").0);
        doc.extend_from_slice(b"</a>");
        assert!(decode_xml(&doc).contains("Книга"));
    }
}
