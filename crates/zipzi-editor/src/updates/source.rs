//! Authenticate the exact feed bytes before Velopack sees package URLs or hashes.
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::{path::Path, sync::mpsc::Sender, time::Duration};
use velopack::{
    Error, HttpOptions, VelopackAsset, VelopackAssetFeed,
    bundle::Manifest,
    sources::{HttpSource, UpdateSource},
};

pub(super) struct SignedSource {
    base: url::Url,
    key: VerifyingKey,
    downloads: HttpSource,
}

pub(super) fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], Error> {
    if value.len() != N * 2 || !value.is_ascii() {
        return Err(Error::Other(
            "Invalid update signing key or signature".into(),
        ));
    }
    let mut result = [0; N];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| Error::Other("Invalid update signing key or signature".into()))?;
    }
    Ok(result)
}

impl SignedSource {
    pub(super) fn new(base: &str, key: &str) -> Result<Self, Error> {
        let base = url::Url::parse(&format!("{}/", base.trim_end_matches('/')))?;
        if base.scheme() != "https"
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(Error::Other(
                "Update source must be an HTTPS directory URL".into(),
            ));
        }
        let key =
            VerifyingKey::from_bytes(&decode_hex(key)?).map_err(|e| Error::Other(e.to_string()))?;
        let downloads = HttpSource::new_with_options(
            base.as_str(),
            HttpOptions {
                TimeoutMilliseconds: 300_000,
                ..Default::default()
            },
        );
        Ok(Self {
            base,
            key,
            downloads,
        })
    }

    fn fetch(&self, name: &str, limit: u64) -> Result<Vec<u8>, Error> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        let mut response = agent.get(self.base.join(name)?.as_str()).call()?;
        Ok(response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()?)
    }

    fn verify(
        &self,
        bytes: &[u8],
        signature: &str,
        app_id: &str,
    ) -> Result<VelopackAssetFeed, Error> {
        let signature = Signature::from_bytes(&decode_hex(signature.trim())?);
        self.key
            .verify_strict(bytes, &signature)
            .map_err(|_| Error::Other("Update feed signature verification failed".into()))?;
        let feed: VelopackAssetFeed = serde_json::from_slice(bytes)?;
        for asset in &feed.Assets {
            // One immutable package in the same directory. No absolute URLs or traversal.
            if asset.PackageId != app_id
                || asset.FileName.is_empty()
                || !asset
                    .FileName
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
                || asset.FileName.starts_with('.')
                || asset.Size == 0
                || asset.Size > 2 << 30
                || decode_hex::<32>(&asset.SHA256).is_err()
            {
                return Err(Error::InvalidPackage("Invalid signed update asset".into()));
            }
        }
        Ok(feed)
    }
}

impl UpdateSource for SignedSource {
    fn get_release_feed(
        &self,
        channel: &str,
        app: &Manifest,
        _: &str,
    ) -> Result<VelopackAssetFeed, Error> {
        if channel.is_empty()
            || !channel
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err(Error::Other("Invalid update channel".into()));
        }
        let name = format!("releases.{channel}.json");
        let bytes = self.fetch(&name, 2 << 20)?;
        let signature = self.fetch(&format!("{name}.sig"), 256)?;
        let signature = std::str::from_utf8(&signature).map_err(|e| Error::Other(e.to_string()))?;
        self.verify(&bytes, signature, &app.id)
    }

    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        path: &Path,
        progress: Option<Sender<i16>>,
    ) -> Result<(), Error> {
        self.downloads.download_release_entry(asset, path, progress)
    }
}

/// Revalidate a cached archive immediately before handing it to the installer.
pub(super) fn verify_package(path: &Path, asset: &VelopackAsset) -> Result<(), Error> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size != asset.Size {
        return Err(Error::SizeInvalid(path.to_owned(), asset.Size, size));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !digest.eq_ignore_ascii_case(&asset.SHA256) {
        return Err(Error::ChecksumInvalid(
            path.to_owned(),
            asset.SHA256.clone(),
            digest,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn signed_feed_rejects_tampering_and_unsafe_assets() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let source = SignedSource::new(
            "https://example.com/updates",
            &hex(key.verifying_key().as_bytes()),
        )
        .unwrap();
        let asset = VelopackAsset {
            PackageId: "Zipzi.Editor".into(),
            Version: "1.0.1".into(),
            Type: "Full".into(),
            FileName: "Zipzi.Editor-1.0.1-full.nupkg".into(),
            SHA256: "ab".repeat(32),
            Size: 123,
            ..Default::default()
        };
        let bytes = serde_json::to_vec(&VelopackAssetFeed {
            Assets: vec![asset.clone()],
        })
        .unwrap();
        let signature = hex(&key.sign(&bytes).to_bytes());
        assert_eq!(
            source
                .verify(&bytes, &signature, "Zipzi.Editor")
                .unwrap()
                .Assets
                .len(),
            1
        );
        let mut corrupted = bytes.clone();
        corrupted[10] ^= 1;
        assert!(
            source
                .verify(&corrupted, &signature, "Zipzi.Editor")
                .is_err()
        );
        assert!(source.verify(&bytes, &signature, "Other.App").is_err());
        for path in [
            "../evil.nupkg",
            "https://evil.test/payload",
            "..",
            "a%2fb",
            "a\\b",
        ] {
            let mut unsafe_asset = asset.clone();
            unsafe_asset.FileName = path.into();
            let bytes = serde_json::to_vec(&VelopackAssetFeed {
                Assets: vec![unsafe_asset],
            })
            .unwrap();
            assert!(
                source
                    .verify(&bytes, &hex(&key.sign(&bytes).to_bytes()), "Zipzi.Editor")
                    .is_err()
            );
        }
    }

    #[test]
    fn update_source_requires_https_and_a_valid_pinned_key() {
        let key = hex(SigningKey::from_bytes(&[7; 32]).verifying_key().as_bytes());
        for url in [
            "http://example.com",
            "file:///tmp",
            "https://user@example.com",
            "https://example.com?a=1",
        ] {
            assert!(SignedSource::new(url, &key).is_err());
        }
        assert!(SignedSource::new("https://example.com", "bad-key").is_err());
    }

    #[test]
    fn cached_package_tampering_is_rejected_before_installation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("update.nupkg");
        std::fs::write(&path, b"authentic").unwrap();
        let asset = VelopackAsset {
            Size: 9,
            SHA256: hex(&Sha256::digest(b"authentic")),
            ..Default::default()
        };
        verify_package(&path, &asset).unwrap();
        std::fs::write(&path, b"corrupted").unwrap();
        assert!(verify_package(&path, &asset).is_err());
        std::fs::write(&path, b"short").unwrap();
        assert!(verify_package(&path, &asset).is_err());
    }
}
