//! Storage optimization candidates (recommendations only; never auto-applied).

use audiovault_core::optimize::{ConversionKind, Recommendation, TargetFormat};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{OptionalExt, Result, now_ts};

/// Replace open recommendations of a copy (queued/done/dismissed ones are kept).
pub fn replace_for_copy(conn: &Connection, copy_id: i64, recs: &[Recommendation]) -> Result<()> {
    conn.execute("DELETE FROM optimization_candidate WHERE copy_id = ?1 AND state = 'open'", [copy_id])?;
    let mut st = conn.prepare_cached(
        "INSERT OR IGNORE INTO optimization_candidate (copy_id, kind, target, current_codec, current_bitrate_kbps, target_bitrate_kbps,
            current_size, estimated_size, reencode, preserves_chapters, notes, state, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'open', ?12)",
    )?;
    let now = now_ts();
    for r in recs {
        st.execute(params![
            copy_id,
            r.kind.as_str(),
            r.target.as_str(),
            r.current_codec,
            r.current_bitrate_kbps,
            r.target_bitrate_kbps,
            r.current_size,
            r.estimated_size,
            r.reencode as i64,
            r.preserves_chapters as i64,
            r.notes,
            now
        ])?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OptimizationRow {
    pub id: i64,
    pub copy_id: i64,
    pub book_id: i64,
    pub title: String,
    pub authors: String,
    pub volume_id: i64,
    pub volume_name: String,
    pub volume_online: bool,
    pub rel_path: String,
    pub kind: ConversionKind,
    pub target: TargetFormat,
    pub current_codec: String,
    pub current_bitrate_kbps: Option<i64>,
    pub target_bitrate_kbps: Option<i64>,
    pub current_size: i64,
    pub estimated_size: i64,
    pub reencode: bool,
    pub preserves_chapters: bool,
    pub notes: String,
    pub state: String,
    pub audio_file_count: i64,
    pub duration_ms: i64,
}

impl OptimizationRow {
    pub fn savings(&self) -> i64 {
        (self.current_size - self.estimated_size).max(0)
    }
}

const SELECT: &str = "SELECT o.id, o.copy_id, c.book_id, b.title, b.authors_display, c.volume_id, s.display_name, s.is_online,
        c.rel_path, o.kind, o.target, o.current_codec, o.current_bitrate_kbps, o.target_bitrate_kbps, o.current_size,
        o.estimated_size, o.reencode, o.preserves_chapters, o.notes, o.state, c.audio_file_count, c.total_duration_ms
     FROM optimization_candidate o JOIN book_copy c ON c.id = o.copy_id JOIN book b ON b.id = c.book_id
     JOIN storage_volume s ON s.id = c.volume_id";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<OptimizationRow> {
    let kind: String = r.get(9)?;
    let target: String = r.get(10)?;
    Ok(OptimizationRow {
        id: r.get(0)?,
        copy_id: r.get(1)?,
        book_id: r.get(2)?,
        title: r.get(3)?,
        authors: r.get(4)?,
        volume_id: r.get(5)?,
        volume_name: r.get(6)?,
        volume_online: r.get::<_, i64>(7)? != 0,
        rel_path: r.get(8)?,
        kind: ConversionKind::parse(&kind).unwrap_or(ConversionKind::LossyToLossy),
        target: TargetFormat::parse(&target).unwrap_or(TargetFormat::M4bAac),
        current_codec: r.get(11)?,
        current_bitrate_kbps: r.get(12)?,
        target_bitrate_kbps: r.get(13)?,
        current_size: r.get(14)?,
        estimated_size: r.get(15)?,
        reencode: r.get::<_, i64>(16)? != 0,
        preserves_chapters: r.get::<_, i64>(17)? != 0,
        notes: r.get(18)?,
        state: r.get(19)?,
        audio_file_count: r.get(20)?,
        duration_ms: r.get(21)?,
    })
}

/// Open recommendations ordered by estimated savings.
pub fn list(conn: &Connection, states: &[&str], volume_id: Option<i64>, offset: i64, limit: i64) -> Result<Vec<OptimizationRow>> {
    let states_json = serde_json::to_string(states)?;
    let mut st = conn.prepare_cached(&format!(
        "{SELECT} WHERE o.state IN (SELECT value FROM json_each(?1)) AND (?2 IS NULL OR c.volume_id = ?2) AND c.state = 'present'
         ORDER BY (o.current_size - o.estimated_size) DESC, o.id LIMIT ?3 OFFSET ?4"
    ))?;
    Ok(st.query_map(params![states_json, volume_id, limit, offset], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<OptimizationRow>> {
    Ok(conn.query_row(&format!("{SELECT} WHERE o.id = ?1"), [id], row).optional()?)
}

pub fn set_state(conn: &Connection, id: i64, state: &str) -> Result<()> {
    conn.execute("UPDATE optimization_candidate SET state = ?2 WHERE id = ?1", params![id, state])?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct OptimizationTotals {
    pub copies: i64,
    pub current_bytes: i64,
    pub estimated_savings: i64,
    pub lossless_savings: i64,
}

/// Totals counting only the best recommendation per copy.
pub fn totals(conn: &Connection) -> Result<OptimizationTotals> {
    Ok(conn.query_row(
        "WITH best AS (
            SELECT o.copy_id, o.kind, o.current_size, MAX(o.current_size - o.estimated_size) AS saving
            FROM optimization_candidate o JOIN book_copy c ON c.id = o.copy_id
            WHERE o.state = 'open' AND c.state = 'present' GROUP BY o.copy_id)
         SELECT COUNT(*), COALESCE(SUM(current_size), 0), COALESCE(SUM(saving), 0),
                COALESCE(SUM(CASE WHEN kind = 'lossless_to_lossy' THEN saving ELSE 0 END), 0) FROM best",
        [],
        |r| Ok(OptimizationTotals { copies: r.get(0)?, current_bytes: r.get(1)?, estimated_savings: r.get(2)?, lossless_savings: r.get(3)? }),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{self, CopyEnumeration};
    use crate::test_util::temp_db;
    use crate::volumes;

    #[test]
    fn replace_and_totals() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let vol = volumes::insert(&conn, &volumes::ObservedVolume::synthetic("V", "/v"), "V").unwrap();
        let book = catalog::create_book(&conn, "Книга", &[]).unwrap();
        let var = catalog::ensure_variant(&conn, book, &[]).unwrap();
        let copy = catalog::insert_copy(
            &conn,
            var,
            book,
            &CopyEnumeration { volume_id: vol, rel_path: "Книга".into(), ..Default::default() },
        )
        .unwrap();
        let rec = Recommendation {
            kind: ConversionKind::LosslessToLossy,
            target: TargetFormat::M4bAac,
            current_codec: "flac".into(),
            current_bitrate_kbps: Some(700),
            target_bitrate_kbps: Some(64),
            current_size: 1_000,
            estimated_size: 100,
            reencode: true,
            preserves_chapters: true,
            notes: "n".into(),
        };
        let opus = Recommendation { target: TargetFormat::Opus, estimated_size: 60, ..rec.clone() };
        replace_for_copy(&conn, copy, &[rec.clone(), opus]).unwrap();
        replace_for_copy(&conn, copy, &[rec]).unwrap();
        let rows = list(&conn, &["open"], None, 0, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].savings(), 900);
        let t = totals(&conn).unwrap();
        assert_eq!((t.copies, t.estimated_savings, t.lossless_savings), (1, 900, 900));
        set_state(&conn, rows[0].id, "dismissed").unwrap();
        assert_eq!(totals(&conn).unwrap().copies, 0);
    }
}
