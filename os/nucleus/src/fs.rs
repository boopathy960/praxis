//! The filesystem — a from-scratch inode VFS with a RAM-backed store.
//!
//! This is the layer an OS lives or dies on: a hierarchical namespace of
//! directories and files, path resolution, file descriptors with byte
//! offsets, and the read/write/create/mkdir/unlink/stat operations userland
//! actually calls. It is deliberately structured like a real Unix VFS —
//! numbered inodes, a directory being a map of name → inode, descriptors
//! decoupled from inodes — so a disk-backed filesystem could be dropped in
//! under the same interface later.
//!
//! Persistence closes the loop with the [block layer](crate::block):
//! [`Vfs::snapshot`] serializes the whole tree onto a [`BlockDevice`] and
//! [`Vfs::restore`] reads it back — the filesystem survives power loss exactly
//! as it would on the SPU's non-volatile tier.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::block::{BlockDevice, BlockError, BLOCK_SIZE};

pub type Ino = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Dir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    NotADirectory,
    IsADirectory,
    AlreadyExists,
    BadPath,
    BadDescriptor,
    DirectoryNotEmpty,
    CorruptImage,
    DeviceTooSmall,
}

#[derive(Debug, Clone)]
struct Inode {
    kind: NodeKind,
    /// File contents (empty for directories).
    data: Vec<u8>,
    /// Directory entries: name → child inode (empty for files).
    entries: BTreeMap<String, Ino>,
}

impl Inode {
    fn dir() -> Self {
        Self {
            kind: NodeKind::Dir,
            data: Vec::new(),
            entries: BTreeMap::new(),
        }
    }
    fn file() -> Self {
        Self {
            kind: NodeKind::File,
            data: Vec::new(),
            entries: BTreeMap::new(),
        }
    }
}

/// An open file descriptor: an inode plus a byte cursor. Decoupled from the
/// inode so two descriptors can hold independent offsets on the same file.
#[derive(Debug, Clone, Copy)]
struct Descriptor {
    ino: Ino,
    offset: usize,
    writable: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Metadata {
    pub kind: NodeKind,
    pub size: usize,
    pub ino: Ino,
}

/// The virtual filesystem. Root is always inode 1.
pub struct Vfs {
    nodes: BTreeMap<Ino, Inode>,
    next_ino: Ino,
    fds: BTreeMap<u64, Descriptor>,
    next_fd: u64,
}

const ROOT: Ino = 1;

impl Vfs {
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(ROOT, Inode::dir());
        Self {
            nodes,
            next_ino: 2,
            fds: BTreeMap::new(),
            next_fd: 3, // 0/1/2 reserved for the console (stdin/out/err)
        }
    }

    // ── path resolution ─────────────────────────────────────────────────

    fn components(path: &str) -> Result<Vec<&str>, FsError> {
        if !path.starts_with('/') {
            return Err(FsError::BadPath);
        }
        Ok(path.split('/').filter(|c| !c.is_empty()).collect())
    }

    fn resolve(&self, path: &str) -> Result<Ino, FsError> {
        let mut ino = ROOT;
        for name in Self::components(path)? {
            let node = self.nodes.get(&ino).ok_or(FsError::NotFound)?;
            if node.kind != NodeKind::Dir {
                return Err(FsError::NotADirectory);
            }
            ino = *node.entries.get(name).ok_or(FsError::NotFound)?;
        }
        Ok(ino)
    }

    /// Split a path into (parent inode, final component).
    fn resolve_parent<'a>(&self, path: &'a str) -> Result<(Ino, &'a str), FsError> {
        let comps = Self::components(path)?;
        let (last, parents) = comps.split_last().ok_or(FsError::BadPath)?;
        let mut ino = ROOT;
        for name in parents {
            let node = self.nodes.get(&ino).ok_or(FsError::NotFound)?;
            if node.kind != NodeKind::Dir {
                return Err(FsError::NotADirectory);
            }
            ino = *node.entries.get(*name).ok_or(FsError::NotFound)?;
        }
        Ok((ino, last))
    }

    // ── namespace operations ────────────────────────────────────────────

    /// Create a directory (parent must exist).
    pub fn mkdir(&mut self, path: &str) -> Result<Ino, FsError> {
        self.create_node(path, NodeKind::Dir)
    }

    /// Create an empty file (parent must exist).
    pub fn create(&mut self, path: &str) -> Result<Ino, FsError> {
        self.create_node(path, NodeKind::File)
    }

    fn create_node(&mut self, path: &str, kind: NodeKind) -> Result<Ino, FsError> {
        let (parent, name) = self.resolve_parent(path)?;
        if name.is_empty() {
            return Err(FsError::BadPath);
        }
        let parent_node = self.nodes.get(&parent).ok_or(FsError::NotFound)?;
        if parent_node.kind != NodeKind::Dir {
            return Err(FsError::NotADirectory);
        }
        if parent_node.entries.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }
        let ino = self.next_ino;
        self.next_ino += 1;
        self.nodes.insert(
            ino,
            match kind {
                NodeKind::Dir => Inode::dir(),
                NodeKind::File => Inode::file(),
            },
        );
        self.nodes
            .get_mut(&parent)
            .expect("parent verified")
            .entries
            .insert(name.to_string(), ino);
        Ok(ino)
    }

    /// List a directory's entries (sorted).
    pub fn readdir(&self, path: &str) -> Result<Vec<String>, FsError> {
        let ino = self.resolve(path)?;
        let node = self.nodes.get(&ino).ok_or(FsError::NotFound)?;
        if node.kind != NodeKind::Dir {
            return Err(FsError::NotADirectory);
        }
        Ok(node.entries.keys().cloned().collect())
    }

    pub fn stat(&self, path: &str) -> Result<Metadata, FsError> {
        let ino = self.resolve(path)?;
        let node = self.nodes.get(&ino).ok_or(FsError::NotFound)?;
        Ok(Metadata {
            kind: node.kind,
            size: node.data.len(),
            ino,
        })
    }

    /// Remove a file or an empty directory.
    pub fn unlink(&mut self, path: &str) -> Result<(), FsError> {
        let (parent, name) = self.resolve_parent(path)?;
        let ino = *self
            .nodes
            .get(&parent)
            .ok_or(FsError::NotFound)?
            .entries
            .get(name)
            .ok_or(FsError::NotFound)?;
        let node = self.nodes.get(&ino).ok_or(FsError::NotFound)?;
        if node.kind == NodeKind::Dir && !node.entries.is_empty() {
            return Err(FsError::DirectoryNotEmpty);
        }
        self.nodes.remove(&ino);
        self.nodes
            .get_mut(&parent)
            .expect("parent verified")
            .entries
            .remove(name);
        Ok(())
    }

    // ── descriptors & IO ────────────────────────────────────────────────

    /// Open a path, optionally creating it. Returns a file descriptor.
    pub fn open(&mut self, path: &str, create: bool, writable: bool) -> Result<u64, FsError> {
        let ino = match self.resolve(path) {
            Ok(ino) => ino,
            Err(FsError::NotFound) if create => self.create(path)?,
            Err(e) => return Err(e),
        };
        if self.nodes.get(&ino).ok_or(FsError::NotFound)?.kind == NodeKind::Dir {
            return Err(FsError::IsADirectory);
        }
        let fd = self.next_fd;
        self.next_fd += 1;
        self.fds.insert(
            fd,
            Descriptor {
                ino,
                offset: 0,
                writable,
            },
        );
        Ok(fd)
    }

    pub fn close(&mut self, fd: u64) -> Result<(), FsError> {
        self.fds
            .remove(&fd)
            .map(|_| ())
            .ok_or(FsError::BadDescriptor)
    }

    /// Read up to `buf.len()` bytes from the descriptor's cursor; returns the
    /// count read and advances the cursor.
    pub fn read(&mut self, fd: u64, buf: &mut [u8]) -> Result<usize, FsError> {
        let desc = *self.fds.get(&fd).ok_or(FsError::BadDescriptor)?;
        let node = self.nodes.get(&desc.ino).ok_or(FsError::NotFound)?;
        let available = node.data.len().saturating_sub(desc.offset);
        let n = available.min(buf.len());
        buf[..n].copy_from_slice(&node.data[desc.offset..desc.offset + n]);
        self.fds.get_mut(&fd).expect("fd verified").offset += n;
        Ok(n)
    }

    /// Write `bytes` at the descriptor's cursor (extending the file as needed).
    pub fn write(&mut self, fd: u64, bytes: &[u8]) -> Result<usize, FsError> {
        let desc = *self.fds.get(&fd).ok_or(FsError::BadDescriptor)?;
        if !desc.writable {
            return Err(FsError::BadDescriptor);
        }
        let node = self.nodes.get_mut(&desc.ino).ok_or(FsError::NotFound)?;
        let end = desc.offset + bytes.len();
        if node.data.len() < end {
            node.data.resize(end, 0);
        }
        node.data[desc.offset..end].copy_from_slice(bytes);
        self.fds.get_mut(&fd).expect("fd verified").offset = end;
        Ok(bytes.len())
    }

    /// Seek a descriptor to an absolute offset.
    pub fn seek(&mut self, fd: u64, offset: usize) -> Result<(), FsError> {
        self.fds.get_mut(&fd).ok_or(FsError::BadDescriptor)?.offset = offset;
        Ok(())
    }

    /// Whole-file convenience read (used by the shell's `cat`).
    pub fn read_all(&self, path: &str) -> Result<Vec<u8>, FsError> {
        let ino = self.resolve(path)?;
        let node = self.nodes.get(&ino).ok_or(FsError::NotFound)?;
        if node.kind == NodeKind::Dir {
            return Err(FsError::IsADirectory);
        }
        Ok(node.data.clone())
    }

    /// Whole-file convenience write, creating/truncating (the shell's `write`).
    pub fn write_all(&mut self, path: &str, bytes: &[u8]) -> Result<(), FsError> {
        let ino = match self.resolve(path) {
            Ok(ino) => ino,
            Err(FsError::NotFound) => self.create(path)?,
            Err(e) => return Err(e),
        };
        let node = self.nodes.get_mut(&ino).ok_or(FsError::NotFound)?;
        if node.kind == NodeKind::Dir {
            return Err(FsError::IsADirectory);
        }
        node.data = bytes.to_vec();
        Ok(())
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn open_fds(&self) -> usize {
        self.fds.len()
    }

    // ── persistence: snapshot to / restore from a block device ──────────
    //
    // A compact, self-describing image format. Not a production on-disk FS
    // (no free-list, no journaling) — the honest job here is to demonstrate a
    // real path from the in-memory tree to durable blocks and back:
    //
    //   [magic u32][next_ino u64][node_count u64]
    //   per node: [ino u64][kind u8][len u64][payload len bytes]
    //     file payload = raw bytes
    //     dir  payload = repeated [name_len u16][name][child_ino u64]

    const MAGIC: u32 = 0x5350_5546; // "SPUF"

    /// Serialize the whole tree to a self-describing image (magic + inode
    /// table). Shared by the plain and journaled snapshot paths.
    fn serialize(&self) -> Vec<u8> {
        let mut image: Vec<u8> = Vec::new();
        image.extend_from_slice(&Self::MAGIC.to_le_bytes());
        image.extend_from_slice(&self.next_ino.to_le_bytes());
        image.extend_from_slice(&(self.nodes.len() as u64).to_le_bytes());
        for (ino, node) in &self.nodes {
            image.extend_from_slice(&ino.to_le_bytes());
            let (kind_byte, payload) = match node.kind {
                NodeKind::File => (0u8, node.data.clone()),
                NodeKind::Dir => {
                    let mut p = Vec::new();
                    for (name, child) in &node.entries {
                        p.extend_from_slice(&(name.len() as u16).to_le_bytes());
                        p.extend_from_slice(name.as_bytes());
                        p.extend_from_slice(&child.to_le_bytes());
                    }
                    (1u8, p)
                }
            };
            image.push(kind_byte);
            image.extend_from_slice(&(payload.len() as u64).to_le_bytes());
            image.extend_from_slice(&payload);
        }
        image
    }

    /// Serialize the whole tree onto `dev`, starting at block 0.
    pub fn snapshot(&self, dev: &mut dyn BlockDevice) -> Result<u64, FsError> {
        let image = self.serialize();
        Self::write_image(dev, &image)?;
        Ok(image.len() as u64)
    }

    /// Rebuild a filesystem from an image previously written by [`snapshot`].
    pub fn restore(dev: &dyn BlockDevice) -> Result<Self, FsError> {
        let image = Self::read_image(dev)?;
        Self::deserialize(&image)
    }

    /// Parse a serialized image back into a live tree.
    fn deserialize(image: &[u8]) -> Result<Self, FsError> {
        let mut cur = Cursor::new(image);
        if cur.u32()? != Self::MAGIC {
            return Err(FsError::CorruptImage);
        }
        let next_ino = cur.u64()?;
        let count = cur.u64()?;
        let mut nodes = BTreeMap::new();
        for _ in 0..count {
            let ino = cur.u64()?;
            let kind_byte = cur.u8()?;
            let len = cur.u64()? as usize;
            let payload = cur.bytes(len)?;
            let node = match kind_byte {
                0 => Inode {
                    kind: NodeKind::File,
                    data: payload.to_vec(),
                    entries: BTreeMap::new(),
                },
                1 => {
                    let mut entries = BTreeMap::new();
                    let mut pc = Cursor::new(payload);
                    while pc.remaining() > 0 {
                        let name_len = pc.u16()? as usize;
                        let name = String::from_utf8(pc.bytes(name_len)?.to_vec())
                            .map_err(|_| FsError::CorruptImage)?;
                        let child = pc.u64()?;
                        entries.insert(name, child);
                    }
                    Inode {
                        kind: NodeKind::Dir,
                        data: Vec::new(),
                        entries,
                    }
                }
                _ => return Err(FsError::CorruptImage),
            };
            nodes.insert(ino, node);
        }
        if !nodes.contains_key(&ROOT) {
            return Err(FsError::CorruptImage);
        }
        Ok(Self {
            nodes,
            next_ino,
            fds: BTreeMap::new(),
            next_fd: 3,
        })
    }

    // ── crash-safe (journaled) snapshot ────────────────────────────────
    //
    // The plain `snapshot` overwrites the image in place: a power loss partway
    // through leaves block 0's length header describing more blocks than were
    // actually written, and `restore` reads garbage. The journaled path is
    // write-ahead and atomic-commit:
    //
    //   block 0, 1 : two alternating COMMIT records (generation, active slot,
    //                image length, image checksum, self checksum)
    //   slot 0     : blocks 2 .. 2+S
    //   slot 1     : blocks 2+S .. 2+2S
    //
    // `snapshot_journaled` writes the new image to the *inactive* slot, then
    // writes a fresh commit record (generation+1) to the older of the two
    // commit blocks. The commit block write is the atomic commit point — a
    // single sector. `restore_journaled` picks the highest-generation commit
    // whose self checksum *and* image checksum both verify, so a torn image
    // write or a torn commit is detected and the last good image is recovered.

    const JOURNAL_MAGIC: u32 = 0x574A_4C31; // "WJL1"
    const COMMIT_A: u64 = 0;
    const COMMIT_B: u64 = 1;
    const SLOT_BASE: u64 = 2;

    /// Blocks per slot for a device of `num_blocks` (two commit blocks + two
    /// equal slots).
    fn slot_span(num_blocks: u64) -> u64 {
        num_blocks.saturating_sub(Self::SLOT_BASE) / 2
    }

    /// Crash-safe snapshot. Returns the image byte length written.
    pub fn snapshot_journaled(&self, dev: &mut dyn BlockDevice) -> Result<u64, FsError> {
        let image = self.serialize();
        let span = Self::slot_span(dev.num_blocks());
        if span == 0 || image.len() as u64 > span * BLOCK_SIZE as u64 {
            return Err(FsError::DeviceTooSmall);
        }
        // Find the current best commit to learn the active slot + generation.
        let current = Self::read_best_commit(dev);
        let (active_slot, generation) =
            current.map_or((1u8, 0u64), |c| (c.active_slot, c.generation));
        let write_slot = 1 - active_slot; // write to the INACTIVE slot
        let slot_block = Self::SLOT_BASE + u64::from(write_slot) * span;

        // 1. Write the image to the inactive slot (safe: nothing points here yet).
        let mut block = [0u8; BLOCK_SIZE];
        for (i, chunk) in image.chunks(BLOCK_SIZE).enumerate() {
            block.fill(0);
            block[..chunk.len()].copy_from_slice(chunk);
            dev.write_block(slot_block + i as u64, &block)
                .map_err(|_| FsError::CorruptImage)?;
        }
        // 2. Atomic commit: a fresh record to the *older* commit block.
        let commit_block = if current.is_some_and(|c| c.which == Self::COMMIT_A) {
            Self::COMMIT_B
        } else {
            Self::COMMIT_A
        };
        let record = Commit {
            generation: generation + 1,
            active_slot: write_slot,
            image_len: image.len() as u64,
            image_checksum: checksum32(&image),
            which: commit_block,
        };
        dev.write_block(commit_block, &record.encode())
            .map_err(|_| FsError::CorruptImage)?;
        Ok(image.len() as u64)
    }

    /// Restore from a journaled snapshot, recovering the last *fully committed*
    /// image even if the most recent write was interrupted.
    pub fn restore_journaled(dev: &dyn BlockDevice) -> Result<Self, FsError> {
        // Try commits by generation (best first); accept the first whose slot
        // image checksum verifies.
        let mut commits = [
            Self::read_commit(dev, Self::COMMIT_A),
            Self::read_commit(dev, Self::COMMIT_B),
        ];
        commits.sort_by_key(|c| core::cmp::Reverse(c.map_or(0, |c| c.generation)));
        let span = Self::slot_span(dev.num_blocks());
        for commit in commits.into_iter().flatten() {
            if let Ok(image) = Self::read_slot(dev, commit, span) {
                if checksum32(&image) == commit.image_checksum {
                    return Self::deserialize(&image);
                }
            }
        }
        Err(FsError::CorruptImage)
    }

    fn read_slot(dev: &dyn BlockDevice, commit: Commit, span: u64) -> Result<Vec<u8>, FsError> {
        let slot_block = Self::SLOT_BASE + u64::from(commit.active_slot) * span;
        let need = (commit.image_len as usize).div_ceil(BLOCK_SIZE);
        if need as u64 > span {
            return Err(FsError::CorruptImage);
        }
        let mut out = Vec::with_capacity(commit.image_len as usize);
        let mut block = [0u8; BLOCK_SIZE];
        for i in 0..need as u64 {
            dev.read_block(slot_block + i, &mut block)
                .map_err(|_| FsError::CorruptImage)?;
            out.extend_from_slice(&block);
        }
        out.truncate(commit.image_len as usize);
        Ok(out)
    }

    fn read_commit(dev: &dyn BlockDevice, which: u64) -> Option<Commit> {
        let mut block = [0u8; BLOCK_SIZE];
        dev.read_block(which, &mut block).ok()?;
        Commit::decode(&block, which)
    }

    fn read_best_commit(dev: &dyn BlockDevice) -> Option<Commit> {
        let a = Self::read_commit(dev, Self::COMMIT_A);
        let b = Self::read_commit(dev, Self::COMMIT_B);
        match (a, b) {
            (Some(a), Some(b)) => Some(if a.generation >= b.generation { a } else { b }),
            (x, y) => x.or(y),
        }
    }

    /// Length-prefix the image so restore reads exactly what snapshot wrote,
    /// then pack it across as many blocks as needed.
    fn write_image(dev: &mut dyn BlockDevice, image: &[u8]) -> Result<(), FsError> {
        let mut framed = Vec::with_capacity(image.len() + 8);
        framed.extend_from_slice(&(image.len() as u64).to_le_bytes());
        framed.extend_from_slice(image);
        let needed = framed.len().div_ceil(BLOCK_SIZE) as u64;
        if needed > dev.num_blocks() {
            return Err(FsError::DeviceTooSmall);
        }
        let mut block = [0u8; BLOCK_SIZE];
        for (i, chunk) in framed.chunks(BLOCK_SIZE).enumerate() {
            block.fill(0);
            block[..chunk.len()].copy_from_slice(chunk);
            dev.write_block(i as u64, &block)
                .map_err(|_| FsError::CorruptImage)?;
        }
        Ok(())
    }

    fn read_image(dev: &dyn BlockDevice) -> Result<Vec<u8>, FsError> {
        let mut block = [0u8; BLOCK_SIZE];
        dev.read_block(0, &mut block)
            .map_err(|_| FsError::CorruptImage)?;
        let len = u64::from_le_bytes(block[..8].try_into().unwrap()) as usize;
        let mut out = Vec::with_capacity(len);
        out.extend_from_slice(&block[8..]);
        let mut idx = 1u64;
        while out.len() < len {
            dev.read_block(idx, &mut block).map_err(|e| match e {
                BlockError::OutOfRange => FsError::CorruptImage,
                BlockError::BadLength => FsError::CorruptImage,
            })?;
            out.extend_from_slice(&block);
            idx += 1;
        }
        out.truncate(len);
        Ok(out)
    }
}

impl Default for Vfs {
    fn default() -> Self {
        Self::new()
    }
}

/// A journal commit record: which slot holds the latest fully-written image,
/// with checksums so a torn write of either the image or this record is
/// detected. Fits in one 512-byte sector (the atomic write unit).
#[derive(Debug, Clone, Copy)]
struct Commit {
    generation: u64,
    active_slot: u8,
    image_len: u64,
    image_checksum: u32,
    /// Which commit block this was read from (0 or 1) — not serialized.
    which: u64,
}

impl Commit {
    fn encode(&self) -> [u8; BLOCK_SIZE] {
        let mut b = [0u8; BLOCK_SIZE];
        b[0..4].copy_from_slice(&Vfs::JOURNAL_MAGIC.to_le_bytes());
        b[4..12].copy_from_slice(&self.generation.to_le_bytes());
        b[12] = self.active_slot;
        b[13..21].copy_from_slice(&self.image_len.to_le_bytes());
        b[21..25].copy_from_slice(&self.image_checksum.to_le_bytes());
        // Self checksum over the meaningful prefix, so a torn record fails.
        let self_ck = checksum32(&b[0..25]);
        b[25..29].copy_from_slice(&self_ck.to_le_bytes());
        b
    }

    fn decode(b: &[u8], which: u64) -> Option<Self> {
        if b.len() < 29 || u32::from_le_bytes(b[0..4].try_into().ok()?) != Vfs::JOURNAL_MAGIC {
            return None;
        }
        let stored_ck = u32::from_le_bytes(b[25..29].try_into().ok()?);
        if checksum32(&b[0..25]) != stored_ck {
            return None; // torn / partially-written commit record
        }
        Some(Self {
            generation: u64::from_le_bytes(b[4..12].try_into().ok()?),
            active_slot: b[12],
            image_len: u64::from_le_bytes(b[13..21].try_into().ok()?),
            image_checksum: u32::from_le_bytes(b[21..25].try_into().ok()?),
            which,
        })
    }
}

/// A fast, dependency-free 32-bit checksum (FNV-1a) for journal integrity.
fn checksum32(data: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in data {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// A tiny little-endian byte reader for image parsing.
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], FsError> {
        if self.pos + n > self.buf.len() {
            return Err(FsError::CorruptImage);
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, FsError> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, FsError> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, FsError> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, FsError> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::RamDisk;

    #[test]
    fn directory_tree_and_path_resolution() {
        let mut fs = Vfs::new();
        fs.mkdir("/etc").unwrap();
        fs.mkdir("/etc/agents").unwrap();
        fs.create("/etc/agents/researcher.ctx").unwrap();
        assert_eq!(fs.readdir("/").unwrap(), alloc::vec!["etc"]);
        assert_eq!(
            fs.readdir("/etc/agents").unwrap(),
            alloc::vec!["researcher.ctx"]
        );
        assert_eq!(fs.stat("/etc/agents").unwrap().kind, NodeKind::Dir);

        // Errors are precise.
        assert_eq!(fs.mkdir("/etc").unwrap_err(), FsError::AlreadyExists);
        assert_eq!(fs.readdir("/nope").unwrap_err(), FsError::NotFound);
        assert_eq!(fs.create("no-slash").unwrap_err(), FsError::BadPath);
        assert_eq!(
            fs.readdir("/etc/agents/researcher.ctx").unwrap_err(),
            FsError::NotADirectory
        );
    }

    #[test]
    fn file_io_with_offsets() {
        let mut fs = Vfs::new();
        let fd = fs.open("/log", true, true).unwrap();
        assert_eq!(fs.write(fd, b"hello ").unwrap(), 6);
        assert_eq!(fs.write(fd, b"praxis").unwrap(), 6);
        assert_eq!(fs.stat("/log").unwrap().size, 12);

        // A fresh descriptor reads from the top.
        let rfd = fs.open("/log", false, false).unwrap();
        let mut buf = [0u8; 5];
        assert_eq!(fs.read(rfd, &mut buf).unwrap(), 5);
        assert_eq!(&buf, b"hello");
        fs.seek(rfd, 6).unwrap();
        let mut rest = [0u8; 16];
        let n = fs.read(rfd, &mut rest).unwrap();
        assert_eq!(&rest[..n], b"praxis");

        // Read-only descriptor cannot write.
        assert!(fs.write(rfd, b"x").is_err());
    }

    #[test]
    fn unlink_respects_nonempty_directories() {
        let mut fs = Vfs::new();
        fs.mkdir("/d").unwrap();
        fs.create("/d/f").unwrap();
        assert_eq!(fs.unlink("/d").unwrap_err(), FsError::DirectoryNotEmpty);
        fs.unlink("/d/f").unwrap();
        fs.unlink("/d").unwrap();
        assert_eq!(fs.readdir("/").unwrap().len(), 0);
    }

    #[test]
    fn snapshot_survives_a_power_cycle() {
        let mut fs = Vfs::new();
        fs.mkdir("/etc").unwrap();
        fs.write_all("/etc/motd", b"proof buys speed").unwrap();
        fs.write_all("/etc/agents.list", b"researcher\narchivist")
            .unwrap();
        fs.mkdir("/empty").unwrap();

        // Persist to the "NV tier", then simulate power loss by dropping fs.
        let mut disk = RamDisk::new(256);
        let written = fs.snapshot(&mut disk).unwrap();
        assert!(written > 0);
        drop(fs);

        // Cold boot: rebuild purely from blocks.
        let restored = Vfs::restore(&disk).unwrap();
        assert_eq!(restored.read_all("/etc/motd").unwrap(), b"proof buys speed");
        assert_eq!(restored.stat("/etc/agents.list").unwrap().size, 20);
        assert_eq!(restored.readdir("/").unwrap(), alloc::vec!["empty", "etc"]);
        assert_eq!(restored.readdir("/empty").unwrap().len(), 0);
    }

    #[test]
    fn journaled_snapshot_round_trips() {
        let mut fs = Vfs::new();
        fs.mkdir("/etc").unwrap();
        fs.write_all("/etc/motd", b"journaled and durable").unwrap();
        let mut disk = RamDisk::new(2048);
        fs.snapshot_journaled(&mut disk).unwrap();
        let restored = Vfs::restore_journaled(&disk).unwrap();
        assert_eq!(
            restored.read_all("/etc/motd").unwrap(),
            b"journaled and durable"
        );
    }

    #[test]
    fn journaled_restore_recovers_last_good_image_after_a_torn_write() {
        let mut disk = RamDisk::new(2048);
        let mut fs = Vfs::new();
        fs.mkdir("/etc").unwrap();
        fs.write_all("/etc/a", b"generation one").unwrap();
        fs.snapshot_journaled(&mut disk).unwrap(); // gen 1 → slot 0

        fs.write_all("/etc/a", b"generation two, more content")
            .unwrap();
        fs.snapshot_journaled(&mut disk).unwrap(); // gen 2 → slot 1

        // Simulate a power loss that tore the gen-2 image write: clobber a
        // block of slot 1. The gen-2 commit exists, but its image no longer
        // checksums — restore must fall back to the fully-committed gen 1.
        let span = Vfs::slot_span(disk.num_blocks());
        let slot1 = Vfs::SLOT_BASE + span;
        disk.write_block(slot1, &[0xAA; BLOCK_SIZE]).unwrap();

        let restored = Vfs::restore_journaled(&disk).unwrap();
        assert_eq!(
            restored.read_all("/etc/a").unwrap(),
            b"generation one",
            "torn latest write → recover the previous committed image"
        );
    }

    #[test]
    fn journaled_restore_ignores_a_torn_commit_record() {
        let mut disk = RamDisk::new(2048);
        let mut fs = Vfs::new();
        fs.write_all("/x", b"committed").unwrap();
        fs.snapshot_journaled(&mut disk).unwrap(); // gen 1, COMMIT_A

        fs.write_all("/x", b"newer but its commit is torn").unwrap();
        fs.snapshot_journaled(&mut disk).unwrap(); // gen 2, COMMIT_B

        // A torn commit record for gen 2 (a single bit-flip fails its checksum).
        let mut commit_b = [0u8; BLOCK_SIZE];
        disk.read_block(Vfs::COMMIT_B, &mut commit_b).unwrap();
        commit_b[8] ^= 0xFF;
        disk.write_block(Vfs::COMMIT_B, &commit_b).unwrap();

        // The gen-2 commit is not trustworthy → fall back to gen 1.
        let restored = Vfs::restore_journaled(&disk).unwrap();
        assert_eq!(restored.read_all("/x").unwrap(), b"committed");
    }

    #[test]
    fn device_too_small_is_reported() {
        let mut fs = Vfs::new();
        for i in 0..200 {
            fs.write_all(&alloc::format!("/f{i}"), &[0u8; 64]).unwrap();
        }
        let mut tiny = RamDisk::new(2); // 1 KiB — far too small
        assert_eq!(fs.snapshot(&mut tiny).unwrap_err(), FsError::DeviceTooSmall);
    }
}
