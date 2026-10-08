// SPDX-License-Identifier: MIT
//! Compound files (the structured storage container of `.ipt` files) as
//! Microsoft's published specification describes them (MS-CFB): a header,
//! a file allocation table (FAT) chaining fixed-size sectors, a directory of
//! storages and streams kept as red-black trees, and a mini stream with its
//! own table for streams below the cutoff size. Versions 3 (512-byte
//! sectors) and 4 (4096-byte sectors) are read.
//!
//! The reader checks every chain against the file's size and against
//! cycles, so a damaged or hostile file gives an error, never a hang.

use std::collections::BTreeSet;
use std::fmt;

/// The signature at the start of every compound file.
pub const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Largest regular sector number; larger values have special meanings.
const MAX_REGULAR: u32 = 0xFFFF_FFFA;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
/// No sibling or child in the directory.
const NO_STREAM: u32 = 0xFFFF_FFFF;

#[derive(Clone, Debug, PartialEq)]
pub enum CfbError {
    /// Not a compound file (no signature).
    NotCompound,
    /// The header's fields are outside what the specification allows.
    Header(String),
    /// A sector chain or a directory entry points outside the file or
    /// loops.
    Corrupt(String),
    Io(String),
}

impl fmt::Display for CfbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CfbError::NotCompound => f.write_str("not a compound file"),
            CfbError::Header(e) => write!(f, "bad compound file header: {e}"),
            CfbError::Corrupt(e) => write!(f, "damaged compound file: {e}"),
            CfbError::Io(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for CfbError {}

/// What a directory entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Storage,
    Stream,
    Root,
}

/// One directory entry.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    /// The class id of a storage (all zero when unset).
    pub clsid: [u8; 16],
    /// First sector of a stream (in the mini stream when it is small).
    pub start: u32,
    pub size: u64,
    left: u32,
    right: u32,
    child: u32,
}

/// A parsed compound file. The whole file is kept in memory.
pub struct CompoundFile {
    data: Vec<u8>,
    /// Major version: 3 or 4.
    pub version: u16,
    sector_size: usize,
    mini_sector_size: usize,
    mini_cutoff: u64,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    entries: Vec<Entry>,
    mini_stream: Vec<u8>,
    /// Paths of the entries reachable from the root, depth first; the
    /// root is "", the others "/a/b".
    paths: Vec<(String, usize)>,
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn u64_at(d: &[u8], o: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[o..o + 8]);
    u64::from_le_bytes(b)
}

impl CompoundFile {
    pub fn open(path: &std::path::Path) -> Result<CompoundFile, CfbError> {
        let data =
            std::fs::read(path).map_err(|e| CfbError::Io(format!("{}: {e}", path.display())))?;
        CompoundFile::parse(data)
    }

    pub fn parse(data: Vec<u8>) -> Result<CompoundFile, CfbError> {
        if data.len() < 512 || data[..8] != SIGNATURE {
            return Err(CfbError::NotCompound);
        }
        let version = u16_at(&data, 0x1A);
        let byte_order = u16_at(&data, 0x1C);
        let sector_shift = u16_at(&data, 0x1E);
        let mini_shift = u16_at(&data, 0x20);
        if byte_order != 0xFFFE {
            return Err(CfbError::Header(format!("byte order {byte_order:#x}")));
        }
        match (version, sector_shift) {
            (3, 9) | (4, 12) => {}
            _ => {
                return Err(CfbError::Header(format!(
                    "version {version} with sectors of 2^{sector_shift} bytes"
                )));
            }
        }
        if mini_shift != 6 {
            return Err(CfbError::Header(format!(
                "mini sectors of 2^{mini_shift} bytes"
            )));
        }
        let sector_size = 1usize << sector_shift;
        let fat_sectors = u32_at(&data, 0x2C) as usize;
        let first_dir = u32_at(&data, 0x30);
        let mini_cutoff = u64::from(u32_at(&data, 0x38));
        let first_mini_fat = u32_at(&data, 0x3C);
        let first_difat = u32_at(&data, 0x44);
        let difat_sectors = u32_at(&data, 0x48) as usize;
        let sector_count = (data.len() / sector_size).saturating_sub(1);
        if fat_sectors > sector_count {
            return Err(CfbError::Header(format!(
                "{fat_sectors} FAT sectors in a file of {sector_count}"
            )));
        }

        let mut file = CompoundFile {
            data,
            version,
            sector_size,
            mini_sector_size: 64,
            mini_cutoff,
            fat: Vec::new(),
            mini_fat: Vec::new(),
            entries: Vec::new(),
            mini_stream: Vec::new(),
            paths: Vec::new(),
        };

        // The FAT's sectors: 109 in the header, the rest in DIFAT sectors
        // (each ends with the next DIFAT sector's number).
        let mut fat_list: Vec<u32> = (0..109).map(|i| u32_at(&file.data, 0x4C + 4 * i)).collect();
        let mut difat = first_difat;
        let mut seen = BTreeSet::new();
        for _ in 0..difat_sectors {
            if difat > MAX_REGULAR {
                break;
            }
            if !seen.insert(difat) {
                return Err(CfbError::Corrupt("the DIFAT chain loops".into()));
            }
            let sector = file.sector(difat)?;
            let per = sector_size / 4;
            fat_list.extend((0..per - 1).map(|i| u32_at(sector, 4 * i)));
            difat = u32_at(sector, 4 * (per - 1));
        }
        let mut fat = Vec::with_capacity(fat_sectors * sector_size / 4);
        for &s in fat_list.iter().take(fat_sectors) {
            let sector = file.sector(s)?;
            fat.extend((0..sector_size / 4).map(|i| u32_at(sector, 4 * i)));
        }
        file.fat = fat;

        // The directory: 128-byte entries.
        let dir = file.chain(first_dir, None)?;
        let mut entries = Vec::with_capacity(dir.len() / 128);
        for raw in dir.as_chunks::<128>().0 {
            let kind = match raw[0x42] {
                1 => Some(EntryKind::Storage),
                2 => Some(EntryKind::Stream),
                5 => Some(EntryKind::Root),
                _ => None,
            };
            let name_len = (u16_at(raw, 0x40) as usize).min(64);
            let units: Vec<u16> = (0..name_len.saturating_sub(2) / 2)
                .map(|i| u16_at(raw, 2 * i))
                .collect();
            let mut clsid = [0u8; 16];
            clsid.copy_from_slice(&raw[0x50..0x60]);
            // Version 3 files may leave junk in the size's high half.
            let size = if version == 3 {
                u64::from(u32_at(raw, 0x78))
            } else {
                u64_at(raw, 0x78)
            };
            entries.push((
                kind,
                Entry {
                    name: String::from_utf16_lossy(&units),
                    kind: kind.unwrap_or(EntryKind::Stream),
                    clsid,
                    start: u32_at(raw, 0x74),
                    size,
                    left: u32_at(raw, 0x44),
                    right: u32_at(raw, 0x48),
                    child: u32_at(raw, 0x4C),
                },
            ));
        }
        if entries.first().map(|e| e.0) != Some(Some(EntryKind::Root)) {
            return Err(CfbError::Corrupt("no root entry".into()));
        }
        // Unused entries stay in the table (so that indices hold) but are
        // never reached from the root.
        let valid: Vec<bool> = entries.iter().map(|e| e.0.is_some()).collect();
        file.entries = entries.into_iter().map(|e| e.1).collect();

        // The mini stream (the root's data) and its allocation table.
        if first_mini_fat <= MAX_REGULAR {
            let raw = file.chain(first_mini_fat, None)?;
            file.mini_fat = raw
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect();
        }
        let root = file.entries[0].clone();
        if root.start <= MAX_REGULAR && root.size > 0 {
            file.mini_stream = file.chain(root.start, Some(root.size))?;
        }

        file.paths = file.walk(&valid)?;
        Ok(file)
    }

    fn sector(&self, n: u32) -> Result<&[u8], CfbError> {
        let start = (n as usize + 1)
            .checked_mul(self.sector_size)
            .ok_or_else(|| CfbError::Corrupt(format!("sector {n}")))?;
        self.data
            .get(start..start + self.sector_size)
            .ok_or_else(|| CfbError::Corrupt(format!("sector {n} past the end of the file")))
    }

    /// The bytes of a FAT chain, cut to `size` when given.
    fn chain(&self, first: u32, size: Option<u64>) -> Result<Vec<u8>, CfbError> {
        let mut out = Vec::new();
        let mut n = first;
        let mut visited = vec![false; self.fat.len().max(1)];
        while n != END_OF_CHAIN {
            if n > MAX_REGULAR {
                return Err(CfbError::Corrupt(format!(
                    "chain from sector {first} has {n:#x}"
                )));
            }
            match visited.get_mut(n as usize) {
                Some(true) => {
                    return Err(CfbError::Corrupt(format!(
                        "chain from sector {first} loops"
                    )));
                }
                Some(seen) => *seen = true,
                // Before the FAT is read (its own sectors): not tracked.
                None if self.fat.is_empty() => {}
                None => return Err(CfbError::Corrupt(format!("sector {n} has no FAT entry"))),
            }
            out.extend_from_slice(self.sector(n)?);
            if let Some(size) = size
                && out.len() as u64 >= size
            {
                break;
            }
            n = *self
                .fat
                .get(n as usize)
                .ok_or_else(|| CfbError::Corrupt(format!("sector {n} has no FAT entry")))?;
        }
        if let Some(size) = size {
            if (out.len() as u64) < size {
                return Err(CfbError::Corrupt(format!(
                    "chain from sector {first} holds {} of {size} bytes",
                    out.len()
                )));
            }
            out.truncate(size as usize);
        }
        Ok(out)
    }

    fn mini_chain(&self, first: u32, size: u64) -> Result<Vec<u8>, CfbError> {
        let mut out = Vec::with_capacity(size as usize);
        let mut n = first;
        let mut visited = vec![false; self.mini_fat.len()];
        while (out.len() as u64) < size {
            if n > MAX_REGULAR {
                return Err(CfbError::Corrupt(format!(
                    "mini chain from {first} ends early"
                )));
            }
            match visited.get_mut(n as usize) {
                Some(false) => visited[n as usize] = true,
                Some(true) => {
                    return Err(CfbError::Corrupt(format!("mini chain from {first} loops")));
                }
                None => return Err(CfbError::Corrupt(format!("mini sector {n} has no entry"))),
            }
            let start = n as usize * self.mini_sector_size;
            let bytes = self
                .mini_stream
                .get(start..start + self.mini_sector_size)
                .ok_or_else(|| {
                    CfbError::Corrupt(format!("mini sector {n} past the mini stream"))
                })?;
            out.extend_from_slice(bytes);
            n = *self
                .mini_fat
                .get(n as usize)
                .ok_or_else(|| CfbError::Corrupt(format!("mini sector {n} has no entry")))?;
        }
        out.truncate(size as usize);
        Ok(out)
    }

    /// Entry paths depth first, each storage's children in name order.
    fn walk(&self, valid: &[bool]) -> Result<Vec<(String, usize)>, CfbError> {
        let mut out = vec![(String::new(), 0)];
        let mut visited = vec![false; self.entries.len()];
        visited[0] = true;
        let mut stack = vec![(String::new(), self.entries[0].child)];
        while let Some((parent, child)) = stack.pop() {
            let mut members = Vec::new();
            // The siblings of a storage's tree, in order (left, self, right).
            let mut pending = vec![child];
            while let Some(i) = pending.pop() {
                if i == NO_STREAM {
                    continue;
                }
                let index = i as usize;
                if index >= self.entries.len() || !valid[index] {
                    return Err(CfbError::Corrupt(format!(
                        "directory entry {i} does not exist"
                    )));
                }
                if visited[index] {
                    return Err(CfbError::Corrupt(format!(
                        "directory entry {i} is reached twice"
                    )));
                }
                visited[index] = true;
                members.push(index);
                pending.push(self.entries[index].left);
                pending.push(self.entries[index].right);
            }
            members.sort_by(|&a, &b| self.entries[a].name.cmp(&self.entries[b].name));
            let mut storages = Vec::new();
            for index in members {
                let path = format!("{parent}/{}", self.entries[index].name);
                out.push((path.clone(), index));
                if self.entries[index].kind == EntryKind::Storage {
                    storages.push((path, self.entries[index].child));
                }
            }
            // Depth first, in name order.
            stack.extend(storages.into_iter().rev());
        }
        // Children right after their storage.
        let mut sorted = out;
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(sorted)
    }

    /// Every entry reachable from the root with its path ("" for the
    /// root, "/RSeStorage/RSeDb" below it), sorted by path.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.paths
            .iter()
            .map(|(p, i)| (p.as_str(), &self.entries[*i]))
    }

    /// The root entry (its class id names the kind of document).
    pub fn root(&self) -> &Entry {
        &self.entries[0]
    }

    /// The entry at a path ("/RSeStorage/RSeDb"); names compare without
    /// case, as the specification has it.
    pub fn find(&self, path: &str) -> Option<&Entry> {
        let path = if path.starts_with('/') || path.is_empty() {
            path.to_string()
        } else {
            format!("/{path}")
        };
        self.paths
            .iter()
            .find(|(p, _)| *p == path)
            .or_else(|| {
                self.paths
                    .iter()
                    .find(|(p, _)| p.to_uppercase() == path.to_uppercase())
            })
            .map(|(_, i)| &self.entries[*i])
    }

    /// The direct children of a storage path, sorted by name.
    pub fn children<'a>(
        &'a self,
        storage: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a Entry)> + 'a {
        let prefix = format!("{storage}/");
        self.entries().filter(move |(p, _)| {
            p.strip_prefix(prefix.as_str())
                .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
        })
    }

    /// The contents of a stream.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>, CfbError> {
        if entry.kind != EntryKind::Stream {
            return Err(CfbError::Corrupt(format!("{} is not a stream", entry.name)));
        }
        if entry.size == 0 {
            return Ok(Vec::new());
        }
        if entry.size > self.data.len() as u64 {
            return Err(CfbError::Corrupt(format!(
                "{} claims {} bytes in a file of {}",
                entry.name,
                entry.size,
                self.data.len()
            )));
        }
        if entry.size < self.mini_cutoff {
            self.mini_chain(entry.start, entry.size)
        } else {
            self.chain(entry.start, Some(entry.size))
        }
    }

    /// The contents of the stream at a path.
    pub fn read_path(&self, path: &str) -> Result<Vec<u8>, CfbError> {
        let entry = self
            .find(path)
            .ok_or_else(|| CfbError::Corrupt(format!("no stream {path}")))?;
        self.read(entry)
    }
}

/// A class id or format id in the registry's text form (the first three
/// fields are little-endian).
pub fn guid_text(id: &[u8; 16]) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        u32::from_le_bytes([id[0], id[1], id[2], id[3]]),
        u16::from_le_bytes([id[4], id[5]]),
        u16::from_le_bytes([id[6], id[7]]),
        id[8],
        id[9],
        id[10],
        id[11],
        id[12],
        id[13],
        id[14],
        id[15]
    )
}

/// A class id from its text form (`guid_text`'s inverse); None when it is
/// not one.
pub fn guid_bytes(text: &str) -> Option<[u8; 16]> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut b = [0u8; 16];
    for (i, byte) in b.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    // The first three fields are stored little-endian.
    b[0..4].reverse();
    b[4..6].reverse();
    b[6..8].reverse();
    Some(b)
}

/// Writes compound files (version 3) for tests: streams below the cutoff
/// go into the mini stream, the others into regular sectors, as the
/// specification requires. The directory is a degenerate tree (each
/// storage's children chained through their right siblings, all black),
/// not the balanced red-black tree the specification asks writers for;
/// the reader walks any tree, and the writer is for tests only.
#[derive(Default)]
pub struct Writer {
    /// (path, data); a path's storages are made as needed.
    streams: Vec<(String, Vec<u8>)>,
    clsids: Vec<(String, [u8; 16])>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    /// A stream at a path like "RSeStorage/RSeDb".
    pub fn stream(&mut self, path: &str, data: &[u8]) -> &mut Self {
        self.streams
            .push((path.trim_start_matches('/').to_string(), data.to_vec()));
        self
    }

    /// The class id of a storage ("" for the root).
    pub fn clsid(&mut self, path: &str, clsid: [u8; 16]) -> &mut Self {
        self.clsids
            .push((path.trim_start_matches('/').to_string(), clsid));
        self
    }

    pub fn finish(&self) -> Vec<u8> {
        const SECTOR: usize = 512;
        const MINI: usize = 64;
        const CUTOFF: usize = 4096;
        const FREE: u32 = 0xFFFF_FFFF;
        const FATSECT: u32 = 0xFFFF_FFFD;

        // The directory tree: index 0 is the root.
        struct Node {
            name: String,
            stream: Option<usize>,
            children: Vec<usize>,
            clsid: [u8; 16],
        }
        let mut nodes = vec![Node {
            name: "Root Entry".into(),
            stream: None,
            children: Vec::new(),
            clsid: [0; 16],
        }];
        let storage_of = |nodes: &mut Vec<Node>, parts: &[&str]| -> usize {
            let mut at = 0;
            for part in parts {
                let found = nodes[at]
                    .children
                    .iter()
                    .copied()
                    .find(|&c| nodes[c].name == *part && nodes[c].stream.is_none());
                at = match found {
                    Some(c) => c,
                    None => {
                        nodes.push(Node {
                            name: (*part).to_string(),
                            stream: None,
                            children: Vec::new(),
                            clsid: [0; 16],
                        });
                        let n = nodes.len() - 1;
                        nodes[at].children.push(n);
                        n
                    }
                };
            }
            at
        };
        for (i, (path, _)) in self.streams.iter().enumerate() {
            let parts: Vec<&str> = path.split('/').collect();
            let parent = storage_of(&mut nodes, &parts[..parts.len() - 1]);
            nodes.push(Node {
                name: parts[parts.len() - 1].to_string(),
                stream: Some(i),
                children: Vec::new(),
                clsid: [0; 16],
            });
            let n = nodes.len() - 1;
            nodes[parent].children.push(n);
        }
        for (path, clsid) in &self.clsids {
            let parts: Vec<&str> = if path.is_empty() {
                Vec::new()
            } else {
                path.split('/').collect()
            };
            let n = storage_of(&mut nodes, &parts);
            nodes[n].clsid = *clsid;
        }

        // Regular sectors: big streams, then the mini stream, the mini
        // FAT, the directory and the FAT. Sector numbers are assigned in
        // that order.
        let mut sectors: Vec<Vec<u8>> = Vec::new();
        let mut fat: Vec<u32> = Vec::new();
        let put = |sectors: &mut Vec<Vec<u8>>, fat: &mut Vec<u32>, data: &[u8]| -> u32 {
            if data.is_empty() {
                return END_OF_CHAIN;
            }
            let first = sectors.len() as u32;
            let count = data.len().div_ceil(SECTOR);
            for k in 0..count {
                let mut s = data[k * SECTOR..((k + 1) * SECTOR).min(data.len())].to_vec();
                s.resize(SECTOR, 0);
                sectors.push(s);
                fat.push(if k + 1 == count {
                    END_OF_CHAIN
                } else {
                    first + k as u32 + 1
                });
            }
            first
        };
        let mut starts = vec![END_OF_CHAIN; self.streams.len()];
        let mut mini_stream = Vec::new();
        let mut mini_fat: Vec<u32> = Vec::new();
        for (i, (_, data)) in self.streams.iter().enumerate() {
            if data.len() >= CUTOFF {
                starts[i] = put(&mut sectors, &mut fat, data);
            } else if !data.is_empty() {
                let first = (mini_stream.len() / MINI) as u32;
                let count = data.len().div_ceil(MINI);
                for k in 0..count {
                    let mut s = data[k * MINI..((k + 1) * MINI).min(data.len())].to_vec();
                    s.resize(MINI, 0);
                    mini_stream.extend_from_slice(&s);
                    mini_fat.push(if k + 1 == count {
                        END_OF_CHAIN
                    } else {
                        first + k as u32 + 1
                    });
                }
                starts[i] = first;
            }
        }
        let mini_start = put(&mut sectors, &mut fat, &mini_stream);
        let mini_fat_bytes: Vec<u8> = mini_fat.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mini_fat_start = put(&mut sectors, &mut fat, &mini_fat_bytes);
        let mini_fat_sectors = mini_fat_bytes.len().div_ceil(SECTOR);

        let mut dir = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            let mut e = vec![0u8; 128];
            let units: Vec<u16> = node.name.encode_utf16().collect();
            for (k, u) in units.iter().enumerate().take(31) {
                e[2 * k..2 * k + 2].copy_from_slice(&u.to_le_bytes());
            }
            e[0x40..0x42].copy_from_slice(&((units.len().min(31) as u16 + 1) * 2).to_le_bytes());
            e[0x42] = match (index, node.stream) {
                (0, _) => 5,
                (_, Some(_)) => 2,
                _ => 1,
            };
            e[0x43] = 1; // black
            // Siblings: each child points right to the next one.
            let parent_children = nodes
                .iter()
                .find(|p| p.children.contains(&index))
                .map(|p| &p.children);
            let right = parent_children
                .and_then(|c| {
                    c.iter()
                        .position(|&x| x == index)
                        .and_then(|p| c.get(p + 1))
                })
                .map_or(NO_STREAM, |&x| x as u32);
            e[0x44..0x48].copy_from_slice(&NO_STREAM.to_le_bytes());
            e[0x48..0x4C].copy_from_slice(&right.to_le_bytes());
            let child = node.children.first().map_or(NO_STREAM, |&c| c as u32);
            e[0x4C..0x50].copy_from_slice(&child.to_le_bytes());
            e[0x50..0x60].copy_from_slice(&node.clsid);
            let (start, size) = match node.stream {
                Some(s) => (starts[s], self.streams[s].1.len() as u64),
                None if index == 0 => (mini_start, mini_stream.len() as u64),
                None => (0, 0),
            };
            e[0x74..0x78].copy_from_slice(&start.to_le_bytes());
            e[0x78..0x80].copy_from_slice(&size.to_le_bytes());
            dir.extend_from_slice(&e);
        }
        // Unused entries of the last directory sector.
        while dir.len() % SECTOR != 0 {
            let mut e = vec![0u8; 128];
            e[0x44..0x50].copy_from_slice(&[0xFF; 12]);
            dir.extend_from_slice(&e);
        }
        let dir_start = put(&mut sectors, &mut fat, &dir);

        // The FAT covers itself: enough sectors for every sector's entry.
        let mut fat_sectors = 1;
        while (sectors.len() + fat_sectors) > fat_sectors * SECTOR / 4 {
            fat_sectors += 1;
        }
        assert!(
            fat_sectors <= 109,
            "test files stay below the header's FAT list"
        );
        let fat_first = sectors.len() as u32;
        fat.extend(std::iter::repeat_n(FATSECT, fat_sectors));
        fat.resize(fat_sectors * SECTOR / 4, FREE);
        let fat_bytes: Vec<u8> = fat.iter().flat_map(|v| v.to_le_bytes()).collect();
        for chunk in fat_bytes.chunks(SECTOR) {
            sectors.push(chunk.to_vec());
        }

        let mut header = vec![0u8; SECTOR];
        header[..8].copy_from_slice(&SIGNATURE);
        header[0x18..0x1A].copy_from_slice(&0x3Eu16.to_le_bytes());
        header[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes());
        header[0x1C..0x1E].copy_from_slice(&0xFFFEu16.to_le_bytes());
        header[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes());
        header[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
        header[0x2C..0x30].copy_from_slice(&(fat_sectors as u32).to_le_bytes());
        header[0x30..0x34].copy_from_slice(&dir_start.to_le_bytes());
        header[0x38..0x3C].copy_from_slice(&(CUTOFF as u32).to_le_bytes());
        header[0x3C..0x40].copy_from_slice(&mini_fat_start.to_le_bytes());
        header[0x40..0x44].copy_from_slice(&(mini_fat_sectors as u32).to_le_bytes());
        header[0x44..0x48].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
        for i in 0..109 {
            let v = if i < fat_sectors {
                fat_first + i as u32
            } else {
                FREE
            };
            header[0x4C + 4 * i..0x50 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        let mut out = header;
        for s in sectors {
            out.extend_from_slice(&s);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_streams_in_storages() {
        let big: Vec<u8> = (0..10_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let small = b"small stream".to_vec();
        let clsid = guid_bytes("4d29b490-49b2-11d0-93c3-7e0706000000").unwrap();
        let mut w = Writer::new();
        w.stream("Top", &small)
            .stream("Store/Inner/Big", &big)
            .stream("Store/Tiny", b"x")
            .stream("Store/Empty", b"")
            .clsid("", clsid);
        let data = w.finish();
        let f = CompoundFile::parse(data).unwrap();
        assert_eq!(f.version, 3);
        assert_eq!(
            guid_text(&f.root().clsid),
            "4d29b490-49b2-11d0-93c3-7e0706000000"
        );
        let paths: Vec<&str> = f.entries().map(|(p, _)| p).collect();
        assert_eq!(
            paths,
            [
                "",
                "/Store",
                "/Store/Empty",
                "/Store/Inner",
                "/Store/Inner/Big",
                "/Store/Tiny",
                "/Top"
            ]
        );
        assert_eq!(f.read_path("/Store/Inner/Big").unwrap(), big);
        assert_eq!(f.read_path("Top").unwrap(), small);
        assert_eq!(f.read_path("/store/tiny").unwrap(), b"x");
        assert!(f.read_path("/Store/Empty").unwrap().is_empty());
        let children: Vec<&str> = f.children("/Store").map(|(p, _)| p).collect();
        assert_eq!(children, ["/Store/Empty", "/Store/Inner", "/Store/Tiny"]);
        assert!(f.read_path("/Store").is_err());
    }

    #[test]
    fn refuses_other_files_and_damage() {
        assert_eq!(
            CompoundFile::parse(vec![0; 600]).err(),
            Some(CfbError::NotCompound)
        );
        let mut w = Writer::new();
        w.stream("A", &[1u8; 5000]);
        let good = w.finish();
        // A FAT that points every sector at itself: the chain loops.
        let mut looping = good.clone();
        let fat_sector = u32_at(&looping, 0x4C) as usize;
        let fat_at = (fat_sector + 1) * 512;
        looping[fat_at..fat_at + 4].copy_from_slice(&0u32.to_le_bytes());
        let f = CompoundFile::parse(looping).unwrap();
        assert!(matches!(f.read_path("A"), Err(CfbError::Corrupt(_))));
        // A truncated file: the directory is gone.
        let truncated = good[..1024].to_vec();
        assert!(CompoundFile::parse(truncated).is_err());
        // A wrong sector size for the version.
        let mut bad = good;
        bad[0x1E] = 12;
        assert!(matches!(CompoundFile::parse(bad), Err(CfbError::Header(_))));
    }

    #[test]
    fn guids_round_trip() {
        let text = "32853f0f-3444-11d1-9e93-0060b03c1ca6";
        assert_eq!(guid_text(&guid_bytes(text).unwrap()), text);
        assert_eq!(guid_bytes("nope"), None);
    }
}
