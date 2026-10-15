//! Saving settings to `config.toml` without losing anything of the user's.
//!
//! Edits go through `toml_edit`, so comments, ordering, unknown keys and
//! unrelated sections come back out as they went in. A save reads the file
//! again first: if somebody else has changed it since it was opened, nothing
//! is written unless the caller says to overwrite. The new text goes to a
//! temporary file in the same directory and is renamed into place, with the
//! permissions of the old file and a `.bak` of its previous contents.
//!
//! The saved view must also win over the choice `dormer view` keeps in the
//! state directory, which outranks the config. Both files are changed as one
//! step: if the second update fails, the config is put back (unless somebody
//! changed it meanwhile, which is then reported rather than overwritten).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use toml_edit::{value, DocumentMut, Item, Table, TableLike, Value};

use super::{Channel, Kind, RuntimeSettings, SceneSources, Source};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavePolicy {
    /// Refuse when the file is no longer what was opened.
    IfUnchanged,
    /// The user has chosen to replace whatever is there now.
    Overwrite,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SaveError {
    /// The file is not valid TOML (or the settings in it are not valid). Left untouched.
    Parse(String),
    /// The file changed after it was opened. Left untouched, with the external edit in it.
    Conflict,
    /// Reading or writing failed. `recovery` says what state the files are in afterwards.
    Io { why: String, recovery: Recovery },
    /// The settings to save are not valid.
    Invalid(String),
}

/// What is left on disk after a failed save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Nothing was changed.
    Untouched,
    /// The first file was written and then put back as it was.
    RolledBack,
    /// Both files may be half-way: the message names which. Needs the user's attention.
    Partial,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Parse(e) => write!(f, "config.toml is not valid: {}", e),
            SaveError::Conflict => write!(f, "config.toml was changed by something else since it was opened"),
            SaveError::Io { why, recovery } => write!(
                f,
                "{}{}",
                why,
                match recovery {
                    Recovery::Untouched => " (nothing was changed)",
                    Recovery::RolledBack => " (earlier changes were put back)",
                    Recovery::Partial => " (the files may be half updated: check them)",
                }
            ),
            SaveError::Invalid(e) => write!(f, "cannot save: {}", e),
        }
    }
}

impl std::error::Error for SaveError {}

/// What a successful save did.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveReceipt {
    pub config: PathBuf,
    /// where the previous contents went, when there were any
    pub backup: Option<PathBuf>,
    /// the saved settings' revision: counts saves through this handle
    pub revision: u64,
    /// the persisted `dormer view` choice was removed so the saved view is in effect
    pub view_file_cleared: bool,
    pub changed_keys: Vec<String>,
}

/// What a file held when it was read: enough to tell whether it has changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fingerprint(Option<(u64, usize)>);

fn fingerprint(bytes: Option<&[u8]>) -> Fingerprint {
    Fingerprint(bytes.map(|b| {
        let mut h: u64 = 0xcbf29ce484222325;
        for byte in b {
            h = (h ^ *byte as u64).wrapping_mul(0x100000001b3);
        }
        (h, b.len())
    }))
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, SaveError> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(SaveError::Io { why: format!("cannot read {}: {}", path.display(), e), recovery: Recovery::Untouched }),
    }
}

/// Things a test can make fail on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    /// removing the persisted view choice fails
    ViewFile,
    /// ... and so does putting the config back
    ViewFileAndRollback,
}

pub struct ConfigFile {
    path: PathBuf,
    view_file: PathBuf,
    opened: Fingerprint,
    revision: u64,
    fault: Fault,
}

impl ConfigFile {
    /// Open the user's config, with the persisted view choice in the usual state directory.
    pub fn open(path: &Path) -> Result<ConfigFile, SaveError> {
        Self::open_with(path, &crate::sources::state().join("view"))
    }

    pub fn open_with(path: &Path, view_file: &Path) -> Result<ConfigFile, SaveError> {
        let bytes = read_optional(path)?;
        if let Some(b) = &bytes {
            parse_document(b)?;
        }
        Ok(ConfigFile { path: path.to_path_buf(), view_file: view_file.to_path_buf(), opened: fingerprint(bytes.as_deref()), revision: 0, fault: Fault::None })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Has anything else written the file since it was opened or last saved?
    pub fn changed_outside(&self) -> bool {
        read_optional(&self.path).map_or(true, |b| fingerprint(b.as_deref()) != self.opened)
    }

    /// Accept what is on disk now as the baseline (the user chose to reload).
    pub fn rebase(&mut self) -> Result<(), SaveError> {
        let bytes = read_optional(&self.path)?;
        if let Some(b) = &bytes {
            parse_document(b)?;
        }
        self.opened = fingerprint(bytes.as_deref());
        Ok(())
    }

    #[cfg(test)]
    pub fn inject(&mut self, fault: Fault) {
        self.fault = fault;
    }

    pub fn save(&mut self, desired: &RuntimeSettings, policy: SavePolicy) -> Result<SaveReceipt, SaveError> {
        if crate::views::find(&desired.view).is_none() {
            return Err(SaveError::Invalid(format!("there is no view '{}' (it is one of: {})", desired.view, crate::views::names())));
        }
        let before = read_optional(&self.path)?;
        let now = fingerprint(before.as_deref());
        if now != self.opened && policy == SavePolicy::IfUnchanged {
            return Err(SaveError::Conflict);
        }
        let mut doc = match &before {
            Some(b) => parse_document(b)?,
            None => DocumentMut::new(),
        };
        let current = RuntimeSettings::parse(&doc.to_string()).map_err(|e| SaveError::Parse(e.to_string()))?;
        let changed_keys = edit(&mut doc, &current, desired)?;
        let text = doc.to_string();
        // what is written must read back as what was asked for
        let back = RuntimeSettings::parse(&text).map_err(|e| SaveError::Invalid(e.to_string()))?;
        if back != normalised(desired) {
            return Err(SaveError::Invalid("the settings would not read back as saved".into()));
        }

        let target = write_target(&self.path);
        let backup = match &before {
            Some(old) => {
                let b = backup_path(&target);
                write_atomic(&b, old, None).map_err(|e| io(format!("cannot keep a backup at {}: {}", b.display(), e), Recovery::Untouched))?;
                Some(b)
            }
            None => None,
        };
        let permissions = fs::metadata(&target).ok().map(|m| m.permissions());
        write_atomic(&target, text.as_bytes(), permissions).map_err(|e| io(format!("cannot write {}: {}", target.display(), e), Recovery::Untouched))?;

        // the persisted view choice outranks look.view, so it has to agree with what was saved
        let mut cleared = false;
        let old_view = read_optional(&self.view_file).ok().flatten();
        let outranks = old_view.as_ref().is_some_and(|v| String::from_utf8_lossy(v).trim() != desired.view);
        if outranks {
            let result = if self.fault != Fault::None {
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "injected failure"))
            } else {
                fs::remove_file(&self.view_file)
            };
            match result {
                Ok(()) => cleared = true,
                Err(e) => {
                    let why = format!("cannot update {}: {}", self.view_file.display(), e);
                    return Err(self.roll_back(&target, before.as_deref(), &text, why));
                }
            }
        }
        self.opened = fingerprint(Some(text.as_bytes()));
        self.revision += 1;
        Ok(SaveReceipt { config: self.path.clone(), backup, revision: self.revision, view_file_cleared: cleared, changed_keys })
    }

    /// Put the config back as it was, unless something else has written it since we did.
    fn roll_back(&self, target: &Path, original: Option<&[u8]>, written: &str, why: String) -> SaveError {
        let on_disk = fs::read(target).ok();
        if fingerprint(on_disk.as_deref()) != fingerprint(Some(written.as_bytes())) {
            // somebody saved over our write already: that edit is newer, so it stays
            return io(format!("{}; and {} was changed meanwhile, so it was left as it is", why, target.display()), Recovery::Partial);
        }
        let restored = match original {
            Some(old) if self.fault != Fault::ViewFileAndRollback => write_atomic(target, old, fs::metadata(target).ok().map(|m| m.permissions())),
            None if self.fault != Fault::ViewFileAndRollback => fs::remove_file(target),
            _ => Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "injected failure")),
        };
        match restored {
            Ok(()) => io(why, Recovery::RolledBack),
            Err(e) => io(format!("{}; and the config could not be put back: {}", why, e), Recovery::Partial),
        }
    }
}

fn io(why: String, recovery: Recovery) -> SaveError {
    SaveError::Io { why, recovery }
}

fn parse_document(bytes: &[u8]) -> Result<DocumentMut, SaveError> {
    let text = std::str::from_utf8(bytes).map_err(|e| SaveError::Parse(e.to_string()))?;
    let doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| SaveError::Parse(e.to_string().replace('\n', " ")))?;
    RuntimeSettings::parse(text).map_err(|e| SaveError::Parse(e.to_string()))?;
    Ok(doc)
}

/// A config that is a symlink (a dotfiles manager's) is written through, not replaced.
fn write_target(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn backup_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".bak");
    target.with_file_name(name)
}

/// Same-directory temporary file, flushed, then renamed over the target.
fn write_atomic(path: &Path, bytes: &[u8], permissions: Option<fs::Permissions>) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(".{}.tmp", std::process::id()));
    let tmp = dir.join(name);
    let result = (|| {
        let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).open(&tmp)?;
        if let Some(p) = permissions {
            fs::set_permissions(&tmp, p)?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    } else if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    result
}

/// `desired` as it reads back from text: numbers rounded the way they are written.
fn normalised(desired: &RuntimeSettings) -> RuntimeSettings {
    let mut out = desired.clone();
    let round = |v: f64| (v * 10000.0).round() / 10000.0;
    for map in out.views.values_mut() {
        for v in map.values_mut() {
            *v = round(*v);
        }
    }
    out.rates.birds = round(out.rates.birds as f64) as f32;
    out.rates.meteors = round(out.rates.meteors as f64) as f32;
    out.rates.planes = round(out.rates.planes as f64) as f32;
    out
}

// ── editing ──────────────────────────────────────────────────────────────────

fn walk<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> Result<&'a mut dyn TableLike, SaveError> {
    let mut at: &mut dyn TableLike = doc.as_table_mut();
    for name in path {
        if !at.contains_key(name) {
            let mut t = Table::new();
            t.set_implicit(true);
            at.insert(name, Item::Table(t));
        }
        at = at
            .get_mut(name)
            .and_then(|i| i.as_table_like_mut())
            .ok_or_else(|| SaveError::Invalid(format!("{} is not a table in config.toml", path.join("."))))?;
    }
    Ok(at)
}

/// Set a key to a new value, keeping the comment that trails the old one.
fn put(at: &mut dyn TableLike, key: &str, new: Value) {
    let mut item = value(new);
    if let (Some(old), Some(v)) = (at.get(key).and_then(|i| i.as_value()), item.as_value_mut()) {
        *v.decor_mut() = old.decor().clone();
    }
    at.insert(key, item);
}

fn rounded(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

/// Make `doc` say `desired`, touching only the keys whose meaning changed.
/// Returns the dotted names of the keys that were set or removed.
fn edit(doc: &mut DocumentMut, current: &RuntimeSettings, desired: &RuntimeSettings) -> Result<Vec<String>, SaveError> {
    let mut touched = Vec::new();
    let mut mark = |name: &str| touched.push(name.to_string());

    if desired.view != current.view {
        put(walk(doc, &["look"])?, "view", Value::from(desired.view.clone()));
        mark("look.view");
    }
    if desired.mode != current.mode {
        put(walk(doc, &["look"])?, "mode", Value::from(desired.mode.clone()));
        mark("look.mode");
    }
    if desired.fps != current.fps {
        put(walk(doc, &["live"])?, "fps", Value::from(desired.fps as i64));
        mark("live.fps");
    }
    if desired.interactive != current.interactive {
        put(walk(doc, &["live"])?, "interactive", Value::from(desired.interactive));
        mark("live.interactive");
    }
    for (key, a, b) in [
        ("birds", desired.rates.birds, current.rates.birds),
        ("meteors", desired.rates.meteors, current.rates.meteors),
        ("planes", desired.rates.planes, current.rates.planes),
    ] {
        if a != b {
            put(walk(doc, &["activity"])?, key, Value::from(rounded(a as f64)));
            mark(&format!("activity.{}", key));
        }
    }
    if desired.stories != current.stories {
        put(walk(doc, &["activity"])?, "stories", Value::from(desired.stories));
        mark("activity.stories");
    }

    scene(doc, &current.scene, &desired.scene, &mut mark)?;

    // per-view knobs: what is saved is exactly what the panel holds for that view
    for (id, knobs) in &desired.views {
        let old = current.views.get(id);
        for (key, v) in knobs {
            if old.and_then(|o| o.get(key)) != Some(v) {
                put(walk(doc, &["views", id])?, key, Value::from(rounded(*v)));
                mark(&format!("views.{}.{}", id, key));
            }
        }
    }
    for (id, knobs) in &current.views {
        for key in knobs.keys() {
            if desired.views.get(id).and_then(|d| d.get(key)).is_none() {
                walk(doc, &["views", id])?.remove(key);
                mark(&format!("views.{}.{}", id, key));
            }
        }
    }
    Ok(touched)
}

fn scene(doc: &mut DocumentMut, old: &SceneSources, new: &SceneSources, mark: &mut dyn FnMut(&str)) -> Result<(), SaveError> {
    if old.time != new.time {
        let at = walk(doc, &["scene"])?;
        match new.time {
            Source::Fixed(c) => put(at, "time", Value::from(c.text())),
            Source::Auto => {
                at.remove("time");
            }
        }
        mark("scene.time");
    }
    if old.date != new.date {
        let at = walk(doc, &["scene"])?;
        match new.date {
            Source::Fixed(d) => put(at, "date", Value::from(d.text())),
            Source::Auto => {
                at.remove("date");
            }
        }
        mark("scene.date");
    }
    for channel in Channel::ALL {
        if old.get(channel) == new.get(channel) {
            continue;
        }
        let at = walk(doc, channel.section())?;
        match new.get(channel) {
            Source::Fixed(v) => put(
                at,
                channel.toml_key(),
                match channel.kind() {
                    Kind::Switch => Value::from(v > 0.5),
                    Kind::Whole => Value::from(v.round() as i64),
                    Kind::Float => Value::from(rounded(v)),
                },
            ),
            Source::Auto => {
                at.remove(channel.toml_key());
            }
        }
        mark(&format!("{}.{}", channel.section().join("."), channel.toml_key()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Dir {
            let p = std::env::temp_dir().join(format!("dormer-file-{}-{}", std::process::id(), name));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const ORIGINAL: &str = "# my dormer\n[place]\nname = \"Tangier\"   # home\nlatitude = 35.78\n\n[look]\nview = \"rooftops\"  # the roof\nmood = \"wistful\"\n\n[live]\nfps = 30\n\n[someday]\nthing = [1, 2]\n";

    #[test]
    fn roundtrip_preserves_comments_unknown_keys_and_permissions() {
        let d = Dir::new("roundtrip");
        let (cfg, view) = (d.path("config.toml"), d.path("view"));
        fs::write(&cfg, ORIGINAL).unwrap();
        fs::set_permissions(&cfg, fs::Permissions::from_mode(0o640)).unwrap();
        let mut file = ConfigFile::open_with(&cfg, &view).unwrap();
        let mut want = RuntimeSettings::parse(ORIGINAL).unwrap();
        want.view = "coast".into();
        want.fps = 24;
        want.scene.time = Source::Fixed(super::super::Clock::new(21, 30).unwrap());
        want.scene.fix(Channel::Rain, 40.0);
        want.scene.fix(Channel::Agents, 2.0);
        want.views.entry("coast".into()).or_default().insert("tide".into(), 0.5);
        let receipt = file.save(&want, SavePolicy::IfUnchanged).unwrap();
        assert_eq!(receipt.revision, 1);
        assert_eq!(fs::read_to_string(receipt.backup.as_ref().unwrap()).unwrap(), ORIGINAL);
        let saved = fs::read_to_string(&cfg).unwrap();
        // comments, order, unknown keys and untouched sections survive
        assert!(saved.starts_with("# my dormer\n[place]\nname = \"Tangier\"   # home\n"), "{}", saved);
        assert!(saved.contains("view = \"coast\"  # the roof"), "{}", saved);
        assert!(saved.contains("mood = \"wistful\""));
        assert!(saved.contains("[someday]\nthing = [1, 2]"));
        assert!(saved.contains("fps = 24"));
        assert!(saved.contains("time = \"21:30\""));
        assert!(saved.contains("[scene.simulated_machine]") && saved.contains("agents = 2"));
        assert_eq!(fs::metadata(&cfg).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(RuntimeSettings::parse(&saved).unwrap(), want);
        // saving the same thing again writes nothing new but is still a good save
        let again = file.save(&want, SavePolicy::IfUnchanged).unwrap();
        assert_eq!(again.revision, 2);
        assert!(again.changed_keys.is_empty());
        assert_eq!(fs::read_to_string(&cfg).unwrap(), saved);
        // going back to Auto removes the key rather than writing a default
        want.scene.release(Channel::Rain);
        file.save(&want, SavePolicy::IfUnchanged).unwrap();
        assert!(!fs::read_to_string(&cfg).unwrap().contains("rain"));
    }

    #[test]
    fn a_new_file_is_created_with_only_what_differs() {
        let d = Dir::new("new");
        let cfg = d.path("sub/config.toml");
        let mut file = ConfigFile::open_with(&cfg, &d.path("view")).unwrap();
        let mut want = RuntimeSettings::default();
        want.fps = 15;
        let r = file.save(&want, SavePolicy::IfUnchanged).unwrap();
        assert!(r.backup.is_none());
        assert_eq!(fs::read_to_string(&cfg).unwrap().trim(), "[live]\nfps = 15");
    }

    #[test]
    fn conflict_preserves_external_edit_and_draft() {
        let d = Dir::new("conflict");
        let (cfg, view) = (d.path("config.toml"), d.path("view"));
        fs::write(&cfg, ORIGINAL).unwrap();
        let mut file = ConfigFile::open_with(&cfg, &view).unwrap();
        let external = ORIGINAL.replace("35.78", "40.0");
        fs::write(&cfg, &external).unwrap();
        assert!(file.changed_outside());
        let mut draft = RuntimeSettings::parse(ORIGINAL).unwrap();
        draft.fps = 12;
        assert_eq!(file.save(&draft, SavePolicy::IfUnchanged), Err(SaveError::Conflict));
        assert_eq!(fs::read_to_string(&cfg).unwrap(), external, "the external edit must still be there");
        // an explicit overwrite keeps the external edit to keys the panel does not own
        file.save(&draft, SavePolicy::Overwrite).unwrap();
        let saved = fs::read_to_string(&cfg).unwrap();
        assert!(saved.contains("latitude = 40.0") && saved.contains("fps = 12"));
        assert!(!file.changed_outside());
        // or the user reloads, and the draft is judged against what is there now
        fs::write(&cfg, format!("{}\n# again\n", saved)).unwrap();
        assert!(file.changed_outside());
        file.rebase().unwrap();
        assert!(!file.changed_outside());
    }

    #[test]
    fn invalid_config_is_never_written_over() {
        let d = Dir::new("invalid");
        let cfg = d.path("config.toml");
        fs::write(&cfg, "[live\nfps = 3").unwrap();
        assert!(matches!(ConfigFile::open_with(&cfg, &d.path("view")), Err(SaveError::Parse(_))));
        fs::write(&cfg, ORIGINAL).unwrap();
        let mut file = ConfigFile::open_with(&cfg, &d.path("view")).unwrap();
        let mut bad = RuntimeSettings::default();
        bad.view = "atlantis".into();
        assert!(matches!(file.save(&bad, SavePolicy::IfUnchanged), Err(SaveError::Invalid(_))));
        assert_eq!(fs::read_to_string(&cfg).unwrap(), ORIGINAL);
    }

    #[test]
    fn view_override_is_reconciled() {
        let d = Dir::new("view");
        let (cfg, view) = (d.path("config.toml"), d.path("view"));
        fs::write(&cfg, ORIGINAL).unwrap();
        fs::write(&view, "street\n").unwrap();
        let mut file = ConfigFile::open_with(&cfg, &view).unwrap();
        let mut want = RuntimeSettings::parse(ORIGINAL).unwrap();
        want.view = "coast".into();
        let r = file.save(&want, SavePolicy::IfUnchanged).unwrap();
        assert!(r.view_file_cleared && !view.exists(), "the stale persisted choice must not outrank the saved view");
        // a persisted choice that already agrees is left alone
        fs::write(&view, "coast").unwrap();
        let r = file.save(&want, SavePolicy::IfUnchanged).unwrap();
        assert!(!r.view_file_cleared && view.exists());
    }

    #[test]
    fn failed_second_file_update_rolls_back_owned_write() {
        let d = Dir::new("rollback");
        let (cfg, view) = (d.path("config.toml"), d.path("view"));
        fs::write(&cfg, ORIGINAL).unwrap();
        fs::write(&view, "street").unwrap();
        let mut file = ConfigFile::open_with(&cfg, &view).unwrap();
        let mut want = RuntimeSettings::parse(ORIGINAL).unwrap();
        want.view = "city".into();
        want.fps = 10;
        file.inject(Fault::ViewFile);
        match file.save(&want, SavePolicy::IfUnchanged) {
            Err(SaveError::Io { recovery: Recovery::RolledBack, .. }) => {}
            other => panic!("{:?}", other),
        }
        assert_eq!(fs::read_to_string(&cfg).unwrap(), ORIGINAL, "config must be recoverable");
        assert_eq!(fs::read_to_string(&view).unwrap(), "street", "the old view selection stays");
        // and the same handle can save once the fault is gone
        file.inject(Fault::None);
        assert!(!file.changed_outside());
        file.save(&want, SavePolicy::IfUnchanged).unwrap();
        assert!(fs::read_to_string(&cfg).unwrap().contains("view = \"city\""));
        assert!(!view.exists());

        // when even putting it back fails, say so
        fs::write(&view, "street").unwrap();
        fs::write(&cfg, ORIGINAL).unwrap();
        let mut file = ConfigFile::open_with(&cfg, &view).unwrap();
        file.inject(Fault::ViewFileAndRollback);
        assert!(matches!(file.save(&want, SavePolicy::IfUnchanged), Err(SaveError::Io { recovery: Recovery::Partial, .. })));
    }

    #[test]
    fn a_symlinked_config_is_written_through() {
        let d = Dir::new("link");
        let real = d.path("dotfiles-config.toml");
        let link = d.path("config.toml");
        fs::write(&real, ORIGINAL).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let mut file = ConfigFile::open_with(&link, &d.path("view")).unwrap();
        let mut want = RuntimeSettings::parse(ORIGINAL).unwrap();
        want.fps = 20;
        file.save(&want, SavePolicy::IfUnchanged).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(fs::read_to_string(&real).unwrap().contains("fps = 20"));
    }
}
