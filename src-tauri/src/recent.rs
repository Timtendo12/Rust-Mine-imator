//! Recent projects, shown on the startup screen.
//!
//! The list lives in the application's own data folder in the original's
//! `recent.midata` format. The first time, it is seeded from an existing
//! installation of the original program so that its projects show up.

use mi_format::recent::{gm_date_from_unix, RecentList, RecentProject};
use mi_project::Project;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_NAME: &str = "recent.midata";
/// Largest thumbnail that is sent to the frontend.
const MAX_THUMBNAIL_BYTES: u64 = 2 * 1024 * 1024;

/// Where the original program keeps its recent list: next to the executable
/// in `Data/` on Windows (installed to the home folder by default), in
/// `~/Mine-imator/` elsewhere.
fn original_locations(home: &Path) -> [PathBuf; 2] {
    [home.join("Mine-imator").join("Data").join(FILE_NAME), home.join("Mine-imator").join(FILE_NAME)]
}

/// Loads the list from `data_dir`, or from an installation of the original
/// under `home` if this program has none yet. Unreadable files give an
/// empty list: the list is a convenience and must not stop the program.
pub fn load(data_dir: &Path, home: Option<&Path>) -> RecentList {
    let own = data_dir.join(FILE_NAME);
    let candidates = std::iter::once(own).chain(home.into_iter().flat_map(original_locations));
    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            match RecentList::load(&bytes) {
                Ok(list) => return list,
                Err(error) => eprintln!("Ignoring {}: {error}", path.display()),
            }
        }
    }
    RecentList::default()
}

pub fn save(data_dir: &Path, list: &RecentList) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(data_dir.join(FILE_NAME), list.save())
}

/// Seconds since the Unix epoch on the local clock, which is what the
/// original stores (shifted to its own date origin).
fn local_now() -> f64 {
    // Without a time zone database only UTC is available; the difference
    // affects the displayed time of day, not the order of the list.
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// The entry for a project that has just been opened or saved.
pub fn entry_for(project: &Project) -> Option<RecentProject> {
    let path = project.path()?;
    let info = &project.file().info;
    Some(RecentProject {
        name: info.name.clone(),
        author: info.author.clone(),
        description: info.description.clone(),
        filename: path.to_string_lossy().replace('\\', "/"),
        last_opened: gm_date_from_unix(local_now()),
        pinned: false,
    })
}

/// A recent project as the startup screen shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentItem {
    name: String,
    author: String,
    description: String,
    filename: String,
    /// Milliseconds since the Unix epoch, as local wall-clock time.
    last_opened: Option<f64>,
    pinned: bool,
    /// Whether the project file still exists.
    exists: bool,
    /// `data:` URL of `thumbnail.png` in the project folder.
    thumbnail: Option<String>,
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn thumbnail(project_file: &Path) -> Option<String> {
    let path = project_file.parent()?.join("thumbnail.png");
    let size = std::fs::metadata(&path).ok()?.len();
    if size == 0 || size > MAX_THUMBNAIL_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    // Only hand real PNG files to the page.
    bytes.starts_with(b"\x89PNG\r\n\x1a\n").then(|| format!("data:image/png;base64,{}", base64(&bytes)))
}

pub fn items(list: &RecentList) -> Vec<RecentItem> {
    list.projects
        .iter()
        .map(|project| {
            let path = Path::new(&project.filename);
            RecentItem {
                name: project.name.clone(),
                author: project.author.clone(),
                description: project.description.clone(),
                filename: project.filename.clone(),
                last_opened: project.last_opened_unix().map(|s| s * 1000.0),
                pinned: project.pinned,
                exists: path.is_file(),
                thumbnail: thumbnail(path),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mi-recent-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(name: &str, filename: &str) -> RecentProject {
        RecentProject {
            name: name.to_owned(),
            author: String::new(),
            description: String::new(),
            filename: filename.to_owned(),
            last_opened: 46000.0,
            pinned: false,
        }
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn own_list_wins_and_the_original_seeds_it() {
        let data = temp_dir("data");
        let home = temp_dir("home");
        assert!(load(&data, Some(&home)).projects.is_empty());

        // An installation of the original.
        let original = home.join("Mine-imator").join("Data");
        std::fs::create_dir_all(&original).unwrap();
        let mut theirs = RecentList::default();
        theirs.add(entry("Theirs", "C:/x/theirs.miproject"));
        std::fs::write(original.join(FILE_NAME), theirs.save()).unwrap();
        assert_eq!(load(&data, Some(&home)).projects[0].name, "Theirs");

        // Once this program has saved its own list, that one is used.
        let mut ours = RecentList::default();
        ours.add(entry("Ours", "C:/x/ours.miproject"));
        save(&data, &ours).unwrap();
        assert_eq!(load(&data, Some(&home)).projects[0].name, "Ours");

        // A corrupt own file falls back instead of failing.
        std::fs::write(data.join(FILE_NAME), "nonsense").unwrap();
        assert_eq!(load(&data, Some(&home)).projects[0].name, "Theirs");

        std::fs::remove_dir_all(&data).unwrap();
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn items_report_missing_files_and_thumbnails() {
        let dir = temp_dir("items");
        let project = dir.join("scene.miproject");
        std::fs::write(&project, "{}").unwrap();
        let mut list = RecentList::default();
        list.add(entry("Gone", "C:/does/not/exist.miproject"));
        list.add(entry("Here", &project.to_string_lossy()));

        let found = items(&list);
        assert!(found[0].exists && !found[1].exists);
        assert!(found[0].thumbnail.is_none());
        assert_eq!(found[0].last_opened, Some((46000.0 - 25569.0) * 86400.0 * 1000.0));

        // Only real PNG files become thumbnails.
        std::fs::write(dir.join("thumbnail.png"), "not a png").unwrap();
        assert!(items(&list)[0].thumbnail.is_none());
        std::fs::write(dir.join("thumbnail.png"), b"\x89PNG\r\n\x1a\nrest").unwrap();
        let url = items(&list)[0].thumbnail.clone().unwrap();
        assert!(url.starts_with("data:image/png;base64,iVBORw0KGg"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
