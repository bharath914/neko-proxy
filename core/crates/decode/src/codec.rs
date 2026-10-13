use std::io::Read;

/// Largest decoded body we will produce. Guards against decompression bombs.
pub const DEFAULT_DECODE_LIMIT: usize = 32 * 1024 * 1024;

pub struct Decoded {
    pub bytes: Vec<u8>,
    /// Encodings that were applied, outermost first, e.g. "gzip".
    pub applied: Vec<String>,
    /// Set when decoding stopped early (corrupt, truncated, or over limit).
    pub problem: Option<String>,
}

/// Undo `Content-Encoding`. Handles gzip, deflate, br, zstd, identity and
/// comma-separated chains. Unknown encodings return the bytes untouched with
/// a problem note.
pub fn decode_content(encoding: &str, wire: &[u8], limit: usize) -> Decoded {
    let mut bytes = wire.to_vec();
    let mut applied = Vec::new();
    let mut problem = None;
    // Encodings are listed in the order they were applied; undo in reverse.
    let list: Vec<String> = encoding
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty() && s != "identity")
        .collect();
    for enc in list.iter().rev() {
        let (out, err) = match enc.as_str() {
            "gzip" | "x-gzip" => read_all(flate2::read::MultiGzDecoder::new(&bytes[..]), limit),
            "deflate" => decode_deflate(&bytes, limit),
            "br" => read_all(brotli::Decompressor::new(&bytes[..], 4096), limit),
            "zstd" => match zstd::stream::read::Decoder::new(&bytes[..]) {
                Ok(d) => read_all(d, limit),
                Err(e) => (Vec::new(), Some(e.to_string())),
            },
            other => {
                problem = Some(format!("Unsupported content encoding \"{other}\""));
                break;
            }
        };
        applied.push(enc.clone());
        bytes = out;
        if let Some(e) = err {
            problem = Some(e);
            break;
        }
    }
    Decoded {
        bytes,
        applied,
        problem,
    }
}

fn decode_deflate(bytes: &[u8], limit: usize) -> (Vec<u8>, Option<String>) {
    // "deflate" is zlib-wrapped by spec but servers often send raw deflate.
    let (out, err) = read_all(flate2::read::ZlibDecoder::new(bytes), limit);
    if err.is_none() || !out.is_empty() {
        return (out, err);
    }
    read_all(flate2::read::DeflateDecoder::new(bytes), limit)
}

/// Read to the end or the limit, keeping whatever was produced on error.
fn read_all<R: Read>(r: R, limit: usize) -> (Vec<u8>, Option<String>) {
    let mut out = Vec::new();
    let mut take = r.take(limit as u64 + 1);
    match take.read_to_end(&mut out) {
        Ok(_) if out.len() > limit => {
            out.truncate(limit);
            (
                out,
                Some(format!(
                    "Body larger than {} MB, cut for display",
                    limit >> 20
                )),
            )
        }
        Ok(_) => (out, None),
        Err(e) => (out, Some(format!("Decoding stopped early: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn gzip_roundtrip() {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(b"hello hello hello").unwrap();
        let wire = e.finish().unwrap();
        let d = decode_content("gzip", &wire, 1024);
        assert_eq!(d.bytes, b"hello hello hello");
        assert_eq!(d.applied, ["gzip"]);
        assert!(d.problem.is_none());
    }

    #[test]
    fn brotli_and_zstd_roundtrip() {
        let mut wire = Vec::new();
        {
            let mut w = brotli::CompressorWriter::new(&mut wire, 4096, 5, 20);
            w.write_all(b"brotli body").unwrap();
        }
        assert_eq!(decode_content("br", &wire, 1024).bytes, b"brotli body");

        let wire = zstd::encode_all(&b"zstd body"[..], 3).unwrap();
        assert_eq!(decode_content("zstd", &wire, 1024).bytes, b"zstd body");
    }

    #[test]
    fn chain_is_undone_in_reverse() {
        let inner = zstd::encode_all(&b"layers"[..], 3).unwrap();
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(&inner).unwrap();
        let wire = e.finish().unwrap();
        // zstd applied first, then gzip.
        let d = decode_content("zstd, gzip", &wire, 1024);
        assert_eq!(d.bytes, b"layers");
        assert_eq!(d.applied, ["gzip", "zstd"]);
    }

    #[test]
    fn raw_deflate_is_accepted() {
        let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(b"raw deflate").unwrap();
        let wire = e.finish().unwrap();
        assert_eq!(decode_content("deflate", &wire, 1024).bytes, b"raw deflate");
    }

    #[test]
    fn truncated_gzip_keeps_partial_output() {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::none());
        e.write_all(&vec![b'a'; 4000]).unwrap();
        let mut wire = e.finish().unwrap();
        wire.truncate(wire.len() - 200);
        let d = decode_content("gzip", &wire, 1 << 20);
        assert!(!d.bytes.is_empty());
        assert!(d.problem.is_some());
    }

    #[test]
    fn bomb_is_capped() {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        e.write_all(&vec![0u8; 1 << 20]).unwrap();
        let wire = e.finish().unwrap();
        let d = decode_content("gzip", &wire, 1000);
        assert_eq!(d.bytes.len(), 1000);
        assert!(d.problem.is_some());
    }

    #[test]
    fn unknown_encoding_is_reported() {
        let d = decode_content("compress", b"abc", 100);
        assert_eq!(d.bytes, b"abc");
        assert!(d.problem.unwrap().contains("compress"));
    }
}
