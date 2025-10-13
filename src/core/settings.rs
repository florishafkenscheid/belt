use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Read},
    path::Path,
};

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum PropertyTreeType {
    None = 0,
    Bool = 1,
    Number = 2,
    String = 3,
    List = 4,
    Dictionary = 5,
    SignedInteger = 6,
    UnsignedInteger = 7,
}

impl From<u8> for PropertyTreeType {
    fn from(value: u8) -> Self {
        match value {
            0 => PropertyTreeType::None,
            1 => PropertyTreeType::Bool,
            2 => PropertyTreeType::Number,
            3 => PropertyTreeType::String,
            4 => PropertyTreeType::List,
            5 => PropertyTreeType::Dictionary,
            6 => PropertyTreeType::SignedInteger,
            7 => PropertyTreeType::UnsignedInteger,
            _ => panic!("Invalid PropertyTreeType: {value}"),
        }
    }
}

#[derive(Debug, Clone)]
struct MapVersion {
    data: [u8; 9],
}

impl MapVersion {
    fn from_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let mut data = [0u8; 9];
        r.read_exact(&mut data)?;
        Ok(Self { data })
    }

    fn to_bytes(&self) -> [u8; 9] {
        self.data
    }
}

pub trait BufferStream: Read {
    fn read_u8(&mut self) -> io::Result<u8> {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn read_u32_le(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_i64_le(&mut self) -> io::Result<i64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(i64::from_le_bytes(buf))
    }

    fn read_u64_le(&mut self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn read_f64_le(&mut self) -> io::Result<f64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(f64::from_le_bytes(buf))
    }

    fn read_packed_uint_8_32(&mut self) -> io::Result<u32> {
        let first = self.read_u8()?;
        if first < 255 {
            Ok(first as u32)
        } else {
            self.read_u32_le()
        }
    }

    fn read_string(&mut self, size: u32) -> io::Result<String> {
        let mut buf = vec![0u8; size as usize];
        self.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

impl<R: Read> BufferStream for R {}

fn read_ptree_string<R: BufferStream>(b: &mut R) -> io::Result<String> {
    let empty = b.read_u8()? != 0;
    if empty {
        Ok(String::new())
    } else {
        let size = b.read_packed_uint_8_32()?;
        b.read_string(size)
    }
}

fn load_ptree<R: BufferStream>(b: &mut R) -> io::Result<PropertyTreeData> {
    let type_val = b.read_u8()?;
    let _is_any_type = b.read_u8()?;

    let tree_type = PropertyTreeType::from(type_val);

    match tree_type {
        PropertyTreeType::None => Ok(PropertyTreeData::None),

        PropertyTreeType::Bool => {
            let value = b.read_u8()? != 0;
            Ok(PropertyTreeData::Bool(value))
        }

        PropertyTreeType::Number => {
            let value = b.read_f64_le()?;
            Ok(PropertyTreeData::Number(value))
        }

        PropertyTreeType::String => {
            let value = read_ptree_string(b)?;
            Ok(PropertyTreeData::String(value))
        }

        PropertyTreeType::List => {
            let count = b.read_u32_le()?;
            let mut arr = Vec::with_capacity(count as usize);

            for _ in 0..count {
                let _key = read_ptree_string(b)?; // discard key for list items
                let value = load_ptree(b)?;
                arr.push(value);
            }

            Ok(PropertyTreeData::List(arr))
        }

        PropertyTreeType::Dictionary => {
            let count = b.read_u32_le()?;
            let mut dict = HashMap::with_capacity(count as usize);

            for _ in 0..count {
                let key = read_ptree_string(b)?;
                if key.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Missing key in PropertyTree Dictionary",
                    ));
                }
                let value = load_ptree(b)?;
                dict.insert(key, value);
            }

            Ok(PropertyTreeData::Dictionary(dict))
        }

        PropertyTreeType::SignedInteger => {
            let value = b.read_i64_le()?;
            Ok(PropertyTreeData::SignedInteger(value))
        }

        PropertyTreeType::UnsignedInteger => {
            let value = b.read_u64_le()?;
            Ok(PropertyTreeData::UnsignedInteger(value))
        }
    }
}

fn save_string(s: &str) -> Vec<u8> {
    if s.is_empty() {
        vec![1] // no string
    } else {
        let str_bytes = s.as_bytes();
        let mut result = vec![0];

        if str_bytes.len() < 255 {
            result.push(str_bytes.len() as u8);
        } else {
            result.push(255);
            result.extend_from_slice(&(str_bytes.len() as u32).to_le_bytes());
        }

        result.extend_from_slice(str_bytes);
        result
    }
}

fn type_tag(tree_type: PropertyTreeType) -> Vec<u8> {
    vec![tree_type as u8, 0]
}

fn save_ptree(tree: &PropertyTreeData) -> Vec<u8> {
    match tree {
        PropertyTreeData::String(value) => {
            let mut result = type_tag(PropertyTreeType::String);
            result.extend_from_slice(&save_string(value));
            result
        }

        PropertyTreeData::Bool(value) => {
            let mut result = type_tag(PropertyTreeType::Bool);
            result.push(if *value { 1 } else { 0 });
            result
        }

        PropertyTreeData::Number(value) => {
            let mut result = type_tag(PropertyTreeType::Number);
            result.extend_from_slice(&value.to_le_bytes());
            result
        }

        PropertyTreeData::SignedInteger(value) => {
            let mut result = type_tag(PropertyTreeType::SignedInteger);
            result.extend_from_slice(&value.to_le_bytes());
            result
        }

        PropertyTreeData::UnsignedInteger(value) => {
            let mut result = type_tag(PropertyTreeType::UnsignedInteger);
            result.extend_from_slice(&value.to_le_bytes());
            result
        }

        PropertyTreeData::None => type_tag(PropertyTreeType::None),

        PropertyTreeData::List(values) => {
            let mut result = type_tag(PropertyTreeType::List);
            result.extend_from_slice(&(values.len() as u32).to_le_bytes());

            for value in values {
                result.extend_from_slice(&save_string("")); // no key for list items
                result.extend_from_slice(&save_ptree(value));
            }

            result
        }

        PropertyTreeData::Dictionary(dict) => {
            let mut result = type_tag(PropertyTreeType::Dictionary);
            result.extend_from_slice(&(dict.len() as u32).to_le_bytes());

            for (key, value) in dict {
                result.extend_from_slice(&save_string(key));
                result.extend_from_slice(&save_ptree(value));
            }

            result
        }
    }
}

// Settings
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModSettingsScopeName {
    Startup,
    RuntimeGlobal,
    RuntimePerUser,
}

impl ModSettingsScopeName {
    fn as_str(&self) -> &'static str {
        match self {
            ModSettingsScopeName::Startup => "startup",
            ModSettingsScopeName::RuntimeGlobal => "runtime-global",
            ModSettingsScopeName::RuntimePerUser => "runtime-per-user",
        }
    }

    const ALL: [Self; 3] = [
        ModSettingsScopeName::Startup,
        ModSettingsScopeName::RuntimeGlobal,
        ModSettingsScopeName::RuntimePerUser,
    ];
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModSettingsValue {
    String(String),
    Number(f64),
    Int(i64),
    Bool(bool),
    Color { r: f64, g: f64, b: f64, a: f64 },
}

type ModSettingsScope = HashMap<String, ModSettingsValue>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModSettingsData {
    startup: ModSettingsScope,
    runtime_global: ModSettingsScope,
    runtime_per_user: ModSettingsScope,
}

impl ModSettingsData {
    fn scope_mut(&mut self, s: ModSettingsScopeName) -> &mut ModSettingsScope {
        match s {
            ModSettingsScopeName::Startup => &mut self.startup,
            ModSettingsScopeName::RuntimeGlobal => &mut self.runtime_global,
            ModSettingsScopeName::RuntimePerUser => &mut self.runtime_per_user,
        }
    }

    fn scope_ref(&self, s: ModSettingsScopeName) -> &ModSettingsScope {
        match s {
            ModSettingsScopeName::Startup => &self.startup,
            ModSettingsScopeName::RuntimeGlobal => &self.runtime_global,
            ModSettingsScopeName::RuntimePerUser => &self.runtime_per_user,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModSettings {
    version: MapVersion,
    pub settings: ModSettingsData,
}

impl ModSettings {
    fn from_reader<R: Read>(mut r: R) -> io::Result<Self> {
        let mut br = io::BufReader::new(&mut r);
        let version = MapVersion::from_reader(&mut br)?;
        let tree = load_ptree(&mut br)?;

        let mut loading = ModSettingsData::default();

        let root = match tree {
            PropertyTreeData::Dictionary(d) => d,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ModSettings root is not a dictionary",
                ));
            }
        };

        for scopename in ModSettingsScopeName::ALL {
            let scope_tree = root.get(scopename.as_str()).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Missing scope '{}'", scopename.as_str()),
                )
            })?;

            let scope_dict = match scope_tree {
                PropertyTreeData::Dictionary(d) => d,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Scope '{}' is not a dictionary", scopename.as_str()),
                    ));
                }
            };

            let loadingscope = loading.scope_mut(scopename);

            for (key, wrapper) in scope_dict {
                let wrapper_dict = match wrapper {
                    PropertyTreeData::Dictionary(d) => d,
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Wrapper for key '{key}' is not a dictionary"),
                        ));
                    }
                };

                let element = wrapper_dict.get("value").ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Missing 'value' for key '{key}'"),
                    )
                })?;

                let parsed = match element {
                    PropertyTreeData::String(s) => ModSettingsValue::String(s.clone()),
                    PropertyTreeData::Number(n) => ModSettingsValue::Number(*n),
                    PropertyTreeData::SignedInteger(i) => ModSettingsValue::Int(*i),
                    PropertyTreeData::Bool(b) => ModSettingsValue::Bool(*b),
                    PropertyTreeData::Dictionary(d) => {
                        // Check for color structure {r,g,b,a} as numbers
                        let get_num = |name: &str| -> io::Result<f64> {
                            match d.get(name) {
                                Some(PropertyTreeData::Number(v)) => Ok(*v),
                                _ => Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("Dictionary for '{key}' missing number '{name}'"),
                                )),
                            }
                        };

                        let r = get_num("r")?;
                        let g = get_num("g")?;
                        let b = get_num("b")?;
                        let a = get_num("a")?;

                        ModSettingsValue::Color { r, g, b, a }
                    }
                    other => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "Unexpected type in ModSettings Tree for key '{key}': {other:?}",
                            ),
                        ));
                    }
                };

                loadingscope.insert(key.clone(), parsed);
            }
        }

        Ok(Self {
            version,
            settings: loading,
        })
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut f = File::open(path)?;
        Self::from_reader(&mut f)
    }

    pub fn set(
        &mut self,
        scope: ModSettingsScopeName,
        key: impl Into<String>,
        value: Option<ModSettingsValue>,
    ) -> Option<ModSettingsValue> {
        let scope_map = self.settings.scope_mut(scope);
        let k = key.into();
        match value {
            Some(v) => scope_map.insert(k, v),
            None => scope_map.remove(&k),
        }
    }

    fn to_ptree(&self) -> PropertyTreeData {
        let mut root = HashMap::new();

        for scope_name in ModSettingsScopeName::ALL {
            let scope_map = self.settings.scope_ref(scope_name);
            let mut scope_pt = HashMap::new();

            for (key, value) in scope_map.iter() {
                let data = match value {
                    ModSettingsValue::String(s) => PropertyTreeData::String(s.clone()),
                    ModSettingsValue::Number(n) => PropertyTreeData::Number(*n),
                    ModSettingsValue::Int(i) => PropertyTreeData::SignedInteger(*i),
                    ModSettingsValue::Bool(b) => PropertyTreeData::Bool(*b),
                    ModSettingsValue::Color { r, g, b, a } => {
                        let mut color_dict = HashMap::new();
                        color_dict.insert("r".into(), PropertyTreeData::Number(*r));
                        color_dict.insert("g".into(), PropertyTreeData::Number(*g));
                        color_dict.insert("b".into(), PropertyTreeData::Number(*b));
                        color_dict.insert("a".into(), PropertyTreeData::Number(*a));
                        PropertyTreeData::Dictionary(color_dict)
                    }
                };

                // Wrap as { value: <data> } same as TS
                let mut wrapper = HashMap::new();
                wrapper.insert("value".into(), data);

                scope_pt.insert(key.clone(), PropertyTreeData::Dictionary(wrapper));
            }

            root.insert(
                scope_name.as_str().to_string(),
                PropertyTreeData::Dictionary(scope_pt),
            );
        }

        PropertyTreeData::Dictionary(root)
    }

    fn save_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.version.to_bytes());
        let tree = self.to_ptree();
        out.extend_from_slice(&save_ptree(&tree));
        out
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let bytes = self.save_bytes();
        fs::write(path, bytes)
    }
}
