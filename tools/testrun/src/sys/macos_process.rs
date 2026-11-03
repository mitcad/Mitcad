// SPDX-License-Identifier: MIT
//! Sample the resident memory of a test's process group and descendants.
//! libproc needs no task port or elevated privileges for our own children.
//! This is sampled RSS (shared pages may be counted twice), not a kernel
//! memory limit. Very short-lived children can end between samples.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::{size_of, zeroed};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Identity {
    pid: u32,
    start: (u64, u64),
}

#[derive(Clone, Copy)]
struct Entry {
    id: Identity,
    parent: u32,
    group: u32,
    uid: u32,
    zombie: bool,
}

fn entry(pid: i32) -> Option<Entry> {
    let mut info: libc::proc_bsdinfo = unsafe { zeroed() };
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            std::ptr::from_mut(&mut info).cast(),
            size,
        )
    };
    (read == size).then_some(Entry {
        id: Identity {
            pid: info.pbi_pid,
            start: (info.pbi_start_tvsec, info.pbi_start_tvusec),
        },
        parent: info.pbi_ppid,
        group: info.pbi_pgid,
        uid: info.pbi_uid,
        zombie: info.pbi_status == libc::SZOMB,
    })
}

fn entries() -> Option<BTreeMap<u32, Entry>> {
    // PROC_ALL_PIDS. proc_listpids returns bytes, unlike proc_listallpids.
    const ALL: u32 = 1;
    let needed = unsafe { libc::proc_listpids(ALL, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return None;
    }
    let mut capacity = needed as usize / size_of::<i32>() + 256;
    for _ in 0..4 {
        let mut pids = vec![0i32; capacity];
        let bytes = (pids.len() * size_of::<i32>()) as i32;
        let read = unsafe { libc::proc_listpids(ALL, 0, pids.as_mut_ptr().cast(), bytes) };
        if read <= 0 {
            return None;
        }
        if read >= bytes {
            capacity *= 2;
            continue;
        }
        pids.truncate(read as usize / size_of::<i32>());
        return Some(
            pids.into_iter()
                .filter(|p| *p > 0)
                .filter_map(entry)
                .map(|e| (e.id.pid, e))
                .collect(),
        );
    }
    None
}

fn owned(
    root: Entry,
    known: &BTreeSet<Identity>,
    rows: &BTreeMap<u32, Entry>,
) -> Option<BTreeSet<Identity>> {
    if rows.get(&root.id.pid)?.id != root.id {
        return None;
    }
    let mut found: BTreeSet<Identity> = rows
        .values()
        .filter(|e| {
            e.uid == root.uid
                && (e.id == root.id
                    || known.contains(&e.id)
                    || (e.group == root.id.pid && e.id.start >= root.id.start))
        })
        .map(|e| e.id)
        .collect();
    loop {
        let before = found.len();
        for e in rows.values().filter(|e| e.uid == root.uid) {
            if let Some(parent) = rows.get(&e.parent)
                && found.contains(&parent.id)
                && e.id.start >= parent.id.start
            {
                found.insert(e.id);
            }
        }
        if found.len() == before {
            return Some(found);
        }
    }
}

pub struct Tree {
    root: Entry,
    known: BTreeSet<Identity>,
}

impl Tree {
    pub fn new(pid: i32) -> Option<Self> {
        Some(Self {
            root: entry(pid)?,
            known: BTreeSet::new(),
        })
    }

    pub fn sample(&mut self) -> Option<u64> {
        let rows = entries()?;
        for id in &self.known {
            if !rows.contains_key(&id.pid) && unsafe { libc::kill(id.pid as i32, 0) } == 0 {
                // A known live member was unreadable, not demonstrably
                // gone. Keep its predicted growth reserved this round.
                return None;
            }
        }
        let found = owned(self.root, &self.known, &rows)?;
        // Keep identities even if an intermediate sample misses a child.
        // A PID reused by another process never inherits that ownership.
        self.known.extend(found.iter().copied());
        let mut total = 0u64;
        for id in found {
            if rows[&id.pid].zombie {
                continue;
            }
            let mut task: libc::proc_taskinfo = unsafe { zeroed() };
            let size = size_of::<libc::proc_taskinfo>() as i32;
            let read = unsafe {
                libc::proc_pidinfo(
                    id.pid as i32,
                    libc::PROC_PIDTASKINFO,
                    0,
                    std::ptr::from_mut(&mut task).cast(),
                    size,
                )
            };
            let after = entry(id.pid as i32);
            match after {
                // Exited/reused while sampling: its RSS no longer belongs
                // to this live tree. Never read a replacement's memory.
                None if unsafe { libc::kill(id.pid as i32, 0) } == 0 => return None,
                None => continue,
                Some(e) if e.id != id || e.zombie => continue,
                Some(_) if read != size => return None,
                Some(_) => total = total.saturating_add(task.pti_resident_size),
            }
        }
        Some(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(pid: u32, parent: u32, group: u32, start: u64) -> Entry {
        Entry {
            id: Identity {
                pid,
                start: (start, 0),
            },
            parent,
            group,
            uid: 501,
            zombie: false,
        }
    }
    #[test]
    fn descendants_groups_orphans_and_pid_reuse() {
        let root = row(20, 1, 20, 100);
        let child = row(21, 20, 20, 101);
        let detached = row(22, 21, 22, 102);
        let peer = row(23, 1, 23, 90);
        let rows = [root, child, detached, peer]
            .into_iter()
            .map(|e| (e.id.pid, e))
            .collect();
        let found = owned(root, &BTreeSet::new(), &rows).unwrap();
        assert_eq!(
            found,
            [root.id, child.id, detached.id].into_iter().collect()
        );
        let orphan = Entry {
            parent: 1,
            ..detached
        };
        let reused = row(21, 1, 21, 200);
        let rows = [root, reused, orphan, peer]
            .into_iter()
            .map(|e| (e.id.pid, e))
            .collect();
        assert_eq!(
            owned(root, &found, &rows).unwrap(),
            [root.id, orphan.id].into_iter().collect()
        );
        let rows = [(20, row(20, 1, 20, 300))].into_iter().collect();
        assert!(owned(root, &found, &rows).is_none());
    }
}
