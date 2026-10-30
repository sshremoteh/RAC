use byteorder::{LittleEndian, WriteBytesExt};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AxmlError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML parsing error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("Malformed XML: {0}")]
    Malformed(String),
}

const RES_XML_TYPE: u16 = 0x0003;
const RES_STRING_POOL_TYPE: u16 = 0x0001;
const RES_XML_RESOURCE_MAP_TYPE: u16 = 0x0180;
const RES_XML_START_NAMESPACE_TYPE: u16 = 0x0100;
const RES_XML_END_NAMESPACE_TYPE: u16 = 0x0101;
const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
const RES_XML_END_ELEMENT_TYPE: u16 = 0x0103;

// Typed value types
const TYPE_STRING: u8 = 0x03;
const TYPE_INT_DEC: u8 = 0x10;
const TYPE_INT_HEX: u8 = 0x11;
const TYPE_INT_BOOLEAN: u8 = 0x12;

fn parse_attr_value(attr_name: &str, val: &str, str_idx: u32) -> (u8, u32) {
    if val == "true" {
        return (TYPE_INT_BOOLEAN, 0xFFFFFFFF);
    }
    if val == "false" {
        return (TYPE_INT_BOOLEAN, 0);
    }

    match attr_name {
        "screenOrientation" => {
            let code = match val {
                "unspecified" => Some(0xFFFFFFFF),
                "landscape" => Some(0),
                "portrait" => Some(1),
                "user" => Some(2),
                "behind" => Some(3),
                "sensor" => Some(4),
                "nosensor" => Some(5),
                "sensorLandscape" => Some(6),
                "sensorPortrait" => Some(7),
                "reverseLandscape" => Some(8),
                "reversePortrait" => Some(9),
                "fullSensor" => Some(10),
                "userLandscape" => Some(11),
                "userPortrait" => Some(12),
                "fullUser" => Some(13),
                "locked" => Some(14),
                _ => None,
            };
            if let Some(c) = code {
                return (TYPE_INT_DEC, c);
            }
        }
        "configChanges" => {
            let mut mask = 0u32;
            let mut matched = false;
            for part in val.split('|').map(|s| s.trim()) {
                let bit = match part {
                    "mcc" => Some(0x0001),
                    "mnc" => Some(0x0002),
                    "locale" => Some(0x0004),
                    "touchscreen" => Some(0x0008),
                    "keyboard" => Some(0x0010),
                    "keyboardHidden" => Some(0x0020),
                    "navigation" => Some(0x0040),
                    "orientation" => Some(0x0080),
                    "screenLayout" => Some(0x0100),
                    "uiMode" => Some(0x0200),
                    "screenSize" => Some(0x0400),
                    "smallestScreenSize" => Some(0x0800),
                    "density" => Some(0x1000),
                    "layoutDirection" => Some(0x2000),
                    "colorMode" => Some(0x4000),
                    "fontScale" => Some(0x40000000),
                    _ => None,
                };
                if let Some(b) = bit {
                    mask |= b;
                    matched = true;
                }
            }
            if matched {
                return (TYPE_INT_HEX, mask);
            }
        }
        "installLocation" => {
            let code = match val {
                "auto" => Some(0),
                "internalOnly" => Some(1),
                "preferExternal" => Some(2),
                _ => None,
            };
            if let Some(c) = code {
                return (TYPE_INT_DEC, c);
            }
        }
        "launchMode" => {
            let code = match val {
                "standard" => Some(0),
                "singleTop" => Some(1),
                "singleTask" => Some(2),
                "singleInstance" => Some(3),
                "singleInstancePerTask" => Some(4),
                _ => None,
            };
            if let Some(c) = code {
                return (TYPE_INT_DEC, c);
            }
        }
        "reqTouchScreen" => {
            let code = match val {
                "undefined" => Some(0),
                "notouch" => Some(1),
                "stylus" => Some(2),
                "finger" => Some(3),
                _ => None,
            };
            if let Some(c) = code {
                return (TYPE_INT_DEC, c);
            }
        }
        "reqKeyboardType" => {
            let code = match val {
                "undefined" => Some(0),
                "nokeys" => Some(1),
                "qwerty" => Some(2),
                "twelvekey" => Some(3),
                _ => None,
            };
            if let Some(c) = code {
                return (TYPE_INT_DEC, c);
            }
        }
        "reqNavigation" => {
            let code = match val {
                "undefined" => Some(0),
                "nonav" => Some(1),
                "dpad" => Some(2),
                "trackball" => Some(3),
                "wheel" => Some(4),
                _ => None,
            };
            if let Some(c) = code {
                return (TYPE_INT_DEC, c);
            }
        }
        _ => {}
    }

    if let Some(hex_str) = val.strip_prefix("0x").or_else(|| val.strip_prefix("0X")) {
        if let Ok(hex_val) = u32::from_str_radix(hex_str, 16) {
            return (TYPE_INT_HEX, hex_val);
        }
    }

    if let Ok(int_val) = val.parse::<u32>() {
        return (TYPE_INT_DEC, int_val);
    }

    (TYPE_STRING, str_idx)
}

// Android resource IDs for common manifest attributes
fn get_android_attr_res_id(attr_name: &str) -> Option<u32> {
    match attr_name {
        "theme" => Some(0x01010000),
        "label" => Some(0x01010001),
        "icon" => Some(0x01010002),
        "name" => Some(0x01010003),
        "permission" => Some(0x01010006),
        "exported" => Some(0x01010010),
        "process" => Some(0x01010011),
        "taskAffinity" => Some(0x01010012),
        "multiprocess" => Some(0x01010013),
        "finishOnTaskLaunch" => Some(0x01010014),
        "clearTaskOnLaunch" => Some(0x01010015),
        "stateNotNeeded" => Some(0x01010016),
        "screenOrientation" => Some(0x0101001e),
        "configChanges" => Some(0x0101001f),
        "minSdkVersion" => Some(0x0101020c),
        "maxSdkVersion" => Some(0x01010271),
        "targetSdkVersion" => Some(0x01010270),
        "versionCode" => Some(0x0101021b),
        "versionName" => Some(0x0101021c),
        "hasCode" => Some(0x01010265),
        "allowBackup" => Some(0x01010280),
        "required" => Some(0x0101028e),
        "hardwareAccelerated" => Some(0x010102d3),
        "extractNativeLibs" => Some(0x010104ea),
        "version" => Some(0x01010519),
        _ => None,
    }
}

pub struct StringPoolBuilder {
    strings: Vec<String>,
    indices: HashMap<String, u32>,
}

impl StringPoolBuilder {
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
            indices: HashMap::new(),
        }
    }

    pub fn get_or_insert(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.indices.get(s) {
            idx
        } else {
            let idx = self.strings.len() as u32;
            self.strings.push(s.to_string());
            self.indices.insert(s.to_string(), idx);
            idx
        }
    }

    pub fn build_utf8_chunk(&self) -> Result<Vec<u8>, AxmlError> {
        let string_count = self.strings.len() as u32;
        let mut string_data = Vec::new();
        let mut offsets = Vec::new();

        for s in &self.strings {
            offsets.push(string_data.len() as u32);
            let bytes = s.as_bytes();
            let char_len = s.chars().count();
            let byte_len = bytes.len();

            // Encode character length (UTF-8 length header: 1 or 2 bytes)
            if char_len <= 0x7f {
                string_data.push(char_len as u8);
            } else {
                string_data.push((((char_len >> 8) & 0x7f) | 0x80) as u8);
                string_data.push((char_len & 0xff) as u8);
            }

            // Encode byte length (1 or 2 bytes)
            if byte_len <= 0x7f {
                string_data.push(byte_len as u8);
            } else {
                string_data.push((((byte_len >> 8) & 0x7f) | 0x80) as u8);
                string_data.push((byte_len & 0xff) as u8);
            }

            string_data.extend_from_slice(bytes);
            string_data.push(0x00); // null terminator
        }

        // Align string_data to 4 bytes
        while string_data.len() % 4 != 0 {
            string_data.push(0x00);
        }

        let header_size = 28u16;
        let offsets_size = (string_count * 4) as usize;
        let strings_start = (header_size as u32) + (offsets_size as u32);
        let chunk_size = strings_start + (string_data.len() as u32);

        let mut buf = Vec::new();
        buf.write_u16::<LittleEndian>(RES_STRING_POOL_TYPE)?;
        buf.write_u16::<LittleEndian>(header_size)?;
        buf.write_u32::<LittleEndian>(chunk_size)?;
        buf.write_u32::<LittleEndian>(string_count)?;
        buf.write_u32::<LittleEndian>(0)?; // style_count
        buf.write_u32::<LittleEndian>(1 << 8)?; // flags: UTF-8 = 0x0100
        buf.write_u32::<LittleEndian>(strings_start)?;
        buf.write_u32::<LittleEndian>(0)?; // styles_start

        for offset in offsets {
            buf.write_u32::<LittleEndian>(offset)?;
        }
        buf.extend_from_slice(&string_data);

        Ok(buf)
    }
}

enum XmlNode {
    StartNamespace { prefix: String, uri: String },
    EndNamespace { prefix: String, uri: String },
    StartElement {
        ns: Option<String>,
        name: String,
        attributes: Vec<RawAttribute>,
    },
    EndElement { ns: Option<String>, name: String },
}

struct RawAttribute {
    ns: Option<String>,
    name: String,
    value: String,
}

pub fn compile_xml_to_axml(xml_str: &str) -> Result<Vec<u8>, AxmlError> {
    let mut reader = Reader::from_str(xml_str);
    reader.config_mut().trim_text(true);

    let mut nodes = Vec::new();
    let mut pool = StringPoolBuilder::new();

    // Default Android namespace
    let android_ns_uri = "http://schemas.android.com/apk/res/android";
    let android_prefix = "android";

    pool.get_or_insert(android_prefix);
    pool.get_or_insert(android_ns_uri);

    // Initial namespace start node
    nodes.push(XmlNode::StartNamespace {
        prefix: android_prefix.to_string(),
        uri: android_ns_uri.to_string(),
    });

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let elem_name = String::from_utf8_lossy(e.name().into_inner()).to_string();
                pool.get_or_insert(&elem_name);

                let mut attrs = Vec::new();
                for attr_res in e.attributes() {
                    let attr = attr_res.map_err(|e| AxmlError::Malformed(e.to_string()))?;
                    let key = String::from_utf8_lossy(attr.key.into_inner()).to_string();
                    let val = String::from_utf8_lossy(&attr.value).to_string();

                    let (ns, name) = if let Some(stripped) = key.strip_prefix("android:") {
                        (Some(android_ns_uri.to_string()), stripped.to_string())
                    } else if key == "xmlns:android" {
                        continue;
                    } else {
                        (None, key)
                    };

                    pool.get_or_insert(&name);
                    pool.get_or_insert(&val);

                    attrs.push(RawAttribute { ns, name, value: val });
                }

                nodes.push(XmlNode::StartElement {
                    ns: None,
                    name: elem_name,
                    attributes: attrs,
                });
            }
            Event::Empty(e) => {
                let elem_name = String::from_utf8_lossy(e.name().into_inner()).to_string();
                pool.get_or_insert(&elem_name);

                let mut attrs = Vec::new();
                for attr_res in e.attributes() {
                    let attr = attr_res.map_err(|e| AxmlError::Malformed(e.to_string()))?;
                    let key = String::from_utf8_lossy(attr.key.into_inner()).to_string();
                    let val = String::from_utf8_lossy(&attr.value).to_string();

                    let (ns, name) = if let Some(stripped) = key.strip_prefix("android:") {
                        (Some(android_ns_uri.to_string()), stripped.to_string())
                    } else if key == "xmlns:android" {
                        continue;
                    } else {
                        (None, key)
                    };

                    pool.get_or_insert(&name);
                    pool.get_or_insert(&val);

                    attrs.push(RawAttribute { ns, name, value: val });
                }

                nodes.push(XmlNode::StartElement {
                    ns: None,
                    name: elem_name.clone(),
                    attributes: attrs,
                });
                nodes.push(XmlNode::EndElement {
                    ns: None,
                    name: elem_name,
                });
            }
            Event::End(e) => {
                let elem_name = String::from_utf8_lossy(e.name().into_inner()).to_string();
                nodes.push(XmlNode::EndElement {
                    ns: None,
                    name: elem_name,
                });
            }
            Event::Eof => break,
            _ => (),
        }
        buf.clear();
    }

    nodes.push(XmlNode::EndNamespace {
        prefix: android_prefix.to_string(),
        uri: android_ns_uri.to_string(),
    });

    // Build String Pool
    let string_pool_chunk = pool.build_utf8_chunk()?;

    // Build Resource Map chunk (maps string pool indices to android attr resource IDs)
    let mut res_map_chunk = Vec::new();
    let res_map_size = 8 + (pool.strings.len() * 4) as u32;
    res_map_chunk.write_u16::<LittleEndian>(RES_XML_RESOURCE_MAP_TYPE)?;
    res_map_chunk.write_u16::<LittleEndian>(8)?;
    res_map_chunk.write_u32::<LittleEndian>(res_map_size)?;
    for s in &pool.strings {
        let res_id = get_android_attr_res_id(s).unwrap_or(0);
        res_map_chunk.write_u32::<LittleEndian>(res_id)?;
    }

    // Build XML Nodes Chunks
    let mut nodes_chunk = Vec::new();
    for node in nodes {
        match node {
            XmlNode::StartNamespace { prefix, uri } => {
                let p_idx = pool.get_or_insert(&prefix);
                let u_idx = pool.get_or_insert(&uri);
                nodes_chunk.write_u16::<LittleEndian>(RES_XML_START_NAMESPACE_TYPE)?;
                nodes_chunk.write_u16::<LittleEndian>(16)?; // header size
                nodes_chunk.write_u32::<LittleEndian>(24)?; // chunk size
                nodes_chunk.write_u32::<LittleEndian>(1)?;  // line number
                nodes_chunk.write_u32::<LittleEndian>(0xFFFFFFFF)?; // comment
                nodes_chunk.write_u32::<LittleEndian>(p_idx)?;
                nodes_chunk.write_u32::<LittleEndian>(u_idx)?;
            }
            XmlNode::EndNamespace { prefix, uri } => {
                let p_idx = pool.get_or_insert(&prefix);
                let u_idx = pool.get_or_insert(&uri);
                nodes_chunk.write_u16::<LittleEndian>(RES_XML_END_NAMESPACE_TYPE)?;
                nodes_chunk.write_u16::<LittleEndian>(16)?;
                nodes_chunk.write_u32::<LittleEndian>(24)?;
                nodes_chunk.write_u32::<LittleEndian>(1)?;
                nodes_chunk.write_u32::<LittleEndian>(0xFFFFFFFF)?;
                nodes_chunk.write_u32::<LittleEndian>(p_idx)?;
                nodes_chunk.write_u32::<LittleEndian>(u_idx)?;
            }
            XmlNode::StartElement { ns, name, attributes } => {
                let ns_idx = ns.map(|u| pool.get_or_insert(&u)).unwrap_or(0xFFFFFFFF);
                let name_idx = pool.get_or_insert(&name);
                let attr_count = attributes.len() as u16;
                let chunk_size = 36 + (attr_count as u32) * 20;

                nodes_chunk.write_u16::<LittleEndian>(RES_XML_START_ELEMENT_TYPE)?;
                nodes_chunk.write_u16::<LittleEndian>(16)?;
                nodes_chunk.write_u32::<LittleEndian>(chunk_size)?;
                nodes_chunk.write_u32::<LittleEndian>(1)?; // line
                nodes_chunk.write_u32::<LittleEndian>(0xFFFFFFFF)?; // comment
                nodes_chunk.write_u32::<LittleEndian>(ns_idx)?;
                nodes_chunk.write_u32::<LittleEndian>(name_idx)?;
                nodes_chunk.write_u16::<LittleEndian>(20)?; // attr_start
                nodes_chunk.write_u16::<LittleEndian>(20)?; // attr_size
                nodes_chunk.write_u16::<LittleEndian>(attr_count)?;
                nodes_chunk.write_u16::<LittleEndian>(0)?; // id_index
                nodes_chunk.write_u16::<LittleEndian>(0)?; // class_index
                nodes_chunk.write_u16::<LittleEndian>(0)?; // style_index

                for attr in attributes {
                    let a_ns_idx = attr.ns.map(|u| pool.get_or_insert(&u)).unwrap_or(0xFFFFFFFF);
                    let a_name_idx = pool.get_or_insert(&attr.name);
                    let a_val_idx = pool.get_or_insert(&attr.value);

                    // Parse value into typed value (hex integer, decimal integer, boolean, enum, flags, or string)
                    let (data_type, data_val) = parse_attr_value(&attr.name, &attr.value, a_val_idx);

                    nodes_chunk.write_u32::<LittleEndian>(a_ns_idx)?;
                    nodes_chunk.write_u32::<LittleEndian>(a_name_idx)?;
                    nodes_chunk.write_u32::<LittleEndian>(a_val_idx)?;
                    nodes_chunk.write_u16::<LittleEndian>(8)?; // typed_value size
                    nodes_chunk.write_u8(0)?; // res0
                    nodes_chunk.write_u8(data_type)?;
                    nodes_chunk.write_u32::<LittleEndian>(data_val)?;
                }
            }
            XmlNode::EndElement { ns, name } => {
                let ns_idx = ns.map(|u| pool.get_or_insert(&u)).unwrap_or(0xFFFFFFFF);
                let name_idx = pool.get_or_insert(&name);

                nodes_chunk.write_u16::<LittleEndian>(RES_XML_END_ELEMENT_TYPE)?;
                nodes_chunk.write_u16::<LittleEndian>(16)?;
                nodes_chunk.write_u32::<LittleEndian>(24)?;
                nodes_chunk.write_u32::<LittleEndian>(1)?;
                nodes_chunk.write_u32::<LittleEndian>(0xFFFFFFFF)?;
                nodes_chunk.write_u32::<LittleEndian>(ns_idx)?;
                nodes_chunk.write_u32::<LittleEndian>(name_idx)?;
            }
        }
    }

    // Assemble file
    let total_file_size = 8 + string_pool_chunk.len() + res_map_chunk.len() + nodes_chunk.len();
    let mut final_axml = Vec::with_capacity(total_file_size);

    final_axml.write_u16::<LittleEndian>(RES_XML_TYPE)?;
    final_axml.write_u16::<LittleEndian>(8)?;
    final_axml.write_u32::<LittleEndian>(total_file_size as u32)?;
    final_axml.extend_from_slice(&string_pool_chunk);
    final_axml.extend_from_slice(&res_map_chunk);
    final_axml.extend_from_slice(&nodes_chunk);

    Ok(final_axml)
}
