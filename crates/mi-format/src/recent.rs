//! The recent projects list (`recent.midata`; `recent_load`, `recent_save`,
//! `recent_add`).

use crate::json::{self, Json, JsonWriter};
use crate::FormatError;

/// Days between GameMaker's date origin (30 December 1899) and the Unix
/// epoch.
const UNIX_EPOCH_AS_GM_DATE: f64 = 25569.0;
const SECONDS_PER_DAY: f64 = 86400.0;

/// One recently opened project.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentProject {
    pub name: String,
    pub author: String,
    pub description: String,
    /// Full path of the project file.
    pub filename: String,
    /// When the project was last opened, as a GameMaker date (days since
    /// 30 December 1899, local time); below 1 means never.
    pub last_opened: f64,
    pub pinned: bool,
}

impl RecentProject {
    /// Seconds since the Unix epoch, or `None` if never opened.
    pub fn last_opened_unix(&self) -> Option<f64> {
        (self.last_opened >= 1.0).then_some((self.last_opened - UNIX_EPOCH_AS_GM_DATE) * SECONDS_PER_DAY)
    }
}

/// Converts seconds since the Unix epoch to a GameMaker date.
pub fn gm_date_from_unix(seconds: f64) -> f64 {
    seconds / SECONDS_PER_DAY + UNIX_EPOCH_AS_GM_DATE
}

/// The list, most recently opened first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecentList {
    pub projects: Vec<RecentProject>,
}

impl RecentList {
    pub fn load(bytes: &[u8]) -> Result<Self, FormatError> {
        let root = json::parse(bytes)?;
        let list = root
            .as_object()
            .and_then(|o| o.array("list"))
            .ok_or_else(|| FormatError::Corrupted("missing \"list\"".to_owned()))?;
        let projects = list
            .iter()
            .filter_map(Json::as_object)
            .map(|p| RecentProject {
                name: p.string("name").unwrap_or("").to_owned(),
                author: p.string("author").unwrap_or("").to_owned(),
                description: p.string("description").unwrap_or("").to_owned(),
                filename: p.string("filename").unwrap_or("").to_owned(),
                last_opened: p.real("last_opened").unwrap_or(-1.0),
                pinned: p.flag("pinned").unwrap_or(false),
            })
            .collect();
        Ok(Self { projects })
    }

    pub fn save(&self) -> String {
        let mut w = JsonWriter::new();
        w.object_start(None);
        w.array_start(Some("list"));
        for project in &self.projects {
            w.object_start(None);
            w.var("name", &project.name);
            w.var("author", &project.author);
            w.var("description", &project.description);
            // The original writes the id of the thumbnail texture it had in
            // memory, which means nothing in a file. The key is kept so the
            // layout stays the same.
            w.var("thumbnail", 0.0);
            w.var("filename", &project.filename);
            w.var("last_opened", project.last_opened);
            w.var_bool("pinned", project.pinned);
            w.object_done();
        }
        w.array_done();
        w.object_done();
        w.finish()
    }

    /// Puts a project at the top of the list, replacing an existing entry
    /// for the same file (`recent_add`). A pinned project stays pinned; the
    /// original unpins it on every open, which looks unintended.
    pub fn add(&mut self, mut project: RecentProject) {
        let same_file = |a: &str, b: &str| a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/"));
        if let Some(index) = self.projects.iter().position(|p| same_file(&p.filename, &project.filename)) {
            project.pinned = self.projects.remove(index).pinned;
        }
        self.projects.insert(0, project);
    }

    /// Removes the entry for a file. Returns whether there was one.
    pub fn remove(&mut self, filename: &str) -> bool {
        let before = self.projects.len();
        self.projects.retain(|p| p.filename != filename);
        self.projects.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "{\r\n\t\"list\": [\r\n\t\t{\r\n\t\t\t\"name\": \"Ocarina Of Time\",\r\n\t\t\t\"author\": \"\",\r\n\t\t\t\"description\": \"\",\r\n\t\t\t\"thumbnail\": 0,\r\n\t\t\t\"filename\": \"C:/Projects/Ocarina Of Time/Ocarina Of Time.miproject\",\r\n\t\t\t\"last_opened\": 46297.52265,\r\n\t\t\t\"pinned\": false\r\n\t\t}\r\n\t]\r\n}";

    fn entry(name: &str, filename: &str) -> RecentProject {
        RecentProject {
            name: name.to_owned(),
            author: String::new(),
            description: String::new(),
            filename: filename.to_owned(),
            last_opened: 46000.5,
            pinned: false,
        }
    }

    #[test]
    fn round_trips_the_original_layout() {
        let list = RecentList::load(SAMPLE.as_bytes()).unwrap();
        assert_eq!(list.projects.len(), 1);
        assert_eq!(list.projects[0].name, "Ocarina Of Time");
        assert_eq!(list.save(), SAMPLE);
        assert!(RecentList::load(b"{}").is_err());
    }

    #[test]
    fn dates_convert_to_unix_time() {
        // 46297.52265 is 2 October 2026, 12:32:37 (local time).
        let list = RecentList::load(SAMPLE.as_bytes()).unwrap();
        let unix = list.projects[0].last_opened_unix().unwrap();
        assert!((unix - 1_790_944_357.0).abs() < 1.0, "{unix}");
        assert!((gm_date_from_unix(unix) - 46297.52265).abs() < 1e-9);
        assert!(entry("opened", "x").last_opened_unix().is_some());
        let mut never = entry("never", "x");
        never.last_opened = -1.0;
        assert_eq!(never.last_opened_unix(), None);
    }

    #[test]
    fn adding_moves_to_the_top_and_keeps_pins() {
        let mut list = RecentList::default();
        list.add(entry("A", "C:/p/a.miproject"));
        list.add(entry("B", "C:/p/b.miproject"));
        list.projects[1].pinned = true;
        // Same file with different slashes and case.
        list.add(entry("A renamed", "c:\\p\\A.miproject"));
        let names: Vec<&str> = list.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["A renamed", "B"]);
        assert!(list.projects[0].pinned);

        assert!(list.remove("C:/p/b.miproject"));
        assert!(!list.remove("C:/p/b.miproject"));
        assert_eq!(list.projects.len(), 1);
    }
}
