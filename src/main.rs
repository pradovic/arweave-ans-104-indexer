use bytes::{Buf, Bytes};
use clap::Parser as ClapParser;
use serde::{Deserialize, Serialize, Serializer};
use std::io::{BufReader, Read};
use serde::ser::SerializeStruct;
use sha2::{Sha256, Digest};


use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use base64::Engine;

const CHUNK_SIZE: usize = 1024 * 1024; // 1MB chunks

#[derive(ClapParser)]
#[command(author, version, about, long_about = None)]
struct Args {
    tx_id: String,
}

#[derive(Debug)]
struct Bundle {
    item_count: u32,
    entries: Vec<BundleEntry>,
    items: Vec<DataItem>,
}

#[derive(Debug)]
struct BundleEntry {
    size: u32,
    id: [u8; 32],
}

#[derive(Debug)]
struct DataItem {
    signature_type: u16,
    signature: Vec<u8>,
    owner: Vec<u8>,
    target: Option<[u8; 32]>,
    anchor: Option<[u8; 32]>,
    tags: Vec<Tag>,
    data: Vec<u8>,
}

impl Serialize for DataItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("DataItem", 6)?;

        // id is calculated from signature
        let id = self.calculate_id();
        state.serialize_field("id", &BASE64_URL.encode(id))?;
        
        // Base64URL encode binary fields
        //state.serialize_field("signature", &BASE64_URL.encode(&self.signature))?;
        state.serialize_field("owner", &BASE64_URL.encode(&self.owner))?;
        
        // Handle optional target
        if let Some(target) = &self.target {
            state.serialize_field("target", &BASE64_URL.encode(target))?;
        } else {
            state.serialize_field("target", "")?;
        }

        // Handle optional anchor
        if let Some(anchor) = &self.anchor {
            state.serialize_field("anchor", &BASE64_URL.encode(anchor))?;
        } else {
            state.serialize_field("anchor", "")?;
        }
        
        // Tags are already handled by Tag's Serialize implementation
        state.serialize_field("tags", &self.tags)?;

        // skip data
        // todo
    
        state.end()
    }
}


impl DataItem {
    // Generate ID from signature
    fn calculate_id(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.signature);
        hasher.finalize().into()
    }
}

// Helper function for number parsing (same as before)
fn bytes_to_number(bytes: &[u8]) -> u64 {
    let mut value = 0u64;
    for &byte in bytes.iter().rev() {
        value = value * 256 + byte as u64;
    }
    value
}

impl TryFrom<Bytes> for DataItem {
    type Error = Box<dyn std::error::Error>;

    fn try_from(mut bytes: Bytes) -> Result<Self, Self::Error> {
        // signature type (2 bytes)
        if bytes.remaining() < 2 {
            return Err("Not enough bytes for signature type".into());
        }
        let signature_type = bytes.get_u16_le();

        // signature based on type
        let sig_length = match signature_type {
            1 => 512, // RSA
            2 => 64,  // ED25519
            _ => return Err(format!("Unknown signature type: {}", signature_type).into()),
        };

        if bytes.remaining() < sig_length {
            return Err("Not enough bytes for signature".into());
        }
        let mut signature = vec![0u8; sig_length];
        bytes.copy_to_slice(&mut signature);

        // owner
        let owner_length = match signature_type {
            1 => 512, // RSA
            2 => 32,  // ED25519
            _ => return Err("Invalid signature type for owner".into()),
        };

        if bytes.remaining() < owner_length {
            return Err("Not enough bytes for owner".into());
        }
        let mut owner = vec![0u8; owner_length];
        bytes.copy_to_slice(&mut owner);

        // target (optional)
        if bytes.remaining() < 1 {
            return Err("Not enough bytes for target presence byte".into());
        }
        let target = match bytes.get_u8() {
            1 => {
                if bytes.remaining() < 32 {
                    return Err("Not enough bytes for target".into());
                }
                let mut target = [0u8; 32];
                bytes.copy_to_slice(&mut target);
                Some(target)
            }
            0 => None,
            _ => return Err("Invalid target presence byte".into()),
        };

        // anchor (optional)
        if bytes.remaining() < 1 {
            return Err("Not enough bytes for anchor presence byte".into());
        }
        let anchor = match bytes.get_u8() {
            1 => {
                if bytes.remaining() < 32 {
                    return Err("Not enough bytes for anchor".into());
                }
                let mut anchor = [0u8; 32];
                bytes.copy_to_slice(&mut anchor);
                Some(anchor)
            }
            0 => None,
            _ => return Err("Invalid anchor presence byte".into()),
        };

        //  tags
        if bytes.remaining() < 16 {
            return Err("Not enough bytes for tag counts".into());
        }

        let mut tag_count_bytes = [0u8; 8];
        bytes.copy_to_slice(&mut tag_count_bytes);
        let tag_count = bytes_to_number(&tag_count_bytes) as usize;

        let mut tags_length_bytes = [0u8; 8];
        bytes.copy_to_slice(&mut tags_length_bytes);
        let tags_length = bytes_to_number(&tags_length_bytes) as usize;


        if bytes.remaining() < tags_length {
            return Err("Not enough bytes for tags data".into());
        }

        let tags_slice = bytes.slice(..tags_length);

        let tags = parse_avro_tags(tags_slice.as_ref())?;

        if tags.len() != tag_count {
            return Err(format!(
                "Tag count mismatch: header says {} but found {}",
                tag_count,
                tags.len()
            )
            .into());
        }

        bytes.advance(tags_length);

        //  Remaining bytes are data
        // todo: ignore data becuase we only need to index everything else
        let data = bytes.to_vec();

        Ok(DataItem {
            signature_type,
            signature,
            owner,
            target,
            anchor,
            tags,
            data,
        })
    }
}

impl TryFrom<Bytes> for Bundle {
    type Error = Box<dyn std::error::Error>;

    fn try_from(mut bytes: Bytes) -> Result<Self, Self::Error> {
        if bytes.remaining() < 32 {
            return Err("Not enough bytes for item count".into());
        }
        let mut count_bytes = [0u8; 32];
        bytes.copy_to_slice(&mut count_bytes);
        let item_count = bytes_to_number(&count_bytes) as u32;

        // headers section (64 bytes per item)
        let mut entries = Vec::with_capacity(item_count as usize);
        for i in 0..item_count {
            if bytes.remaining() < 64 {
                return Err(format!("Not enough bytes for bundle entry {}", i).into());
            }

            // length (32 bytes)
            let mut size_bytes = [0u8; 32];
            bytes.copy_to_slice(&mut size_bytes);
            let size = bytes_to_number(&size_bytes) as u32;

            // ID (32 bytes)
            let mut id = [0u8; 32];
            bytes.copy_to_slice(&mut id);

            entries.push(BundleEntry { size, id });
        }

        let mut items = Vec::with_capacity(item_count as usize);
        for (i, entry) in entries.iter().enumerate() {
            if bytes.remaining() < entry.size as usize {
                return Err(format!("Not enough bytes for data item {}", i).into());
            }

            let item_data = bytes.slice(..entry.size as usize);
            let item = DataItem::try_from(item_data)?;
            bytes.advance(entry.size as usize);

            items.push(item);
        }

        Ok(Bundle {
            item_count,
            entries,
            items,
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("Fetching and parsing bundle data...");
    let bundle_data = fetch_all_bundle_data(&args.tx_id)?;
    let bundle = Bundle::try_from(bundle_data)?;

    println!("\nBundle entries:");

    for (i, entry) in bundle.entries.iter().enumerate() {
        if i > 2 {
            break;
        }
        println!("Entry {}: size={}, id={:?}", i, entry.size, entry.id);

    }

    println!("\nParsed data items:");
    for (i, item) in bundle.items.iter().enumerate() {
        if i > 2 {
            break;
        }
        println!("Data item {}", serde_json::to_string_pretty(&item)?);

    }

    Ok(())
}

fn fetch_all_bundle_data(tx_id: &str) -> Result<Bytes, Box<dyn std::error::Error>> {
    let url = format!("https://arweave.net/{}", tx_id);
    println!("Fetching from URL: {}", url);

    let response = ureq::get(&url).call()?;

    let len = response
        .header("Content-Length")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    println!("Expected content length: {}", len);

    let mut reader = BufReader::new(response.into_reader());
    let mut bytes = Vec::with_capacity(len);
    let mut buffer = vec![0; CHUNK_SIZE];

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                bytes.extend_from_slice(&buffer[..n]);
            }
            Err(e) => return Err(format!("Error reading chunk: {}", e).into()),
        }
    }

    println!("Completed reading {} bytes", bytes.len());
    Ok(Bytes::from(bytes))
}

// tags

#[derive(Debug, Deserialize)]
struct Tag {
    #[serde(with = "serde_bytes")]
    name: Vec<u8>,
    #[serde(with = "serde_bytes")]

    value: Vec<u8>,
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
    fn validate(&self) -> Result<(), Box<dyn std::error::Error>> {
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

    fn try_to_utf8(&self) -> Result<(String, String), std::string::FromUtf8Error> {
        Ok((
            String::from_utf8(self.name.clone())?,
            String::from_utf8(self.value.clone())?
        ))
    }
}


impl Serialize for Tag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Tag", 2)?;
        // Base64URL encode both name and value
        state.serialize_field("name", &BASE64_URL.encode(&self.name))?;
        state.serialize_field("value", &BASE64_URL.encode(&self.value))?;
        state.end()
    }
}



fn parse_avro_tags(bytes: &[u8]) -> Result<Vec<Tag>, Box<dyn std::error::Error>> {
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
