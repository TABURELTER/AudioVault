//! EPUB metadata extraction (Dublin Core + OPF).

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

use crate::{MediaError, Result};

pub const MAX_EPUB_OPF_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EpubInfo {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub genres: Vec<String>,
    pub year: Option<i32>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    #[serde(skip)]
    pub cover_data: Option<Vec<u8>>,
    pub cover_mime: Option<String>,
}

fn year_from(s: &str) -> Option<i32> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).take(4).collect();
    digits.parse::<i32>().ok().filter(|y| (1000..=2100).contains(y))
}

pub fn parse_epub_file(path: &Path) -> Result<EpubInfo> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| MediaError::Parse(format!("EPUB zip error: {e}")))?;

    // 1. Locate rootfile from META-INF/container.xml
    let opf_path = find_opf_path(&mut archive)?;

    // 2. Read OPF XML content
    let mut opf_entry = archive
        .by_name(&opf_path)
        .map_err(|e| MediaError::Parse(format!("Cannot open OPF file '{opf_path}': {e}")))?;
    if opf_entry.size() > MAX_EPUB_OPF_BYTES {
        return Err(MediaError::Parse(format!("EPUB OPF too large ({} bytes)", opf_entry.size())));
    }
    let mut opf_bytes = Vec::with_capacity(opf_entry.size() as usize);
    opf_entry.read_to_end(&mut opf_bytes)?;
    drop(opf_entry);

    // 3. Parse OPF metadata
    let (mut info, cover_href) = parse_opf(&opf_bytes)?;

    // 4. Extract cover image if referenced
    if let Some(href) = cover_href {
        // Resolve relative to OPF directory
        let opf_dir = opf_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let full_href = if opf_dir.is_empty() {
            href
        } else {
            format!("{opf_dir}/{href}")
        };
        // Normalize any './' or URL-decoding if present
        let clean_href = full_href.replace("../", "").replace("./", "");
        if let Ok(mut cover_entry) = archive.by_name(&clean_href) {
            if cover_entry.size() < 20 * 1024 * 1024 {
                let mut data = Vec::with_capacity(cover_entry.size() as usize);
                if cover_entry.read_to_end(&mut data).is_ok() {
                    let mime = if clean_href.to_ascii_lowercase().ends_with(".png") {
                        "image/png"
                    } else {
                        "image/jpeg"
                    };
                    info.cover_mime = Some(mime.to_string());
                    info.cover_data = Some(data);
                }
            }
        }
    }

    Ok(info)
}

fn find_opf_path<R: std::io::Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Result<String> {
    let mut entry = archive
        .by_name("META-INF/container.xml")
        .map_err(|e| MediaError::Parse(format!("Missing META-INF/container.xml: {e}")))?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml)?;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let local = e.local_name().as_ref().to_ascii_lowercase();
                if local == "rootfile" {
                    for attr in e.attributes().flatten() {
                        let k = attr.key.local_name().as_ref().to_ascii_lowercase();
                        if k == "full-path" {
                            let path = attr
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map(|v| v.into_owned())
                                .unwrap_or_default();
                            if !path.is_empty() {
                                return Ok(path);
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(MediaError::Parse(format!("XML error in container.xml: {e}"))),
            _ => {}
        }
    }
    Err(MediaError::Parse("No rootfile element found in META-INF/container.xml".into()))
}

fn parse_opf(bytes: &[u8]) -> Result<(EpubInfo, Option<String>)> {
    let text = crate::text::decode_xml(bytes);
    let mut reader = Reader::from_str(&text);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = false;

    let mut info = EpubInfo::default();
    let mut manifest_items: HashMap<String, (String, String)> = HashMap::new(); // id -> (href, media-type)
    let mut cover_meta_id: Option<String> = None;
    let mut cover_manifest_href: Option<String> = None;

    let mut stack: Vec<String> = Vec::new();
    let mut text_buf = String::new();

    loop {
        let ev = match reader.read_event() {
            Ok(e) => e,
            Err(e) => return Err(MediaError::Parse(format!("OPF XML error: {e}"))),
        };
        match ev {
            ev @ (Event::Start(_) | Event::Empty(_)) => {
                let empty = matches!(ev, Event::Empty(_));
                let e = match ev {
                    Event::Start(e) | Event::Empty(e) => e,
                    _ => continue,
                };
                let name = e.local_name().as_ref().to_ascii_lowercase();
                text_buf.clear();

                if name == "item" {
                    let mut id = String::new();
                    let mut href = String::new();
                    let mut media_type = String::new();
                    let mut is_cover_prop = false;
                    for a in e.attributes().flatten() {
                        let k = a.key.local_name().as_ref().to_ascii_lowercase();
                        let v = a
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map(|v| v.into_owned())
                            .unwrap_or_default();
                        match k.as_str() {
                            "id" => id = v,
                            "href" => href = v,
                            "media-type" => media_type = v,
                            "properties" if v.contains("cover-image") => is_cover_prop = true,
                            _ => {}
                        }
                    }
                    if is_cover_prop && !href.is_empty() {
                        cover_manifest_href = Some(href.clone());
                    }
                    if !id.is_empty() && !href.is_empty() {
                        manifest_items.insert(id, (href, media_type));
                    }
                } else if name == "meta" {
                    let mut meta_name = String::new();
                    let mut meta_content = String::new();
                    let mut meta_prop = String::new();
                    for a in e.attributes().flatten() {
                        let k = a.key.local_name().as_ref().to_ascii_lowercase();
                        let v = a
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map(|v| v.into_owned())
                            .unwrap_or_default();
                        match k.as_str() {
                            "name" => meta_name = v,
                            "content" => meta_content = v,
                            "property" => meta_prop = v,
                            _ => {}
                        }
                    }
                    if meta_name == "cover" && !meta_content.is_empty() {
                        cover_meta_id = Some(meta_content.clone());
                    } else if meta_name == "calibre:series" && !meta_content.is_empty() {
                        info.series = Some(meta_content);
                    } else if meta_name == "calibre:series_index" && !meta_content.is_empty() {
                        info.series_index = meta_content.parse::<f32>().ok();
                    } else if meta_prop == "belongs-to-collection" && !meta_content.is_empty() {
                        if info.series.is_none() {
                            info.series = Some(meta_content);
                        }
                    } else if meta_prop == "group-position" && !meta_content.is_empty() {
                        if info.series_index.is_none() {
                            info.series_index = meta_content.parse::<f32>().ok();
                        }
                    }
                }
                if !empty {
                    stack.push(name);
                }
            }
            Event::Text(t) => {
                let s = t.xml10_content();
                text_buf.push_str(&s);
            }
            Event::CData(t) => {
                text_buf.push_str(&t.xml10_content());
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                let current = stack.pop();
                let val = text_buf.trim().to_string();
                if let Some(tag) = current {
                    if tag == name {
                        match name.as_str() {
                            "title" if info.title.is_none() && !val.is_empty() => {
                                info.title = Some(val);
                            }
                            "creator" if !val.is_empty() => {
                                info.authors.push(val);
                            }
                            "description" if info.description.is_none() && !val.is_empty() => {
                                info.description = Some(val);
                            }
                            "language" if info.language.is_none() && !val.is_empty() => {
                                info.language = Some(val);
                            }
                            "publisher" if info.publisher.is_none() && !val.is_empty() => {
                                info.publisher = Some(val);
                            }
                            "date" if info.year.is_none() && !val.is_empty() => {
                                info.year = year_from(&val);
                            }
                            "subject" if !val.is_empty() => {
                                info.genres.push(val);
                            }
                            "meta" if !val.is_empty() && info.series.is_none() => {
                                info.series = Some(val);
                            }
                            _ => {}
                        }
                    }
                }
                text_buf.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }

    // Determine cover href: from cover-image property, or from meta cover id, or id="cover"
    let chosen_href = cover_manifest_href
        .or_else(|| {
            cover_meta_id
                .and_then(|id| manifest_items.get(&id).map(|(href, _)| href.clone()))
        })
        .or_else(|| {
            manifest_items
                .get("cover")
                .or_else(|| manifest_items.get("cover-image"))
                .or_else(|| manifest_items.get("coverimage"))
                .map(|(href, _)| href.clone())
        });

    Ok((info, chosen_href))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_opf_metadata() {
        let opf = r#"<?xml version="1.0" encoding="utf-8"?>
        <package xmlns="http://www.idpf.org/2007/opf" version="2.0">
            <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:title>Убивая Еву</dc:title>
                <dc:creator>Люк Дженнингс</dc:creator>
                <dc:description>Шпионский триллер</dc:description>
                <dc:date>2016-01-01</dc:date>
                <dc:language>ru</dc:language>
                <meta name="calibre:series" content="Ева Поластри"/>
                <meta name="calibre:series_index" content="1.5"/>
                <meta name="cover" content="cov"/>
            </metadata>
            <manifest>
                <item id="cov" href="cover.jpg" media-type="image/jpeg"/>
            </manifest>
        </package>"#;

        let (info, cover_href) = parse_opf(opf.as_bytes()).unwrap();
        assert_eq!(info.title.as_deref(), Some("Убивая Еву"));
        assert_eq!(info.authors, vec!["Люк Дженнингс".to_string()]);
        assert_eq!(info.description.as_deref(), Some("Шпионский триллер"));
        assert_eq!(info.year, Some(2016));
        assert_eq!(info.language.as_deref(), Some("ru"));
        assert_eq!(info.series.as_deref(), Some("Ева Поластри"));
        assert_eq!(info.series_index, Some(1.5));
        assert_eq!(cover_href.as_deref(), Some("cover.jpg"));
    }
}
