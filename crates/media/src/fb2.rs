//! FictionBook 2 (FB2) metadata extraction.
//!
//! Only `<description>` is parsed (title-info, publish-info, document-info)
//! plus the binary referenced by `<coverpage>`. The body is skipped, and the
//! reader is streaming, so multi-megabyte books are cheap to process.

use std::path::Path;

use base64::Engine;
use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

use crate::text::decode_xml;
use crate::{MediaError, Result};

/// Refuse to load absurdly large "FB2" files (untrusted input).
pub const MAX_FB2_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Fb2Info {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub translators: Vec<String>,
    /// (series name, index)
    pub sequences: Vec<(String, Option<f32>)>,
    pub genres: Vec<String>,
    pub annotation: Option<String>,
    pub language: Option<String>,
    pub src_language: Option<String>,
    pub keywords: Option<String>,
    pub publisher: Option<String>,
    pub year: Option<i32>,
    pub isbn: Option<String>,
    pub document_id: Option<String>,
    pub cover_id: Option<String>,
    #[serde(skip)]
    pub cover: Option<Fb2Cover>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fb2Cover {
    pub content_type: String,
    pub data: Vec<u8>,
}

#[derive(Default)]
struct Person {
    first: String,
    middle: String,
    last: String,
    nick: String,
}

impl Person {
    fn display(&self) -> Option<String> {
        let parts: Vec<&str> = [self.first.trim(), self.middle.trim(), self.last.trim()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            let n = self.nick.trim();
            return (!n.is_empty()).then(|| n.to_string());
        }
        Some(parts.join(" "))
    }
}

fn year_from(s: &str) -> Option<i32> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).take(4).collect();
    digits
        .parse::<i32>()
        .ok()
        .filter(|y| (1000..=2100).contains(y))
}

fn entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => return None,
    })
}

pub fn parse_fb2_file(path: &Path) -> Result<Fb2Info> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        return parse_fb2_zip(path);
    }
    let len = std::fs::metadata(path)?.len();
    if len > MAX_FB2_BYTES {
        return Err(MediaError::Parse(format!("FB2 too large ({len} bytes)")));
    }
    parse_fb2(&std::fs::read(path)?)
}

/// Parse FB2 from the first `.fb2` file found inside a ZIP archive.
pub fn parse_fb2_zip(path: &Path) -> Result<Fb2Info> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| MediaError::Parse(format!("ZIP error: {e}")))?;

    let mut target_index = None;
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            if entry.name().to_ascii_lowercase().ends_with(".fb2") {
                target_index = Some(i);
                break;
            }
        }
    }
    let idx = target_index
        .ok_or_else(|| MediaError::Parse("no .fb2 file found inside zip archive".into()))?;
    let mut entry = archive
        .by_index(idx)
        .map_err(|e| MediaError::Parse(format!("ZIP entry error: {e}")))?;
    if entry.size() > MAX_FB2_BYTES {
        return Err(MediaError::Parse(format!(
            "FB2 in zip too large ({} bytes)",
            entry.size()
        )));
    }
    use std::io::Read;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf)?;
    parse_fb2(&buf)
}

/// Fast check whether a ZIP archive contains an `.fb2` file.
pub fn is_fb2_zip(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(mut archive) = zip::ZipArchive::new(std::io::BufReader::new(file)) else {
        return false;
    };
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let lower = entry.name().to_ascii_lowercase();
            if lower.ends_with(".fb2") {
                return true;
            }
        }
    }
    false
}

/// Parse FB2 metadata from raw bytes (encoding taken from the XML prolog).
pub fn parse_fb2(bytes: &[u8]) -> Result<Fb2Info> {
    let text = decode_xml(bytes);
    let mut reader = Reader::from_str(&text);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = false;

    let mut info = Fb2Info::default();
    let mut stack: Vec<String> = Vec::new();
    let mut text_buf = String::new();
    let mut person: Option<Person> = None;
    let mut annotation = String::new();
    let mut binary_id: Option<String> = None;
    let mut binary_type = String::new();
    let mut binary_data = String::new();
    let mut seen_description = false;

    loop {
        let ev = match reader.read_event() {
            Ok(ev) => ev,
            Err(e) => {
                if seen_description {
                    break; // tolerate garbage after the metadata we need
                }
                return Err(MediaError::Parse(e.to_string()));
            }
        };
        match ev {
            ev @ (Event::Start(_) | Event::Empty(_)) => {
                let empty = matches!(ev, Event::Empty(_));
                let e = match ev {
                    Event::Start(e) | Event::Empty(e) => e,
                    _ => continue,
                };
                let name = e.local_name().as_ref().to_ascii_lowercase();
                let in_title_info = stack.iter().any(|s| s == "title-info");
                match name.as_str() {
                    "description" => seen_description = true,
                    "author" | "translator" if in_title_info => person = Some(Person::default()),
                    "sequence" if in_title_info => {
                        let mut sname = None;
                        let mut num = None;
                        for a in e.attributes().flatten() {
                            let key = a.key.local_name().as_ref().to_ascii_lowercase();
                            let val = a
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map(|v| v.into_owned())
                                .unwrap_or_default();
                            match key.as_str() {
                                "name" => sname = Some(val.trim().to_string()),
                                "number" => num = val.trim().replace(',', ".").parse::<f32>().ok(),
                                _ => {}
                            }
                        }
                        if let Some(n) = sname.filter(|s| !s.is_empty()) {
                            info.sequences.push((n, num));
                        }
                    }
                    "image"
                        if stack.iter().any(|s| s == "coverpage") && info.cover_id.is_none() =>
                    {
                        for a in e.attributes().flatten() {
                            if a.key.local_name().as_ref().eq_ignore_ascii_case("href") {
                                let v = a
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map(|v| v.into_owned())
                                    .unwrap_or_default();
                                info.cover_id = Some(v.trim_start_matches('#').to_string());
                            }
                        }
                    }
                    "binary" => {
                        binary_id = None;
                        binary_type.clear();
                        binary_data.clear();
                        for a in e.attributes().flatten() {
                            let key = a.key.local_name().as_ref().to_ascii_lowercase();
                            let val = a
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map(|v| v.into_owned())
                                .unwrap_or_default();
                            match key.as_str() {
                                "id" => binary_id = Some(val),
                                "content-type" => binary_type = val,
                                _ => {}
                            }
                        }
                    }
                    "body" if !empty => {
                        // Skip the book text entirely.
                        let _ = reader.read_to_end(e.name());
                        continue;
                    }
                    _ => {}
                }
                text_buf.clear();
                if !empty {
                    stack.push(name);
                }
            }
            Event::Text(t) => {
                let s = t.xml10_content();
                text_buf.push_str(&s);
                if stack.iter().any(|s| s == "annotation")
                    && stack.iter().any(|s| s == "title-info")
                {
                    annotation.push_str(&s);
                }
                if binary_id.is_some() && stack.last().map(String::as_str) == Some("binary") {
                    binary_data.push_str(&s);
                }
            }
            Event::CData(t) => {
                text_buf.push_str(&t.xml10_content());
            }
            Event::GeneralRef(r) => {
                let c = if r.is_char_ref() {
                    r.resolve_char_ref().ok().flatten()
                } else {
                    entity(&r.xml10_content())
                };
                if let Some(c) = c {
                    text_buf.push(c);
                    if stack.iter().any(|s| s == "annotation")
                        && stack.iter().any(|s| s == "title-info")
                    {
                        annotation.push(c);
                    }
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                // Pop to the matching element (tolerates sloppy nesting).
                while let Some(top) = stack.pop() {
                    if top == name {
                        break;
                    }
                }
                let in_title_info = stack.iter().any(|s| s == "title-info");
                let in_publish = stack.iter().any(|s| s == "publish-info");
                let in_document = stack.iter().any(|s| s == "document-info");
                let value = text_buf.trim().to_string();
                match name.as_str() {
                    "first-name" => {
                        if let Some(p) = person.as_mut() {
                            p.first = value.clone();
                        }
                    }
                    "middle-name" => {
                        if let Some(p) = person.as_mut() {
                            p.middle = value.clone();
                        }
                    }
                    "last-name" => {
                        if let Some(p) = person.as_mut() {
                            p.last = value.clone();
                        }
                    }
                    "nickname" => {
                        if let Some(p) = person.as_mut() {
                            p.nick = value.clone();
                        }
                    }
                    "author" if in_title_info => {
                        if let Some(n) = person.take().and_then(|p| p.display()) {
                            info.authors.push(n);
                        }
                    }
                    "translator" if in_title_info => {
                        if let Some(n) = person.take().and_then(|p| p.display()) {
                            info.translators.push(n);
                        }
                    }
                    "book-title" if in_title_info && !value.is_empty() => {
                        info.title = Some(value.clone())
                    }
                    "genre" if in_title_info && !value.is_empty() => {
                        info.genres.push(value.clone())
                    }
                    "lang" if in_title_info && !value.is_empty() => {
                        info.language = Some(value.to_lowercase())
                    }
                    "src-lang" if in_title_info && !value.is_empty() => {
                        info.src_language = Some(value.to_lowercase())
                    }
                    "keywords" if in_title_info && !value.is_empty() => {
                        info.keywords = Some(value.clone())
                    }
                    "date" if in_title_info && info.year.is_none() => info.year = year_from(&value),
                    "publisher" if in_publish && !value.is_empty() => {
                        info.publisher = Some(value.clone())
                    }
                    "year" if in_publish => {
                        if info.year.is_none() {
                            info.year = year_from(&value);
                        }
                    }
                    "isbn" if in_publish && !value.is_empty() => info.isbn = Some(value.clone()),
                    "id" if in_document && !value.is_empty() => {
                        info.document_id = Some(value.clone())
                    }
                    "annotation" if in_title_info => {
                        let a = annotation.split_whitespace().collect::<Vec<_>>().join(" ");
                        if !a.is_empty() {
                            info.annotation = Some(a);
                        }
                    }
                    "binary" => {
                        if let (Some(id), Some(want)) = (binary_id.take(), info.cover_id.as_ref())
                            && &id == want
                            && info.cover.is_none()
                        {
                            let clean: String =
                                binary_data.chars().filter(|c| !c.is_whitespace()).collect();
                            if let Ok(data) =
                                base64::engine::general_purpose::STANDARD.decode(clean.as_bytes())
                            {
                                info.cover = Some(Fb2Cover {
                                    content_type: binary_type.clone(),
                                    data,
                                });
                            }
                        }
                        binary_data.clear();
                    }
                    "description" if info.cover_id.is_none() => {
                        // No cover referenced: nothing more to read.
                        break;
                    }
                    _ => {}
                }
                if name == "p" && stack.iter().any(|s| s == "annotation") {
                    annotation.push(' ');
                }
                text_buf.clear();
            }
            Event::Eof => break,
            _ => {}
        }
        // Stop once the cover binary was found; nothing else is needed.
        if info.cover.is_some() {
            break;
        }
    }
    if info.title.is_none() && info.authors.is_empty() {
        return Err(MediaError::Parse("no FB2 title-info found".into()));
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding_rs::WINDOWS_1251;

    const SAMPLE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description>
 <title-info>
  <genre>sf_postapocalyptic</genre>
  <author><first-name>Дмитрий</first-name><middle-name>Алексеевич</middle-name><last-name>Глуховский</last-name></author>
  <book-title>Метро 2033</book-title>
  <annotation><p>Мир &amp; война.</p><p>Вторая строка.</p></annotation>
  <date value="2005-01-01">2005</date>
  <coverpage><image l:href="#cover.jpg"/></coverpage>
  <lang>ru</lang>
  <sequence name="Метро" number="1"/>
 </title-info>
 <document-info><id>abc-123</id></document-info>
 <publish-info><publisher>АСТ</publisher><year>2007</year><isbn>978-5</isbn></publish-info>
</description>
<body><section><p>Текст книги, который пропускается…</p></section></body>
<binary id="cover.jpg" content-type="image/jpeg">/9j/4AAQ</binary>
</FictionBook>"##;

    #[test]
    fn parses_title_info() {
        let i = parse_fb2(SAMPLE.as_bytes()).unwrap();
        assert_eq!(i.title.as_deref(), Some("Метро 2033"));
        assert_eq!(i.authors, vec!["Дмитрий Алексеевич Глуховский".to_string()]);
        assert_eq!(i.sequences, vec![("Метро".to_string(), Some(1.0))]);
        assert_eq!(i.genres, vec!["sf_postapocalyptic".to_string()]);
        assert_eq!(i.language.as_deref(), Some("ru"));
        assert_eq!(i.year, Some(2005));
        assert_eq!(i.publisher.as_deref(), Some("АСТ"));
        assert_eq!(i.document_id.as_deref(), Some("abc-123"));
        assert_eq!(i.annotation.as_deref(), Some("Мир & война. Вторая строка."));
        let cover = i.cover.expect("cover decoded");
        assert_eq!(cover.content_type, "image/jpeg");
        assert_eq!(&cover.data[..3], &[0xff, 0xd8, 0xff]);
    }

    #[test]
    fn parses_cp1251() {
        let doc = SAMPLE.replace("UTF-8", "windows-1251");
        let (bytes, _, _) = WINDOWS_1251.encode(&doc);
        let i = parse_fb2(&bytes).unwrap();
        assert_eq!(i.title.as_deref(), Some("Метро 2033"));
    }

    #[test]
    fn garbage_is_error() {
        assert!(parse_fb2(b"not xml at all").is_err());
        assert!(parse_fb2(b"<FictionBook><description><title-info>").is_err());
    }
}
