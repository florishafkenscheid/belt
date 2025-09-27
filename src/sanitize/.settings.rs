use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

// --- Helper for debugging ---
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

// --- PropertyTree Definitions ---
#[derive(Clone, Copy, Debug, PartialEq)]
enum PropertyTreeType {
    None = 0,
    Bool = 1,
    Number = 2, // f64
    String = 3,
    List = 4,
    Dictionary = 5,
    SignedInteger = 6,   // i64
    UnsignedInteger = 7, // u64
}

#[derive(Clone, Debug)]
enum PropertyTreeData {
    None,
    Bool(bool),
    Number(f64),
    String(String),
    List(Vec<PropertyTreeData>),
    Dictionary(HashMap<String, PropertyTreeData>),
    SignedInteger(i64),
    UnsignedInteger(u64),
}

// --- MapVersion Definition ---
// We read exactly 9 bytes for the version.
#[derive(Debug, Clone, Copy)]
struct MapVersion {
    data: [u8; 9],
}

impl MapVersion {
    fn load(reader: &mut impl Read) -> io::Result<Self> {
        let mut data = [0u8; 9];
        reader.read_exact(&mut data)?;
        Ok(Self { data })
    }

    fn save(&self, writer: &mut impl Write) -> io::Result<()> {
        writer.write_all(&self.data)?;
        Ok(())
    }
}

// --- ModSettings Definitions ---
type ModSettingsScope = HashMap<String, ModSettingValue>;
type ModSettingsData = HashMap<String, ModSettingsScope>;

#[derive(Clone, Debug)]
pub enum ModSettingValue {
    Bool(bool),
    Int(i64),
    Number(f64),
    String(String),
    Color { r: f64, g: f64, b: f64, a: f64 },
}

#[derive(Debug)]
pub struct ModSettings {
    version: MapVersion,
    pub settings: ModSettingsData,
}

impl ModSettings {
    fn new_with_defaults(version_data: [u8; 9]) -> Self {
        let mut startup: ModSettingsScope = HashMap::new();

    pub fn load_from_file(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let mut stream: &[u8] = &buf;

        let version = MapVersion::load(&mut stream)?;

        let tree = load_ptree(&mut stream)?;

        if let PropertyTreeData::Dictionary(tree_val) = tree {
            let mut loading: ModSettingsData = HashMap::new();
            loading.insert("startup".to_string(), HashMap::new());
            loading.insert("runtime-global".to_string(), HashMap::new());
            loading.insert("runtime-per-user".to_string(), HashMap::new());

            for scope_name in ["startup", "runtime-global", "runtime-per-user"] {
                if let Some(scope_tree) = tree_val.get(scope_name)
                    && let PropertyTreeData::Dictionary(scope_val) = scope_tree
                {
                    let loading_scope = loading.get_mut(scope_name).unwrap();
                    for (key, wrapper) in scope_val {
                        if let PropertyTreeData::Dictionary(wrapper_val) = wrapper
                            && let Some(element) = wrapper_val.get("value")
                        {
                            match element {
                                PropertyTreeData::String(v) => {
                                    loading_scope
                                        .insert(key.clone(), ModSettingValue::String(v.clone()));
                                }
                                PropertyTreeData::Number(v) => {
                                    loading_scope.insert(key.clone(), ModSettingValue::Number(*v));
                                }
                                PropertyTreeData::SignedInteger(v) => {
                                    loading_scope.insert(key.clone(), ModSettingValue::Int(*v));
                                }
                                PropertyTreeData::Bool(v) => {
                                    loading_scope.insert(key.clone(), ModSettingValue::Bool(*v));
                                }
                                PropertyTreeData::Dictionary(val) => {
                                    if let (Some(r), Some(g), Some(b), Some(a)) = (
                                        val.get("r").and_then(|x| {
                                            if let PropertyTreeData::Number(n) = x {
                                                Some(*n)
                                            } else {
                                                None
                                            }
                                        }),
                                        val.get("g").and_then(|x| {
                                            if let PropertyTreeData::Number(n) = x {
                                                Some(*n)
                                            } else {
                                                None
                                            }
                                        }),
                                        val.get("b").and_then(|x| {
                                            if let PropertyTreeData::Number(n) = x {
                                                Some(*n)
                                            } else {
                                                None
                                            }
                                        }),
                                        val.get("a").and_then(|x| {
                                            if let PropertyTreeData::Number(n) = x {
                                                Some(*n)
                                            } else {
                                                None
                                            }
                                        }),
                                    ) {
                                        loading_scope.insert(
                                            key.clone(),
                                            ModSettingValue::Color { r, g, b, a },
                                        );
                                    } else {
                                        return Err(io::Error::new(
                                            io::ErrorKind::InvalidData,
                                            format!(
                                                "Unknown dictionary structure at {} {}",
                                                scope_name, key
                                            ),
                                        ));
                                    }
                                }
                                _ => {
                                    return Err(io::Error::new(
                                        io::ErrorKind::InvalidData,
                                        format!("Unexpected type in ModSettings: {:?}", element),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            Ok(Self {
                version,
                settings: loading,
            })
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Root tree is not a dictionary",
            ))
        }
    }

    pub fn save_to_file(&self, path: &Path) -> io::Result<()> {
        let mut writer = BufWriter::new(File::create(path)?);

        self.version.save(&mut writer)?;

        let mut tree: HashMap<String, PropertyTreeData> = HashMap::new();
        for scope_name in ["startup", "runtime-global", "runtime-per-user"] {
            let map = HashMap::new();
            let scope = self.settings.get(scope_name).unwrap_or(&map);
            let mut tree_scope: HashMap<String, PropertyTreeData> = HashMap::new();
            for (key, element) in scope {
                let value_tree = match element {
                    ModSettingValue::String(v) => PropertyTreeData::String(v.clone()),
                    ModSettingValue::Bool(v) => PropertyTreeData::Bool(*v),
                    ModSettingValue::Int(v) => PropertyTreeData::SignedInteger(*v),
                    ModSettingValue::Number(v) => PropertyTreeData::Number(*v),
                    ModSettingValue::Color { r, g, b, a } => {
                        let mut color_dict: HashMap<String, PropertyTreeData> = HashMap::new();
                        color_dict.insert("r".to_string(), PropertyTreeData::Number(*r));
                        color_dict.insert("g".to_string(), PropertyTreeData::Number(*g));
                        color_dict.insert("b".to_string(), PropertyTreeData::Number(*b));
                        color_dict.insert("a".to_string(), PropertyTreeData::Number(*a));
                        PropertyTreeData::Dictionary(color_dict)
                    }
                };
                let mut wrapper: HashMap<String, PropertyTreeData> = HashMap::new();
                wrapper.insert("value".to_string(), value_tree);
                tree_scope.insert(key.clone(), PropertyTreeData::Dictionary(wrapper));
            }
            tree.insert(
                scope_name.to_string(),
                PropertyTreeData::Dictionary(tree_scope),
            );
        }
        let root_tree = PropertyTreeData::Dictionary(tree);

        save_ptree(&root_tree, &mut writer)?;

        writer.flush()?;
        Ok(())
    }

    pub fn modify_setting(&mut self, name: &str, new_value: ModSettingValue) -> io::Result<()> {
        for scope in self.settings.values_mut() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), new_value);
                return Ok(());
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Setting '{}' not found in any scope", name),
        ))
    }
}

// --- PropertyTree Load/Save Implementations ---

fn load_ptree(reader: &mut impl Read) -> io::Result<PropertyTreeData> {
    let typ_byte = read_u8(reader)?;
    tracing::debug!(
        "DEBUG: Current PropertyTree Type byte: {} (decimal: {})",
        bytes_to_hex(&[typ_byte]),
        typ_byte
    );

    let is_any_type_byte = read_u8(reader)?;
    tracing::debug!(
        "DEBUG: Current PropertyTree isAnyType byte: {} (decimal: {})",
        bytes_to_hex(&[is_any_type_byte]),
        is_any_type_byte
    );

    let typ = match typ_byte {
        t if t == PropertyTreeType::None as u8 => PropertyTreeType::None,
        t if t == PropertyTreeType::Bool as u8 => PropertyTreeType::Bool,
        t if t == PropertyTreeType::Number as u8 => PropertyTreeType::Number,
        t if t == PropertyTreeType::String as u8 => PropertyTreeType::String,
        t if t == PropertyTreeType::List as u8 => PropertyTreeType::List,
        t if t == PropertyTreeType::Dictionary as u8 => PropertyTreeType::Dictionary,
        t if t == PropertyTreeType::SignedInteger as u8 => PropertyTreeType::SignedInteger,
        t if t == PropertyTreeType::UnsignedInteger as u8 => PropertyTreeType::UnsignedInteger,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid datatype in PropertyTree {}", typ_byte),
            ));
        }
    };

    let result = match typ {
        PropertyTreeType::None => Ok(PropertyTreeData::None),
        PropertyTreeType::Bool => Ok(PropertyTreeData::Bool(read_u8(reader)? != 0)),
        PropertyTreeType::Number => Ok(PropertyTreeData::Number(read_f64_le(reader)?)),
        PropertyTreeType::String => {
            let s = read_ptree_string(reader)?;
            tracing::debug!("DEBUG: Finished reading String: '{}'", s);
            Ok(PropertyTreeData::String(s))
        }
        PropertyTreeType::List => {
            let count = read_u32_le(reader)? as usize;
            tracing::debug!("DEBUG: Reading List with {} items", count);
            let mut arr = Vec::with_capacity(count);
            for i in 0..count {
                let disc_str = read_ptree_string(reader)?; // Discard (per TS)
                tracing::debug!("DEBUG: List item {}: discarded string '{}'", i, disc_str);
                arr.push(load_ptree(reader)?);
            }
            Ok(PropertyTreeData::List(arr))
        }
        PropertyTreeType::Dictionary => {
            let count = read_u32_le(reader)? as usize;
            tracing::debug!("DEBUG: Reading Dictionary with {} items", count);
            // This 0x00 byte after dictionary count seems consistently present in Factorio's format.
            read_u8(reader)?; // Explicitly consume the undocumented byte after count.

            let mut dict = HashMap::with_capacity(count);
            for i in 0..count {
                let key = read_ptree_string(reader)?;
                if key.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Missing key in PropertyTree Dictionary at item {}", i),
                    ));
                }
                tracing::debug!("DEBUG: Dictionary item {}: Key '{}'", i, key);
                let value = load_ptree(reader)?;
                dict.insert(key, value);
            }
            Ok(PropertyTreeData::Dictionary(dict))
        }
        PropertyTreeType::SignedInteger => {
            Ok(PropertyTreeData::SignedInteger(read_i64_le(reader)?))
        }
        PropertyTreeType::UnsignedInteger => {
            Ok(PropertyTreeData::UnsignedInteger(read_u64_le(reader)?))
        }
    };

    tracing::debug!(
        "DEBUG: Finished parsing PropertyTree (Type {}). Remaining bytes (peek 5):",
        typ_byte
    );
    // This peek is useful for debugging misalignment. Note: it consumes bytes from the reader!
    // For pure "peeking" without consuming, you'd need a more advanced buffered reader
    // or to rewind the stream if it supports it. For `&[u8]`, it consumes from the slice.
    let mut peek_buf = [0u8; 5];
    let peeked = reader.by_ref().take(5).read(&mut peek_buf)?;
    tracing::debug!(
        "DEBUG: Peeked {} bytes: {}",
        peeked,
        bytes_to_hex(&peek_buf[0..peeked])
    );
    // The peek consumes bytes, so subsequent reads will start after the peeked bytes.
    // If this is causing problems, remove the peek or make it truly non-consuming.

    // If you need to make the peek non-consuming, you'd need to convert `stream: &[u8]`
    // back to `std::io::Cursor<&[u8]>` after loading the initial buffer,
    // which supports `seek` to go back. But for now, let's keep it simple.

    result
}

fn save_ptree(tree: &PropertyTreeData, writer: &mut impl Write) -> io::Result<()> {
    match tree {
        PropertyTreeData::None => {
            write_u8(writer, PropertyTreeType::None as u8)?;
            write_u8(writer, 0)?; // isAnyType
            Ok(())
        }
        PropertyTreeData::Bool(v) => {
            write_u8(writer, PropertyTreeType::Bool as u8)?;
            write_u8(writer, 0)?;
            write_u8(writer, if *v { 1 } else { 0 })?;
            Ok(())
        }
        PropertyTreeData::Number(v) => {
            write_u8(writer, PropertyTreeType::Number as u8)?;
            write_u8(writer, 0)?;
            write_f64_le(writer, *v)?;
            Ok(())
        }
        PropertyTreeData::String(v) => {
            write_u8(writer, PropertyTreeType::String as u8)?;
            write_u8(writer, 0)?;
            save_ptree_string(v, writer)?;
            Ok(())
        }
        PropertyTreeData::List(arr) => {
            write_u8(writer, PropertyTreeType::List as u8)?;
            write_u8(writer, 0)?;
            write_u32_le(writer, arr.len() as u32)?;
            for item in arr {
                save_ptree_string("", writer)?; // Empty string per TS (discarded on load)
                save_ptree(item, writer)?;
            }
            Ok(())
        }
        PropertyTreeData::Dictionary(dict) => {
            write_u8(writer, PropertyTreeType::Dictionary as u8)?;
            write_u8(writer, 0)?;
            write_u32_le(writer, dict.len() as u32)?;
            // Symmetric: Write the undocumented 0x00 byte after dictionary count.
            write_u8(writer, 0)?;
            for (key, value) in dict {
                save_ptree_string(key, writer)?;
                save_ptree(value, writer)?;
            }
            Ok(())
        }
        PropertyTreeData::SignedInteger(v) => {
            write_u8(writer, PropertyTreeType::SignedInteger as u8)?;
            write_u8(writer, 0)?;
            write_i64_le(writer, *v)?;
            Ok(())
        }
        PropertyTreeData::UnsignedInteger(v) => {
            write_u8(writer, PropertyTreeType::UnsignedInteger as u8)?;
            write_u8(writer, 0)?;
            write_u64_le(writer, *v)?;
            Ok(())
        }
    }
}

// --- String Read/Save Implementations (Factorio Optimized Format) ---

fn read_ptree_string(reader: &mut impl Read) -> io::Result<String> {
    let len_prefix_byte = read_u8(reader)? as u32;
    let len = if len_prefix_byte == 255 {
        read_u32_le(reader)?
    } else {
        len_prefix_byte
    };

    let len = len as usize;
    let mut buf = vec![0u8; len];
    if len > 0 {
        reader.read_exact(&mut buf)?;
    }
    String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn save_ptree_string(s: &str, writer: &mut impl Write) -> io::Result<()> {
    let bytes = s.as_bytes();
    let len = bytes.len() as u32;
    if len < 255 {
        write_u8(writer, len as u8)?;
    } else {
        write_u8(writer, 255)?;
        write_u32_le(writer, len)?;
    }
    writer.write_all(bytes)?;
    Ok(())
}

// --- Low-Level Read/Write Helpers ---

fn read_u8(reader: &mut impl Read) -> io::Result<u8> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    tracing::debug!("DEBUG: read_u8 consumed: {}", bytes_to_hex(&buf));
    Ok(buf[0])
}

fn read_u32_le(reader: &mut impl Read) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    tracing::debug!("DEBUG: read_u32_le consumed: {}", bytes_to_hex(&buf));
    Ok(u32::from_le_bytes(buf))
}

fn read_i64_le(reader: &mut impl Read) -> io::Result<i64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    tracing::debug!("DEBUG: read_i64_le consumed: {}", bytes_to_hex(&buf));
    Ok(i64::from_le_bytes(buf))
}

fn read_u64_le(reader: &mut impl Read) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    tracing::debug!("DEBUG: read_u64_le consumed: {}", bytes_to_hex(&buf));
    Ok(u64::from_le_bytes(buf))
}

fn read_f64_le(reader: &mut impl Read) -> io::Result<f64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    tracing::debug!("DEBUG: read_f64_le consumed: {}", bytes_to_hex(&buf));
    Ok(f64::from_le_bytes(buf))
}

fn write_u8(writer: &mut impl Write, value: u8) -> io::Result<()> {
    writer.write_all(&[value])
}

fn write_u32_le(writer: &mut impl Write, value: u32) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn write_i64_le(writer: &mut impl Write, value: i64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn write_u64_le(writer: &mut impl Write, value: u64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn write_f64_le(writer: &mut impl Write, value: f64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}
