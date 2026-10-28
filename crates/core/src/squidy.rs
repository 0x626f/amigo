//! Named multigraph boards backed by Markdown cards and a separate .link file.
use crate::{
    model::{TaskAttachment, id},
    storage::atomic_write,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub brief: String,
    #[serde(default)]
    pub data: String,
    #[serde(default)]
    pub position: Option<[f32; 2]>,
    #[serde(default = "card_color")]
    pub color: [u8; 3],
    #[serde(default)]
    pub attachments: Vec<TaskAttachment>,
}
pub fn card_color() -> [u8; 3] {
    [88, 142, 208]
}
pub fn edge_color() -> [u8; 3] {
    [112, 126, 148]
}
impl Card {
    pub fn new(name: &str) -> Self {
        Self {
            id: id(),
            name: name.into(),
            brief: String::new(),
            data: String::new(),
            position: None,
            color: card_color(),
            attachments: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub id: String,
    pub from: String,
    pub to: String,
    pub title: String,
    // Older files contain directed connections without an explicit direction field.
    #[serde(default = "legacy_directed")]
    pub directed: bool,
    #[serde(default = "edge_color")]
    pub color: [u8; 3],
}
impl Link {
    pub fn new(from: &str, to: &str, title: &str) -> Self {
        Self {
            id: id(),
            from: from.into(),
            to: to.into(),
            title: title.into(),
            directed: false,
            color: edge_color(),
        }
    }
}
fn legacy_directed() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub pan: [f32; 2],
    #[serde(default = "unit_zoom")]
    pub zoom: f32,
    #[serde(default)]
    pub cards: Vec<Card>,
    #[serde(default)]
    pub links: Vec<Link>,
}
fn unit_zoom() -> f32 {
    1.
}
impl Board {
    pub fn new(name: &str) -> Self {
        Self {
            id: id(),
            name: name.into(),
            pan: [40., 40.],
            zoom: 1.,
            cards: vec![],
            links: vec![],
        }
    }
    pub fn card(&self, name: &str) -> Option<&Card> {
        self.cards.iter().find(|card| key(&card.name) == key(name))
    }
    pub fn put_card(&mut self, mut card: Card) -> Result<()> {
        card.name = card.name.trim().into();
        valid_name(&card.name)?;
        if self
            .cards
            .iter()
            .any(|other| other.id != card.id && key(&other.name) == key(&card.name))
        {
            return Err("A card with this name already exists on this board.".into());
        }
        if let Some(old) = self.cards.iter_mut().find(|old| old.id == card.id) {
            for link in &mut self.links {
                if link.from == old.name {
                    link.from.clone_from(&card.name);
                }
                if link.to == old.name {
                    link.to.clone_from(&card.name);
                }
            }
            *old = card;
        } else {
            self.cards.push(card);
        }
        Ok(())
    }
    pub fn put_link(&mut self, mut link: Link) -> Result<()> {
        link.title = link.title.trim().into();
        valid_name(&link.title)?;
        link.from = self
            .card(&link.from)
            .ok_or("Choose an existing source card.")?
            .name
            .clone();
        link.to = self
            .card(&link.to)
            .ok_or("Choose an existing destination card.")?
            .name
            .clone();
        if let Some(old) = self.links.iter_mut().find(|old| old.id == link.id) {
            *old = link;
        } else {
            self.links.push(link);
        }
        Ok(())
    }
    pub fn remove_card(&mut self, id: &str) {
        if let Some(card) = self.cards.iter().find(|card| card.id == id) {
            let name = card.name.clone();
            self.links
                .retain(|link| link.from != name && link.to != name);
        }
        self.cards.retain(|card| card.id != id);
    }
    pub fn validate(&self) -> Result<()> {
        valid_id(&self.id)?;
        valid_name(&self.name)?;
        if !self.zoom.is_finite()
            || !(0.15..=3.).contains(&self.zoom)
            || self.pan.iter().any(|n| !n.is_finite())
        {
            return Err("Invalid board view position.".into());
        }
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for card in &self.cards {
            valid_id(&card.id)?;
            valid_name(&card.name)?;
            if !names.insert(key(&card.name)) || !ids.insert(&card.id) {
                return Err("Card names and file IDs must be unique within a board.".into());
            }
            if card
                .position
                .is_some_and(|p| p.iter().any(|n| !n.is_finite() || n.abs() > 10_000_000.))
            {
                return Err("Invalid card position.".into());
            }
        }
        let mut ids = BTreeSet::new();
        for link in &self.links {
            valid_id(&link.id)?;
            valid_name(&link.title)?;
            if !ids.insert(&link.id)
                || !self.cards.iter().any(|c| c.name == link.from)
                || !self.cards.iter().any(|c| c.name == link.to)
            {
                return Err("A connection has a duplicate ID or references a missing card.".into());
            }
        }
        Ok(())
    }
    pub fn path(&self, from: &str, to: &str, filter: &Filter) -> Result<Option<GraphPath>> {
        let start = self
            .card(from)
            .ok_or("The starting card name was not found.")?
            .name
            .clone();
        let end = self
            .card(to)
            .ok_or("The destination card name was not found.")?
            .name
            .clone();
        let mut queue = VecDeque::from([start.clone()]);
        let mut seen = BTreeSet::from([start.clone()]);
        let mut previous = BTreeMap::<String, (String, String)>::new();
        let mut adjacency = BTreeMap::<&str, Vec<(&str, &str)>>::new();
        for link in self.links.iter().filter(|link| filter.accepts(&link.title)) {
            adjacency
                .entry(&link.from)
                .or_default()
                .push((&link.to, &link.id));
            if !link.directed || !filter.directed {
                adjacency
                    .entry(&link.to)
                    .or_default()
                    .push((&link.from, &link.id));
            }
        }
        while let Some(current) = queue.pop_front() {
            if current == end {
                let mut path = GraphPath {
                    names: vec![end.clone()],
                    links: vec![],
                };
                let mut node = end;
                while let Some((parent, edge)) = previous.get(&node) {
                    path.names.push(parent.clone());
                    path.links.push(edge.clone());
                    node = parent.clone();
                }
                path.names.reverse();
                path.links.reverse();
                return Ok(Some(path));
            }
            for (next, edge) in adjacency.get(current.as_str()).into_iter().flatten() {
                if seen.insert((*next).to_owned()) {
                    previous.insert((*next).to_owned(), (current.clone(), (*edge).to_owned()));
                    queue.push_back((*next).to_owned());
                }
            }
        }
        Ok(None)
    }
    /// Layer connected cards, keep manual positions pinned, and leave a readable gutter.
    pub fn positions(&self) -> BTreeMap<String, [f32; 2]> {
        let mut result: BTreeMap<_, _> = self
            .cards
            .iter()
            .filter_map(|c| c.position.map(|p| (c.id.clone(), p)))
            .collect();
        let mut order = Vec::new();
        let mut seen = BTreeSet::new();
        let roots = self
            .cards
            .iter()
            .filter(|c| !self.links.iter().any(|e| e.to == c.name))
            .chain(self.cards.iter());
        for root in roots {
            if seen.contains(&root.name) {
                continue;
            }
            let mut queue = VecDeque::from([(root.name.clone(), 0usize)]);
            while let Some((name, depth)) = queue.pop_front() {
                if !seen.insert(name.clone()) {
                    continue;
                }
                order.push((name.clone(), depth));
                for edge in self.links.iter().filter(|e| e.from == name) {
                    queue.push_back((edge.to.clone(), depth + 1));
                }
            }
        }
        let mut rows = BTreeMap::<usize, usize>::new();
        let mut isolated = 0;
        for (name, depth) in order {
            let Some(card) = self.card(&name) else {
                continue;
            };
            if result.contains_key(&card.id) {
                continue;
            }
            let column = if !self.links.iter().any(|e| e.from == name || e.to == name) {
                let column = isolated % 3;
                isolated += 1;
                column
            } else {
                depth.min(5)
            };
            let row = rows.entry(column).or_default();
            let mut candidate = [column as f32 * 330., *row as f32 * 180.];
            while result
                .values()
                .any(|p| (p[0] - candidate[0]).abs() < 290. && (p[1] - candidate[1]).abs() < 150.)
            {
                *row += 1;
                candidate[1] = *row as f32 * 180.;
            }
            *row += 1;
            result.insert(card.id.clone(), candidate);
        }
        result
    }
    pub fn arrange(&mut self) {
        for card in &mut self.cards {
            card.position = None;
        }
        let positions = self.positions();
        for card in &mut self.cards {
            card.position = positions.get(&card.id).copied();
        }
    }
}

fn key(value: &str) -> String {
    value.trim().to_lowercase()
}
fn valid_name(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control)
    {
        return Err("Use a nonempty name of at most 200 characters, without line breaks.".into());
    }
    Ok(())
}
fn valid_id(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Invalid Squidy file ID.".into());
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Filter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// Respect one-way connections; bidirectional connections always work both ways.
    pub directed: bool,
}
impl Default for Filter {
    fn default() -> Self {
        Self {
            include: vec![],
            exclude: vec![],
            directed: true,
        }
    }
}
impl Filter {
    fn accepts(&self, title: &str) -> bool {
        !self.exclude.iter().any(|s| key(s) == key(title))
            && (self.include.is_empty() || self.include.iter().any(|s| key(s) == key(title)))
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct GraphPath {
    pub names: Vec<String>,
    pub links: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
}
#[derive(Serialize, Deserialize)]
struct Metadata {
    version: u32,
    id: String,
    name: String,
    pan: [f32; 2],
    zoom: f32,
    order: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct Connections {
    version: u32,
    links: Vec<Link>,
}

pub struct Store {
    pub root: PathBuf,
}
impl Store {
    pub fn open(library: &Path) -> Result<Self> {
        let root = library.join("squidy");
        if fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Squidy storage must not be a symbolic link.".into());
        }
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(Self { root })
    }
    pub fn folder(&self, id: &str) -> Result<PathBuf> {
        valid_id(id)?;
        let path = self.root.join(id);
        for item in [&path, &path.join("content"), &path.join("content/cards")] {
            if fs::symlink_metadata(item).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("Invalid board folder.".into());
            }
        }
        Ok(path)
    }
    fn recover(&self, folder: &Path) -> Result<()> {
        let marker = folder.join("commit.json");
        if !marker.exists() {
            return Ok(());
        }
        let old: String = serde_json::from_slice(&fs::read(&marker).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        valid_id(&old)?;
        if !old.starts_with("old-") {
            return Err("Invalid Squidy recovery record.".into());
        }
        if !folder.join("content").exists() && folder.join(&old).exists() {
            fs::rename(folder.join(&old), folder.join("content")).map_err(|e| e.to_string())?;
        }
        if folder.join(&old).exists() {
            fs::remove_dir_all(folder.join(old)).map_err(|e| e.to_string())?;
        }
        fs::remove_file(marker).map_err(|e| e.to_string())
    }
    pub fn entries(&self) -> Result<Vec<Entry>> {
        let mut entries = vec![];
        for item in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            if !item.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            let name = item.file_name().to_string_lossy().into_owned();
            valid_id(&name)?;
            let folder = self.folder(&name)?;
            self.recover(&folder)?;
            let meta: Metadata = read(&folder.join("content/board.json"))?;
            if meta.id != name || meta.version != 1 {
                return Err("Invalid Squidy board metadata.".into());
            }
            entries.push(Entry {
                id: meta.id,
                name: meta.name,
            });
        }
        entries.sort_by_key(|e| key(&e.name));
        Ok(entries)
    }
    pub fn load(&self, id: &str) -> Result<(Board, String)> {
        let folder = self.folder(id)?;
        self.recover(&folder)?;
        let content = folder.join("content");
        let meta: Metadata = read(&content.join("board.json"))?;
        let links: Connections = read(&content.join("connections.link"))?;
        if meta.version != 1 || links.version != 1 || meta.id != id {
            return Err("Unsupported Squidy board format.".into());
        }
        let mut board = Board {
            id: meta.id,
            name: meta.name,
            pan: meta.pan,
            zoom: meta.zoom,
            cards: vec![],
            links: links.links,
        };
        let mut files = BTreeMap::new();
        for item in card_files(&content)? {
            if item.path().extension().is_none_or(|e| e != "md") {
                continue;
            }
            if !item.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err("Card files must be regular Markdown files.".into());
            }
            let raw = fs::read_to_string(item.path()).map_err(|e| e.to_string())?;
            let raw = raw
                .strip_prefix("---\n")
                .or_else(|| raw.strip_prefix("---\r\n"))
                .ok_or("Missing card properties.")?;
            let (json, data) = raw
                .split_once("\n---\n")
                .or_else(|| raw.split_once("\r\n---\r\n"))
                .ok_or("Invalid Markdown card properties.")?;
            let mut card: Card = serde_json::from_str(json).map_err(|e| e.to_string())?;
            if item.path().file_stem().and_then(|s| s.to_str()) != Some(&card.id) {
                return Err("Card file ID does not match its properties.".into());
            }
            card.data = data
                .strip_prefix("\r\n")
                .or_else(|| data.strip_prefix('\n'))
                .unwrap_or(data)
                .into();
            files.insert(card.id.clone(), card);
        }
        for id in meta.order {
            if let Some(card) = files.remove(&id) {
                board.cards.push(card);
            }
        }
        board.cards.extend(files.into_values());
        board.validate()?;
        Ok((board, fingerprint(&content)?))
    }
    pub fn create(&self, name: &str) -> Result<Board> {
        let name = crate::library::name(name)?;
        if self.entries()?.iter().any(|e| key(&e.name) == key(&name)) {
            return Err("A board with this name already exists.".into());
        }
        let board = Board::new(&name);
        self.save(&board, None)?;
        Ok(board)
    }
    pub fn save(&self, board: &Board, expected: Option<&str>) -> Result<String> {
        board.validate()?;
        if self
            .entries()?
            .iter()
            .any(|entry| entry.id != board.id && key(&entry.name) == key(&board.name))
        {
            return Err("A board with this name already exists.".into());
        }
        let folder = self.folder(&board.id)?;
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        self.recover(&folder)?;
        let content = folder.join("content");
        if let Some(expected) = expected
            && fingerprint(&content)? != expected
        {
            return Err("This board changed outside Amigo. Your edits are preserved; refresh only when you are ready to reload the saved version.".into());
        }
        let stage = folder.join(format!("stage-{}", id()));
        fs::create_dir_all(stage.join("cards")).map_err(|e| e.to_string())?;
        let result = (|| {
            // Preserve user-added files that are not managed Markdown cards.
            if content.exists() {
                for entry in fs::read_dir(&content).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let name = entry.file_name();
                    if name == "cards" || name == "board.json" || name == "connections.link" {
                        continue;
                    }
                    if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                        return Err("Unexpected directory or link in Squidy content.".into());
                    }
                    fs::copy(entry.path(), stage.join(name)).map_err(|e| e.to_string())?;
                }
                for entry in card_files(&content)? {
                    if entry.path().extension().is_some_and(|e| e == "md") {
                        continue;
                    }
                    if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                        return Err("Unexpected directory or link in Squidy cards.".into());
                    }
                    fs::copy(entry.path(), stage.join("cards").join(entry.file_name()))
                        .map_err(|e| e.to_string())?;
                }
            }
            write(
                &stage.join("board.json"),
                &Metadata {
                    version: 1,
                    id: board.id.clone(),
                    name: board.name.clone(),
                    pan: board.pan,
                    zoom: board.zoom,
                    order: board.cards.iter().map(|c| c.id.clone()).collect(),
                },
            )?;
            write(
                &stage.join("connections.link"),
                &Connections {
                    version: 1,
                    links: board.links.clone(),
                },
            )?;
            for card in &board.cards {
                let mut value = serde_json::to_value(card).map_err(|e| e.to_string())?;
                value.as_object_mut().unwrap().remove("data");
                let text = format!(
                    "---\n{}\n---\n\n{}",
                    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?,
                    card.data
                );
                atomic_write(
                    &stage.join("cards").join(format!("{}.md", card.id)),
                    text.as_bytes(),
                )?;
            }
            let old = format!("old-{}", id());
            write(&folder.join("commit.json"), &old)?;
            if content.exists() {
                fs::rename(&content, folder.join(&old)).map_err(|e| e.to_string())?;
            }
            if let Err(error) = fs::rename(&stage, &content) {
                self.recover(&folder)?;
                return Err(error.to_string());
            }
            self.recover(&folder)?;
            fingerprint(&content)
        })();
        if stage.exists() {
            let _ = fs::remove_dir_all(stage);
        }
        result
    }
    pub fn delete(&self, id: &str) -> Result<()> {
        fs::remove_dir_all(self.folder(id)?).map_err(|e| e.to_string())
    }
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    if fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Squidy files must not be symbolic links.".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_write(
        path,
        &serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
}
fn fingerprint(root: &Path) -> Result<String> {
    let mut files = vec![root.join("board.json"), root.join("connections.link")];
    files.extend(card_files(root)?.into_iter().map(|entry| entry.path()));
    files.sort();
    let mut hash = Sha256::new();
    for file in files {
        if !fs::symlink_metadata(&file)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("Invalid file in Squidy card directory.".into());
        }
        let name = file.file_name().unwrap().to_string_lossy();
        let bytes = fs::read(&file).map_err(|e| e.to_string())?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn card_files(content: &Path) -> Result<Vec<fs::DirEntry>> {
    match fs::read_dir(content.join("cards")) {
        Ok(entries) => entries
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string()),
        // Setup archives contain files, so an empty card directory is not serialized.
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && read::<Metadata>(&content.join("board.json"))?
                    .order
                    .is_empty() =>
        {
            Ok(vec![])
        }
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
#[path = "squidy_tests.rs"]
mod tests;
