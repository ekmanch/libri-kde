//! The sha git gives a blob: the hex SHA-1 of `"blob <length>\0"` followed
//! by the bytes. It is what the Contents API wants as the `sha` of the
//! file being replaced, so Libri computes it from the raw download
//! instead of asking for it, and it is how the three-way sync tells
//! whether a copy changed (the phone's `git_blob.dart`).

use sha1::{Digest, Sha1};

pub fn blob_sha(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(40);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_git_hash_object() {
        // `printf '' | git hash-object --stdin` and `printf 'hello\n' | …`.
        assert_eq!(blob_sha(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
        assert_eq!(
            blob_sha(b"hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }
}
