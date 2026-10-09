//! Immutable font-resource bytes. Linked-file verification belongs to preparation workers.
use super::Error;
use fold_media::Cancel;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path, sync::Arc};

const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    Cancelled,
    InvalidFingerprint,
    Missing,
    InvalidFile,
    Changed,
    Budget,
    Io(String),
}

/// A content-addressed byte snapshot. Font format/family validation occurs when
/// creating the shaping service. No path is retained or implicitly reopened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontSource {
    pub(super) bytes: Arc<[u8]>,
    pub(super) hash: [u8; 32],
}
impl FontSource {
    /// Explicit in-memory sources, including packaged fonts, need no filesystem access.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Budget);
        }
        let hash = Sha256::digest(&bytes).into();
        Ok(Self {
            bytes: bytes.into(),
            hash,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }
    pub fn fingerprint(&self) -> String {
        let hex: String = self.hash.iter().map(|b| format!("{b:02x}")).collect();
        format!("sha256:{hex}")
    }
    /// Reads at most 32 MiB from a regular file and verifies the exact supplied
    /// canonical `sha256:` fingerprint before publication. Call on a preparation
    /// worker. Every new acquisition verifies the linked file; held snapshots remain
    /// valid after source replacement or deletion. No system-font fallback is used.
    pub fn read(path: &Path, expected: &str, cancel: &Cancel) -> Result<Self, LoadError> {
        cancelled(cancel)?;
        let Some(hex) = expected.strip_prefix("sha256:") else {
            return Err(LoadError::InvalidFingerprint);
        };
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(LoadError::InvalidFingerprint);
        }
        let metadata = std::fs::metadata(path).map_err(io)?;
        if !metadata.is_file() {
            return Err(LoadError::InvalidFile);
        }
        if metadata.len() > MAX_BYTES as u64 {
            return Err(LoadError::Budget);
        }
        let input = File::open(path).map_err(io)?;
        let opened = input.metadata().map_err(io)?;
        if !opened.is_file() {
            return Err(LoadError::InvalidFile);
        }
        if opened.len() > MAX_BYTES as u64 {
            return Err(LoadError::Budget);
        }
        read_verified(input, opened.len() as usize, expected, cancel)
    }
}
fn read_verified(
    mut input: impl Read,
    length: usize,
    expected: &str,
    cancel: &Cancel,
) -> Result<FontSource, LoadError> {
    cancelled(cancel)?;
    if length > MAX_BYTES {
        return Err(LoadError::Budget);
    }
    let mut bytes = Vec::with_capacity(length);
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        cancelled(cancel)?;
        let count = input.read(&mut buffer).map_err(io)?;
        if count == 0 {
            break;
        }
        if bytes.len() + count > MAX_BYTES {
            return Err(LoadError::Budget);
        }
        if bytes.len() + count > length {
            return Err(LoadError::Changed);
        }
        bytes.extend_from_slice(&buffer[..count]);
        hash.update(&buffer[..count]);
    }
    cancelled(cancel)?;
    let hash = hash.finalize();
    if bytes.len() != length || format!("sha256:{hash:x}") != expected {
        return Err(LoadError::Changed);
    }
    let source = FontSource {
        bytes: bytes.into(),
        hash: hash.into(),
    };
    cancelled(cancel)?;
    Ok(source)
}

fn cancelled(cancel: &Cancel) -> Result<(), LoadError> {
    cancel.check().map_err(|_| LoadError::Cancelled)
}
fn io(error: std::io::Error) -> LoadError {
    if error.kind() == std::io::ErrorKind::NotFound {
        LoadError::Missing
    } else {
        LoadError::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_growth_truncation_and_mid_read_cancellation_never_publish() {
        let data = vec![42; 128 * 1024];
        let expected = FontSource::from_bytes(data.clone()).unwrap().fingerprint();
        for length in [data.len() - 1, data.len() + 1] {
            assert_eq!(
                read_verified(data.as_slice(), length, &expected, &Cancel::default()).unwrap_err(),
                LoadError::Changed
            );
        }
        struct Cancelling<'a> {
            bytes: &'a [u8],
            cancel: &'a Cancel,
        }
        impl Read for Cancelling<'_> {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                let count = self.bytes.read(output)?;
                self.cancel.cancel();
                Ok(count)
            }
        }
        let cancel = Cancel::default();
        assert_eq!(
            read_verified(
                Cancelling {
                    bytes: &data,
                    cancel: &cancel
                },
                data.len(),
                &expected,
                &cancel
            )
            .unwrap_err(),
            LoadError::Cancelled
        );
    }
}
