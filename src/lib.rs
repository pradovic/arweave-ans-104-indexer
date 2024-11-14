mod tags;
mod utils;

use serde::ser::SerializeStruct;
use serde::Serialize;
use serde::Serializer;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use base64::Engine;
use sha2::{Digest, Sha256};

use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use async_recursion::async_recursion;

use tags::Tag;

#[derive(Debug)]
pub enum StreamParseError {
    ReadError(std::io::Error),
    ParseError {
        message: String,
        bytes_read: usize,
    },
}

impl std::fmt::Display for StreamParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamParseError::ReadError(e) => write!(f, "Read error: {}", e),
            StreamParseError::ParseError { message, .. } => write!(f, "Parse error: {}", message),
        }
    }
}

impl std::error::Error for StreamParseError {}

#[derive(Debug)]
pub struct DataItem {
    signature: Vec<u8>,
    owner: Vec<u8>,
    target: Option<[u8; 32]>,
    anchor: Option<[u8; 32]>,
    tags: Vec<Tag>,
    bundled_in: String,
    is_bundle: bool,
}

impl Serialize for DataItem {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("DataItem", 8)?;

        let id = self.calculate_id();
        state.serialize_field("id", &BASE64_URL.encode(id))?;

        state.serialize_field("signature", &BASE64_URL.encode(&self.signature))?;
        state.serialize_field("owner", &BASE64_URL.encode(&self.owner))?;

        if let Some(target) = &self.target {
            state.serialize_field("target", &BASE64_URL.encode(target))?;
        } else {
            state.serialize_field("target", "")?;
        }

        if let Some(anchor) = &self.anchor {
            state.serialize_field("anchor", &BASE64_URL.encode(anchor))?;
        } else {
            state.serialize_field("anchor", "")?;
        }

        state.serialize_field("tags", &self.tags)?;
        state.serialize_field("bundled_in", &self.bundled_in)?;
        state.serialize_field("is_bundle", &self.is_bundle)?;
        state.end()
    }
}

impl DataItem {
    fn calculate_id(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.signature);
        hasher.finalize().into()
    }

    pub async fn parse_stream<R: AsyncRead + Unpin>(
        stream: &mut R,
        bundled_in: String,
        size: usize,
    ) -> Result<Self, StreamParseError> {
        let mut bytes_read = 0;

        let mut sig_type = [0u8; 2];
        stream.read_exact(&mut sig_type).await.map_err(StreamParseError::ReadError)?;
        bytes_read += 2;
        let signature_type = u16::from_le_bytes(sig_type);

        let sig_length = match signature_type {
            1 => 512, // RSA
            2 => 64,  // ED25519
            _ => {
                return Err(StreamParseError::ParseError {
                    message: format!("Unknown signature type: {}", signature_type),
                    bytes_read,
                });
            }
        };

        let mut signature = vec![0u8; sig_length];
        stream.read_exact(&mut signature).await.map_err(StreamParseError::ReadError)?;
        bytes_read += sig_length;

        let owner_length = match signature_type {
            1 => 512, // RSA
            2 => 32,  // ED25519
            _ => {
                return Err(StreamParseError::ParseError {
                    message: "Invalid owner signature type".to_string(),
                    bytes_read,
                });
            }
        };
        let mut owner = vec![0u8; owner_length];
        stream.read_exact(&mut owner).await.map_err(StreamParseError::ReadError)?;
        bytes_read += owner_length;

        let mut presence = [0u8; 1];
        stream.read_exact(&mut presence).await.map_err(StreamParseError::ReadError)?;
        bytes_read += 1;

        let target = match presence[0] {
            1 => {
                let mut target = [0u8; 32];
                stream.read_exact(&mut target).await.map_err(StreamParseError::ReadError)?;
                bytes_read += 32;
                Some(target)
            }
            0 => None,
            _ => {
                return Err(StreamParseError::ParseError {
                    message: "Invalid target presence byte".to_string(),
                    bytes_read,
                });
            }
        };

        stream.read_exact(&mut presence).await.map_err(StreamParseError::ReadError)?;
        bytes_read += 1;

        let anchor = match presence[0] {
            1 => {
                let mut anchor = [0u8; 32];
                stream.read_exact(&mut anchor).await.map_err(StreamParseError::ReadError)?;
                bytes_read += 32;
                Some(anchor)
            }
            0 => None,
            _ => {
                return Err(StreamParseError::ParseError {
                    message: "Invalid anchor presence byte".to_string(),
                    bytes_read,
                });
            }
        };

        let mut tag_count_bytes = [0u8; 8];
        stream.read_exact(&mut tag_count_bytes).await.map_err(StreamParseError::ReadError)?;
        bytes_read += 8;

        let tag_count = utils::bytes_to_number(&tag_count_bytes) as usize;
        if tag_count > 128 {
            return Err(StreamParseError::ParseError {
                message: format!("Too many tags: {}", tag_count),
                bytes_read,
            });
        }

        let mut tags_length_bytes = [0u8; 8];
        stream.read_exact(&mut tags_length_bytes).await.map_err(StreamParseError::ReadError)?;
        bytes_read += 8;

        let tags_length = utils::bytes_to_number(&tags_length_bytes) as usize;

        let mut tags_bytes = vec![0u8; tags_length];
        stream.read_exact(&mut tags_bytes).await.map_err(StreamParseError::ReadError)?;
        bytes_read += tags_length;

        let (tags, is_bundle) = parse_avro_tags(&tags_bytes).map_err(|e| StreamParseError::ParseError {
            message: format!("Failed to parse tags: {}", e),
            bytes_read,
        })?;

        if tags.len() != tag_count {
            return Err(StreamParseError::ParseError {
                message: format!("Tag count mismatch: expected {}, found {}", tag_count, tags.len()),
                bytes_read,
            });
        }

        let item = DataItem {
            signature,
            owner,
            target,
            anchor,
            tags,
            bundled_in,
            is_bundle,
        };

        if !is_bundle {
            let remaining = size - bytes_read;
            let mut skip_buf = vec![0u8; remaining];
            stream.read_exact(&mut skip_buf).await.map_err(StreamParseError::ReadError)?;
        }

        Ok(item)
    }
}


fn parse_avro_tags(bytes: &[u8]) -> Result<(Vec<Tag>, bool), String> {
    let schema = tags::TAGS_SCHEMA
        .parse()
        .map_err(|e| format!("parse schema error: {}", e))?;
    let tags: Vec<Tag> = serde_avro_fast::from_datum_slice(bytes, &schema)
        .map_err(|e| format!("avro parse error: {}", e))?;

    if tags.len() > 128 {
        return Err(format!("Too many tags: {}", tags.len()));
    }

    let mut valid_tags = Vec::with_capacity(tags.len());
    let mut bundle_format_found = false;
    let mut bundle_version_found = false;

    for tag in tags {
        if let Err(e) = tag.validate() {
            tracing::warn!("Invalid tag found: {:?}, Error: {}", tag, e);
            continue;
        }

        if let Ok((name, value)) = tag.try_to_utf8() {
            if name == "Bundle-Format" && value == "binary" {
                bundle_format_found = true;
            }
            if name == "Bundle-Version" && value == "2.0.0" {
                bundle_version_found = true;
            }
        }

        valid_tags.push(tag);
    }

    let is_bundle = bundle_format_found && bundle_version_found;
    Ok((valid_tags, is_bundle))
}


#[derive(Debug)]
pub struct Bundle {
    pub item_count: u32,
    pub entries: Vec<BundleEntry>,
}

#[derive(Debug)]
pub struct BundleEntry {
    pub size: u32,
    pub id: [u8; 32],
}

impl Bundle {
    pub async fn parse_stream<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Self, StreamParseError> {
        let mut count_buf = [0u8; 32];
        stream.read_exact(&mut count_buf).await.map_err(StreamParseError::ReadError)?;
        let item_count = utils::bytes_to_number(&count_buf) as u32;

        let mut entries = Vec::with_capacity(item_count as usize);
        for _ in 0..item_count {
            let mut size_buf = [0u8; 32];
            stream.read_exact(&mut size_buf).await.map_err(StreamParseError::ReadError)?;
            let size = utils::bytes_to_number(&size_buf) as u32;

            let mut id = [0u8; 32];
            stream.read_exact(&mut id).await.map_err(StreamParseError::ReadError)?;

            entries.push(BundleEntry { size, id });
        }

        Ok(Bundle {
            item_count,
            entries,
        })
    }
}


#[async_recursion]
pub async fn process_bundle(
    stream: &mut (impl AsyncRead + Unpin + Send),
    tx: mpsc::Sender<DataItem>,
    bundled_in: &str,
) -> Result<(), String> {
    let bundle = Bundle::parse_stream(stream)
        .await
        .map_err(|e| format!("Parse bundle fatal error: {}", e))?;

    tracing::info!(
        "Processing bundle with {} entries, bundled in {}",
        bundle.item_count,
        bundled_in
    );

    for entry in bundle.entries {
        match DataItem::parse_stream(stream, bundled_in.to_string(), entry.size as usize).await {
            Ok(data_item) => {
                if data_item.is_bundle {
                    tx.send(data_item)
                        .await
                        .map_err(|e| format!("Channel send error: {}", e))?;
                    process_bundle(stream, tx.clone(), &BASE64_URL.encode(entry.id)).await?;
                } else {
                    tx.send(data_item)
                        .await
                        .map_err(|e| format!("Channel send error: {}", e))?;
                }
            }
            Err(StreamParseError::ReadError(e)) => {
                return Err(format!("Stream read error: {}", e));
            }
            Err(StreamParseError::ParseError { message, bytes_read }) => {
                let remaining = entry.size as usize - bytes_read;
                tracing::warn!(
                    "Parse error: {}, skipping {} bytes for entry {}",
                    message,
                    remaining,
                    BASE64_URL.encode(entry.id)
                );
                let mut skip_buf = vec![0u8; remaining];
                stream
                    .read_exact(&mut skip_buf)
                    .await
                    .map_err(|e| format!("Failed to skip bytes: {}", e))?;
            }
        }
    }

    Ok(())
}
