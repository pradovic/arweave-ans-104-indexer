use clap::Parser as ClapParser;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use reqwest::Client;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use base64::Engine;

use serde::Serializer;
use serde::ser::SerializeStruct;


#[derive(ClapParser)]
#[command(author, version, about, long_about = None)]
struct Args {
    tx_id: String,

    #[arg(short, long, default_value = "bundle.json")]
    output: std::path::PathBuf,
}

#[derive(Debug)]
struct DataItem {
    signature_type: u16,
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


enum DataItemResult {
    Item(DataItem),
    NestedBundle(DataItem),
}



impl DataItem {
    // Generate ID from signature
    fn calculate_id(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.signature);
        hasher.finalize().into()
    }

    // Add method to check if this item could be a bundle
    fn is_bundle(&self) -> bool {
        self.tags.iter().any(|tag| {
            tag.try_to_utf8()
                .map(|(name, value)| name == "Bundle-Format" && value == "binary")
                .unwrap_or(false)
        }) && self.tags.iter().any(|tag| {
            tag.try_to_utf8()
                .map(|(name, value)| name == "Bundle-Version" && value == "2.0.0")
                .unwrap_or(false)
        })
    }
}


fn bytes_to_number(bytes: &[u8]) -> u64 {
    let mut value = 0u64;
    for &byte in bytes.iter().rev() {
        value = value * 256 + byte as u64;
    }
    value
}


// tags

#[derive(Debug, Deserialize)]
struct Tag {
    #[serde(with = "serde_bytes")]
    name: Vec<u8>,
    #[serde(with = "serde_bytes")]

    value: Vec<u8>,
}


impl Serialize for Tag {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Tag", 2)?;
        
        // Try to decode to UTF-8 first, if fails fallback to base64
        match (String::from_utf8(self.name.clone()), String::from_utf8(self.value.clone())) {
            (Ok(name), Ok(value)) => {
                state.serialize_field("name", &name)?;
                state.serialize_field("value", &value)?;
            },
            _ => {
                state.serialize_field("name", &BASE64_URL.encode(&self.name))?;
                state.serialize_field("value", &BASE64_URL.encode(&self.value))?;
            }
        }
        
        state.end()
    }
}


const TAGS_SCHEMA: &str = r#"{
    "type": "array",
    "items": {
      "type": "record",
      "name": "Tag",
      "fields": [
        { "name": "name", "type": "bytes" },
        { "name": "value", "type": "bytes" }
      ]
    }
  }"#;

impl Tag {
    fn validate(&self) -> Result<()> {
        if self.name.len() > 1024 {
            return Err("Tag name exceeds 1024 bytes".into());
        }
        if self.value.len() > 3072 {
            return Err("Tag value exceeds 3072 bytes".into());
        }
        if self.name.is_empty() || self.value.is_empty() {
            return Err("Tag name and value must not be empty".into());
        }
        Ok(())
    }

    fn try_to_utf8(&self) -> Result<(String, String)> {
        Ok((
            String::from_utf8(self.name.clone())?,
            String::from_utf8(self.value.clone())?
        ))
    }
}



fn parse_avro_tags(bytes: &[u8]) -> Result<Vec<Tag>> {
    let schema: serde_avro_fast::Schema = TAGS_SCHEMA.parse()?;
    let tags: Vec<Tag> = serde_avro_fast::from_datum_slice(bytes, &schema)?;

    if tags.len() > 128 {
        return Err("Too many tags".into());
    }

    for tag in &tags {
        if tag.name.len() > 1024 || tag.value.len() > 3072 {
            return Err("Tag size exceeds limits".into());
        }
    }

    Ok(tags)
}

impl DataItem {
    pub async fn parse_stream<R: AsyncRead + Unpin>(
        stream: &mut R,
        bundled_in: String,
        size: usize,
    ) -> Result<DataItemResult> {
        let mut bytes_read = 0;
        
        // Read signature type (2 bytes)
        let mut sig_type = [0u8; 2];
        stream.read_exact(&mut sig_type).await?;
        bytes_read += 2;
        let signature_type = u16::from_le_bytes(sig_type);

        // Read signature
        let sig_length = match signature_type {
            1 => 512, // RSA
            2 => 64,  // ED25519
            _ => return Err(format!("Unknown signature type: {}", signature_type).into()),
        };
        let mut signature = vec![0u8; sig_length];
        stream.read_exact(&mut signature).await?;
        bytes_read += sig_length;

        // Read owner
        let owner_length = match signature_type {
            1 => 512, // RSA
            2 => 32,  // ED25519
            _ => return Err("Invalid signature type for owner".into()),
        };
        let mut owner = vec![0u8; owner_length];
        stream.read_exact(&mut owner).await?;
        bytes_read += owner_length;

        // Read target (optional)
        let mut presence = [0u8; 1];
        stream.read_exact(&mut presence).await?;
        bytes_read += 1;
        let target = match presence[0] {
            1 => {
                let mut target = [0u8; 32];
                stream.read_exact(&mut target).await?;
                bytes_read += 32;
                Some(target)
            }
            0 => None,
            _ => return Err("Invalid target presence byte".into()),
        };

        // Read anchor (optional)
        stream.read_exact(&mut presence).await?;
        bytes_read += 1;
        let anchor = match presence[0] {
            1 => {
                let mut anchor = [0u8; 32];
                stream.read_exact(&mut anchor).await?;
                bytes_read += 32;
                Some(anchor)
            }
            0 => None,
            _ => return Err("Invalid anchor presence byte".into()),
        };

        // Read tag counts
        let mut tag_count_bytes = [0u8; 8];
        stream.read_exact(&mut tag_count_bytes).await?;
        bytes_read += 8;
        let tag_count = bytes_to_number(&tag_count_bytes) as usize;

        let mut tags_length_bytes = [0u8; 8];
        stream.read_exact(&mut tags_length_bytes).await?;
        bytes_read += 8;
        let tags_length = bytes_to_number(&tags_length_bytes) as usize;

        // Read tags
        let mut tags_bytes = vec![0u8; tags_length];
        stream.read_exact(&mut tags_bytes).await?;
        bytes_read += tags_length;
        let tags = parse_avro_tags(&tags_bytes)?;

        if tags.len() != tag_count {
            return Err(format!(
                "Tag count mismatch: header says {} but found {}",
                tag_count,
                tags.len()
            )
            .into());
        }

        let item = DataItem {
            signature_type,
            signature,
            owner,
            target,
            anchor,
            tags,
            bundled_in,
            is_bundle: false, // Will be set to true if needed
        };

        if item.is_bundle() {
            Ok(DataItemResult::NestedBundle(
                DataItem { is_bundle: true, ..item }
            ))
        } else {
            // Skip remaining data bytes
            let remaining = size - bytes_read;
            let mut skip_buf = vec![0u8; remaining];
            stream.read_exact(&mut skip_buf).await?;
            
            Ok(DataItemResult::Item(item))
        }
    }
}

#[derive(Debug)]
struct Bundle {
    item_count: u32,
    entries: Vec<BundleEntry>,
}

#[derive(Debug)]
struct BundleEntry {
    size: u32,
    id: [u8; 32],
}

use tokio::io::AsyncReadExt;  
use tokio::io::AsyncRead;     

impl Bundle {
    async fn parse_stream<R: AsyncRead + Unpin>(
        stream: &mut R,
    ) -> Result<Self> {
        let mut count_buf = [0u8; 32];
        stream.read_exact(&mut count_buf).await?;
        let item_count = bytes_to_number(&count_buf) as u32;

        let mut entries = Vec::with_capacity(item_count as usize);
        for _ in 0..item_count {
            let mut size_buf = [0u8; 32];
            stream.read_exact(&mut size_buf).await?;
            let size = bytes_to_number(&size_buf) as u32;

            let mut id = [0u8; 32];
            stream.read_exact(&mut id).await?;

            entries.push(BundleEntry { size, id });
        }

        Ok(Bundle {
            item_count,
            entries,
        })
    }
}

use async_recursion::async_recursion;
use tokio::io::AsyncWriteExt;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
#[async_recursion]
async fn process_bundle(
    stream: &mut (impl AsyncRead + Unpin + Send),
    file: &mut tokio::fs::File,
    tx_id: &str,
) -> Result<()> {
    let bundle = Bundle::parse_stream(stream).await?;
    

   for (i, entry) in bundle.entries.into_iter().enumerate() {
    match DataItem::parse_stream(stream, tx_id.to_string(), entry.size as usize).await? {
        DataItemResult::Item(item) => {
            file.write_all(serde_json::to_string_pretty(&item)?.as_bytes()).await?;
            file.write_all(b"\n").await?;
        },
        DataItemResult::NestedBundle(item) => {
            file.write_all(serde_json::to_string_pretty(&item)?.as_bytes()).await?;
            file.write_all(b"\n").await?;
            process_bundle(stream, file, &BASE64_URL.encode(entry.id)).await?;
        }
    }
    println!("Finished processing entry {}", i + 1);
}
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut file = tokio::fs::File::create(&args.output).await?;
    
    let client = Client::new();
    let response = client.get(format!("https://arweave.net/{}", args.tx_id))
        .send()
        .await?;
    
    let response_bytes = response.bytes().await?;
    let mut cursor = std::io::Cursor::new(response_bytes);
    
    process_bundle(&mut cursor, &mut file, &args.tx_id).await?;
    
    Ok(())
}