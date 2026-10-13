use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Content-addressed, zstd-compressed body storage.
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn new(root: PathBuf) -> std::io::Result<BlobStore> {
        fs::create_dir_all(&root)?;
        Ok(BlobStore { root })
    }

    fn path(&self, hash: &str) -> PathBuf {
        self.root.join(&hash[..2]).join(&hash[2..])
    }

    /// Store `bytes`, returning its hash, or `None` for an empty body.
    pub fn put(&self, bytes: &[u8]) -> std::io::Result<Option<String>> {
        if bytes.is_empty() {
            return Ok(None);
        }
        let hash = blake3::hash(bytes).to_hex().to_string();
        let path = self.path(&hash);
        if path.exists() {
            return Ok(Some(hash));
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let packed = zstd::encode_all(bytes, 3)?;
        let tmp = path.with_extension("tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&packed)?;
        }
        fs::rename(&tmp, &path)?;
        Ok(Some(hash))
    }

    pub fn get(&self, hash: &str) -> std::io::Result<Vec<u8>> {
        if hash.len() < 3 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(std::io::Error::other("bad blob hash"));
        }
        let packed = fs::read(self.path(hash))?;
        zstd::decode_all(&packed[..])
    }

    pub fn wipe(&self) -> std::io::Result<()> {
        match fs::remove_dir_all(&self.root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        fs::create_dir_all(&self.root)
    }
}
