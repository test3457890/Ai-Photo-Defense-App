//! Media Manager catalog. The database stores file references and organisation data only:
//! paths, hashes, dimensions, camera make/model, capture date, ratings, tags and albums. It
//! never stores pixels. GPS coordinates are not copied into the catalog.
//!
//! The thumbnail cache is OFF by default because cached previews can reveal sensitive content.
//! When enabled it lives in a user-chosen directory and `clear` overwrites files before deleting
//! them (best effort: SSD wear levelling and file-system snapshots can retain old blocks).
use anyhow::{Context, Result, ensure};
use photo_log::{Code, Component};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::path::{Path, PathBuf};

const SCHEMA: i64 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhotoRow {
    pub id: i64,
    pub path: PathBuf,
    pub blake3: String,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub camera: String,
    pub captured: String,
    pub rating: u8,
    pub flagged: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub text: String,
    pub min_rating: u8,
    pub format: Option<String>,
    pub camera: Option<String>,
    pub tag: Option<String>,
    pub album: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub flagged_only: bool,
}

pub struct Catalog {
    db: Connection,
}

fn exif_info(bytes: &[u8]) -> (String, String) {
    let Ok(e) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) else { return (String::new(), String::new()) };
    let get = |t| {
        e.get_field(t, exif::In::PRIMARY)
            .map(|f| f.display_value().to_string().trim_matches('"').chars().filter(|c| !c.is_control()).take(64).collect::<String>())
            .unwrap_or_default()
    };
    let camera = format!("{} {}", get(exif::Tag::Make), get(exif::Tag::Model)).trim().to_string();
    (camera, get(exif::Tag::DateTimeOriginal))
}

impl Catalog {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path).context("Open catalog")?;
        Self::init(db)
    }
    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }
    fn init(db: Connection) -> Result<Self> {
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON;
             CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value INTEGER);
             CREATE TABLE IF NOT EXISTS photos(id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, blake3 TEXT NOT NULL,
               size INTEGER, width INTEGER, height INTEGER, format TEXT, camera TEXT, captured TEXT,
               rating INTEGER NOT NULL DEFAULT 0 CHECK(rating BETWEEN 0 AND 5), flagged INTEGER NOT NULL DEFAULT 0);
             CREATE INDEX IF NOT EXISTS photos_hash ON photos(blake3);
             CREATE TABLE IF NOT EXISTS tags(photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE, tag TEXT NOT NULL, PRIMARY KEY(photo_id, tag));
             CREATE TABLE IF NOT EXISTS albums(id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL);
             CREATE TABLE IF NOT EXISTS album_photos(album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
               photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE, PRIMARY KEY(album_id, photo_id));",
        )?;
        let v: Option<i64> = db.query_row("SELECT value FROM meta WHERE key='schema'", [], |r| r.get(0)).optional()?;
        match v {
            None => {
                db.execute("INSERT INTO meta(key,value) VALUES('schema',?1)", [SCHEMA])?;
            }
            Some(s) => ensure!(s == SCHEMA, "Catalog schema {s} is not supported"),
        }
        Ok(Self { db })
    }

    /// Imports a photo after a full bounded decode (so only readable images are catalogued).
    pub fn add(&self, path: &Path) -> Result<i64> {
        let photo = photo_core::load(path).inspect_err(|e| photo_log::warn(Component::Catalog, Code::DecodeFailed, &format!("{e:#}")))?;
        let (camera, captured) = exif_info(&photo.bytes);
        let format = match photo.raw {
            Some(k) => format!("RAW {k:?}"),
            None => format!("{:?}", photo.format),
        };
        let p = path.to_string_lossy();
        self.db.execute(
            "INSERT INTO photos(path,blake3,size,width,height,format,camera,captured) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(path) DO UPDATE SET blake3=excluded.blake3,size=excluded.size,width=excluded.width,height=excluded.height,format=excluded.format,camera=excluded.camera,captured=excluded.captured",
            params![p, photo.fingerprint.blake3, photo.bytes.len() as i64, photo.pixels.width(), photo.pixels.height(), format, camera, captured],
        )?;
        Ok(self.db.query_row("SELECT id FROM photos WHERE path=?1", [p], |r| r.get(0))?)
    }
    pub fn set_rating(&self, id: i64, rating: u8) -> Result<()> {
        ensure!(rating <= 5, "Rating must be 0..5");
        self.db.execute("UPDATE photos SET rating=?1 WHERE id=?2", params![rating, id])?;
        Ok(())
    }
    pub fn set_flag(&self, id: i64, flagged: bool) -> Result<()> {
        self.db.execute("UPDATE photos SET flagged=?1 WHERE id=?2", params![flagged, id])?;
        Ok(())
    }
    pub fn tag(&self, id: i64, tag: &str) -> Result<()> {
        let t = tag.trim();
        ensure!(!t.is_empty() && t.len() <= 64 && !t.chars().any(char::is_control), "Tags must be 1..64 printable characters");
        self.db.execute("INSERT OR IGNORE INTO tags(photo_id,tag) VALUES(?1,?2)", params![id, t])?;
        Ok(())
    }
    pub fn untag(&self, id: i64, tag: &str) -> Result<()> {
        self.db.execute("DELETE FROM tags WHERE photo_id=?1 AND tag=?2", params![id, tag])?;
        Ok(())
    }
    pub fn album(&self, name: &str) -> Result<i64> {
        let n = name.trim();
        ensure!(!n.is_empty() && n.len() <= 100, "Album names must be 1..100 characters");
        self.db.execute("INSERT OR IGNORE INTO albums(name) VALUES(?1)", [n])?;
        Ok(self.db.query_row("SELECT id FROM albums WHERE name=?1", [n], |r| r.get(0))?)
    }
    pub fn add_to_album(&self, album: i64, photo: i64) -> Result<()> {
        self.db.execute("INSERT OR IGNORE INTO album_photos(album_id,photo_id) VALUES(?1,?2)", params![album, photo])?;
        Ok(())
    }
    pub fn albums(&self) -> Result<Vec<(i64, String, i64)>> {
        let mut s = self.db.prepare("SELECT a.id,a.name,(SELECT COUNT(*) FROM album_photos p WHERE p.album_id=a.id) FROM albums a ORDER BY a.name")?;
        let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        Ok(rows)
    }
    pub fn remove(&self, id: i64) -> Result<()> {
        self.db.execute("DELETE FROM photos WHERE id=?1", [id])?;
        Ok(())
    }
    /// Parameterised query; user text is never interpolated into SQL.
    pub fn query(&self, f: &Filter) -> Result<Vec<PhotoRow>> {
        let like = |s: &str| format!("%{}%", s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let mut s = self.db.prepare(
            "SELECT p.id,p.path,p.blake3,p.size,p.width,p.height,p.format,p.camera,p.captured,p.rating,p.flagged FROM photos p
             WHERE p.rating >= ?1
               AND (?2 = '' OR p.path LIKE ?3 ESCAPE '\\' OR p.camera LIKE ?3 ESCAPE '\\' OR EXISTS(SELECT 1 FROM tags t WHERE t.photo_id=p.id AND t.tag LIKE ?3 ESCAPE '\\'))
               AND (?4 IS NULL OR p.format = ?4)
               AND (?5 IS NULL OR p.camera = ?5)
               AND (?6 IS NULL OR EXISTS(SELECT 1 FROM tags t WHERE t.photo_id=p.id AND t.tag=?6))
               AND (?7 IS NULL OR EXISTS(SELECT 1 FROM album_photos ap JOIN albums a ON a.id=ap.album_id WHERE ap.photo_id=p.id AND a.name=?7))
               AND (?8 IS NULL OR p.captured >= ?8)
               AND (?9 IS NULL OR p.captured <= ?9)
               AND (?10 = 0 OR p.flagged = 1)
             ORDER BY p.captured, p.path LIMIT 10000",
        )?;
        let rows: Vec<PhotoRow> = s
            .query_map(
                params![f.min_rating, f.text, like(&f.text), f.format, f.camera, f.tag, f.album, f.date_from, f.date_to, f.flagged_only],
                |r| {
                    Ok(PhotoRow {
                        id: r.get(0)?,
                        path: PathBuf::from(r.get::<_, String>(1)?),
                        blake3: r.get(2)?,
                        size: r.get::<_, i64>(3)? as u64,
                        width: r.get(4)?,
                        height: r.get(5)?,
                        format: r.get(6)?,
                        camera: r.get(7)?,
                        captured: r.get(8)?,
                        rating: r.get(9)?,
                        flagged: r.get(10)?,
                        tags: vec![],
                    })
                },
            )?
            .collect::<Result<_, _>>()?;
        let mut t = self.db.prepare("SELECT tag FROM tags WHERE photo_id=?1 ORDER BY tag")?;
        rows.into_iter()
            .map(|mut row| {
                row.tags = t.query_map([row.id], |r| r.get(0))?.collect::<Result<_, _>>()?;
                Ok(row)
            })
            .collect()
    }
    /// Groups of identical files (same BLAKE3) with more than one member.
    pub fn duplicates(&self) -> Result<Vec<Vec<PathBuf>>> {
        let mut s = self.db.prepare("SELECT blake3, path FROM photos WHERE blake3 IN (SELECT blake3 FROM photos GROUP BY blake3 HAVING COUNT(*)>1) ORDER BY blake3, path")?;
        let mut out: Vec<(String, Vec<PathBuf>)> = vec![];
        for r in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (h, p) = r?;
            match out.last_mut() {
                Some((lh, v)) if *lh == h => v.push(p.into()),
                _ => out.push((h, vec![p.into()])),
            }
        }
        Ok(out.into_iter().map(|(_, v)| v).collect())
    }
    pub fn missing(&self) -> Result<Vec<(i64, PathBuf)>> {
        Ok(self.query(&Filter::default())?.into_iter().filter(|r| !r.path.is_file()).map(|r| (r.id, r.path)).collect())
    }
    pub fn distinct(&self, column: &str) -> Result<Vec<String>> {
        ensure!(matches!(column, "format" | "camera"), "Unknown column");
        let mut s = self.db.prepare(&format!("SELECT DISTINCT {column} FROM photos WHERE {column} <> '' ORDER BY 1"))?;
        Ok(s.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
    }
}

/// Optional preview cache. Disabled unless a directory is configured.
pub struct ThumbCache {
    dir: Option<PathBuf>,
}
impl ThumbCache {
    pub fn disabled() -> Self {
        Self { dir: None }
    }
    pub fn enabled(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir: Some(dir) })
    }
    pub fn is_enabled(&self) -> bool {
        self.dir.is_some()
    }
    fn file(&self, hash: &str) -> Option<PathBuf> {
        let d = self.dir.as_ref()?;
        hash.chars().all(|c| c.is_ascii_hexdigit()).then(|| d.join(format!("{}.png", &hash[..hash.len().min(64)])))
    }
    pub fn get_or_make(&self, hash: &str, make: impl FnOnce() -> Result<image::DynamicImage>) -> Result<image::DynamicImage> {
        if let Some(f) = self.file(hash)
            && let Ok(b) = std::fs::read(&f)
            && let Ok((img, _)) = photo_core::decode_bounded(&b)
        {
            return Ok(img);
        }
        let img = make()?.thumbnail(256, 256);
        if let Some(f) = self.file(hash) {
            let bytes = photo_core::encode(&img, photo_core::ExportFormat::Png, 90)?;
            let _ = photo_core::save_new(&f, &bytes);
        }
        Ok(img)
    }
    pub fn size_bytes(&self) -> u64 {
        self.dir.as_ref().and_then(|d| std::fs::read_dir(d).ok()).map(|r| r.filter_map(|e| e.ok()?.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0)
    }
    /// Overwrites each cached file with zeros, syncs, then deletes it.
    pub fn clear(&self) -> Result<usize> {
        let Some(d) = &self.dir else { return Ok(0) };
        let mut n = 0;
        for e in std::fs::read_dir(d)?.filter_map(|e| e.ok()) {
            let p = e.path();
            let m = std::fs::symlink_metadata(&p)?;
            if !m.is_file() || p.extension().is_none_or(|x| x != "png") {
                continue;
            }
            if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&p) {
                use std::io::Write;
                let zeros = vec![0u8; 64 * 1024];
                let mut left = m.len();
                while left > 0 {
                    let k = left.min(zeros.len() as u64) as usize;
                    f.write_all(&zeros[..k])?;
                    left -= k as u64;
                }
                f.sync_all()?;
            }
            std::fs::remove_file(&p)?;
            n += 1;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn img(dir: &Path, name: &str, shade: u8) -> PathBuf {
        let p = dir.join(name);
        let i = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(20, 10, image::Rgb([shade; 3])));
        std::fs::write(&p, photo_core::encode(&i, photo_core::ExportFormat::Png, 90).unwrap()).unwrap();
        p
    }
    #[test]
    fn catalog_organisation_filters_and_duplicates() {
        let d = tempfile::tempdir().unwrap();
        let c = Catalog::open(&d.path().join("catalog.db")).unwrap();
        let a = c.add(&img(d.path(), "beach.png", 10)).unwrap();
        let b = c.add(&img(d.path(), "copy.png", 10)).unwrap();
        let x = c.add(&img(d.path(), "city.png", 200)).unwrap();
        assert!(c.add(&d.path().join("missing.png")).is_err());
        c.set_rating(a, 5).unwrap();
        assert!(c.set_rating(a, 9).is_err());
        c.tag(a, "holiday").unwrap();
        c.tag(x, "work").unwrap();
        c.set_flag(x, true).unwrap();
        let alb = c.album("Summer").unwrap();
        c.add_to_album(alb, a).unwrap();
        assert_eq!(c.query(&Filter { min_rating: 4, ..Default::default() }).unwrap().len(), 1);
        assert_eq!(c.query(&Filter { tag: Some("work".into()), ..Default::default() }).unwrap()[0].id, x);
        assert_eq!(c.query(&Filter { album: Some("Summer".into()), ..Default::default() }).unwrap()[0].tags, vec!["holiday"]);
        assert_eq!(c.query(&Filter { text: "beach".into(), ..Default::default() }).unwrap().len(), 1);
        assert_eq!(c.query(&Filter { flagged_only: true, ..Default::default() }).unwrap().len(), 1);
        // SQL metacharacters are data, not syntax.
        assert!(c.query(&Filter { text: "'; DROP TABLE photos; --".into(), ..Default::default() }).unwrap().is_empty());
        assert_eq!(c.query(&Filter::default()).unwrap().len(), 3);
        assert_eq!(c.duplicates().unwrap(), vec![vec![d.path().join("beach.png"), d.path().join("copy.png")]]);
        std::fs::remove_file(d.path().join("copy.png")).unwrap();
        assert_eq!(c.missing().unwrap(), vec![(b, d.path().join("copy.png"))]);
        assert_eq!(c.distinct("format").unwrap(), vec!["Png"]);
        assert!(c.distinct("path; DROP").is_err());
        c.remove(b).unwrap();
        assert_eq!(c.albums().unwrap(), vec![(alb, "Summer".into(), 1)]);
    }
    #[test]
    fn thumbnail_cache_off_by_default_and_secure_clear() {
        assert!(!ThumbCache::disabled().is_enabled());
        let d = tempfile::tempdir().unwrap();
        let cache = ThumbCache::enabled(d.path().join("thumbs")).unwrap();
        let t = cache.get_or_make("abcdef", || Ok(image::DynamicImage::new_rgb8(800, 600))).unwrap();
        assert!(t.width() <= 256);
        assert!(cache.size_bytes() > 0);
        assert_eq!(cache.get_or_make("abcdef", || anyhow::bail!("should hit cache")).unwrap().width(), t.width());
        assert!(cache.get_or_make("../../evil", || Ok(image::DynamicImage::new_rgb8(4, 4))).is_ok());
        assert!(!d.path().join("evil.png").exists());
        assert_eq!(cache.clear().unwrap(), 1);
        assert_eq!(cache.size_bytes(), 0);
    }
}
