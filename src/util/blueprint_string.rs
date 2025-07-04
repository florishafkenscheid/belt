// See https://wiki.factorio.com/Blueprint_string_format#Json_representation_of_a_blueprint/blueprint_book

use base64::{prelude::BASE64_STANDARD, Engine};
use flate2::read::ZlibDecoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;

/// Parse a Factorio blueprint string and return the decoded JSON data.
/// 
/// Blueprint strings follow this format:
/// 1. Version byte (currently 0)
/// 2. Base64 encoded zlib compressed JSON data
pub fn from_str(blueprint_string: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Remove any whitespace and ensure we have at least one character
    let trimmed = blueprint_string.trim();
    if trimmed.is_empty() {
        return Err("Blueprint string is empty".into());
    }

    // Extract version byte and data
    let version_byte = trimmed.chars().next().unwrap();
    if version_byte != '0' {
        return Err(format!("Unsupported blueprint version: {}", version_byte).into());
    }

    // Get the base64 encoded data (skip the first character which is the version)
    let base64_data = &trimmed[1..];
    
    // Decode from base64
    let compressed_data = BASE64_STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    // Decompress using zlib
    let mut decoder = ZlibDecoder::new(&compressed_data[..]);
    let mut decompressed_data = Vec::new();
    match decoder.read_to_end(&mut decompressed_data) {
        Ok(_) => Ok(decompressed_data),
        Err(e) => {
            // Try raw deflate if zlib fails
            use flate2::read::DeflateDecoder;
            let mut decoder = DeflateDecoder::new(&compressed_data[..]);
            let mut decompressed_data = Vec::new();
            decoder
                .read_to_end(&mut decompressed_data)
                .map_err(|e2| format!("Failed to decompress both zlib and raw deflate: zlib={}, deflate={}", e, e2))?;
            Ok(decompressed_data)
        }
    }
}

/// Data structures representing Factorio blueprint format
/// Based on https://wiki.factorio.com/Blueprint_string_format

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintString {
    pub blueprint: Option<Blueprint>,
    pub blueprint_book: Option<BlueprintBook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blueprint {
    pub item: String,
    pub label: Option<String>,
    pub label_color: Option<Color>,
    pub entities: Option<Vec<Entity>>,
    pub tiles: Option<Vec<Tile>>,
    pub icons: Option<Vec<Icon>>,
    pub schedules: Option<Vec<Schedule>>,
    pub description: Option<String>,
    #[serde(rename = "snap-to-grid")]
    pub snap_to_grid: Option<Position>,
    #[serde(rename = "absolute-snapping")]
    pub absolute_snapping: Option<bool>,
    #[serde(rename = "position-relative-to-grid")]
    pub position_relative_to_grid: Option<Position>,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintBook {
    pub item: String,
    pub label: Option<String>,
    pub label_color: Option<Color>,
    pub blueprints: Vec<BlueprintEntry>,
    pub active_index: u32,
    pub icons: Option<Vec<Icon>>,
    pub description: Option<String>,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintEntry {
    pub index: u32,
    pub blueprint: Blueprint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub entity_number: u32,
    pub name: String,
    pub position: Position,
    pub direction: Option<u32>,
    pub orientation: Option<f64>,
    pub connections: Option<HashMap<String, Connection>>,
    pub neighbours: Option<Vec<u32>>,
    pub control_behavior: Option<Value>, // Complex nested object, using Value for flexibility
    pub items: Option<Value>, // Can be HashMap<String, u32> or complex inventory items
    pub recipe: Option<String>,
    pub bar: Option<u32>,
    pub ammo_inventory: Option<Inventory>,
    pub trunk_inventory: Option<Inventory>,
    pub inventory: Option<Inventory>,
    pub infinity_settings: Option<InfinitySettings>,
    #[serde(rename = "type")]
    pub entity_type: Option<String>,
    pub input_priority: Option<String>,
    pub output_priority: Option<String>,
    pub filter: Option<Filter>,
    pub filters: Option<Vec<ItemFilter>>,
    pub filter_mode: Option<String>,
    pub override_stack_size: Option<u8>,
    pub drop_position: Option<Position>,
    pub pickup_position: Option<Position>,
    pub request_filters: Option<RequestFilters>,
    pub request_from_buffers: Option<bool>,
    pub parameters: Option<Value>, // Speaker parameters
    pub alert_parameters: Option<Value>, // Speaker alert parameters
    pub auto_launch: Option<bool>,
    pub variation: Option<u32>,
    pub color: Option<Color>,
    pub station: Option<String>,
    pub manual_trains_limit: Option<u32>,
    pub switch_state: Option<bool>,
    pub tags: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Icon {
    pub index: u32,
    pub signal: SignalId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalId {
    pub name: String,
    #[serde(rename = "type")]
    pub signal_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tile {
    pub name: String,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    #[serde(rename = "1")]
    pub first: Option<ConnectionPoint>,
    #[serde(rename = "2")]
    pub second: Option<ConnectionPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoint {
    pub red: Option<Vec<ConnectionData>>,
    pub green: Option<Vec<ConnectionData>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionData {
    pub entity_id: u32,
    pub circuit_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemFilter {
    pub name: String,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogisticFilter {
    pub name: String,
    pub index: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub filters: Option<Vec<ItemFilter>>,
    pub bar: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfinitySettings {
    pub remove_unfiltered_items: bool,
    pub filters: Option<Vec<InfinityFilter>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfinityFilter {
    pub name: String,
    pub count: u32,
    pub mode: String,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub schedule: Vec<ScheduleRecord>,
    pub locomotives: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRecord {
    pub station: String,
    pub wait_conditions: Vec<WaitCondition>,
    pub temporary: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitCondition {
    #[serde(rename = "type")]
    pub condition_type: String,
    pub compare_type: String,
    pub ticks: Option<u32>,
    pub condition: Option<Value>, // CircuitCondition object
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFilters {
    pub sections: Vec<Section>,
    #[serde(default)]
    pub trash_not_requested: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub index: u32,
    pub filters: Option<Vec<Filter>>,
    pub multiplier: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub name: String,
    pub quality: Option<String>,
    pub comparator: Option<String>,
}

/// Parse a blueprint string into a structured BlueprintString object
pub fn parse_blueprint(blueprint_string: &str) -> Result<BlueprintString, Box<dyn std::error::Error>> {
    let json_data = from_str(blueprint_string)?;
    let json_str = String::from_utf8(json_data)?;
    let blueprint: BlueprintString = serde_json::from_str(&json_str)?;
    Ok(blueprint)
}

/// Save the decompressed JSON from a blueprint string to a file
pub fn save_blueprint_json<P: AsRef<Path>>(blueprint: &BlueprintString, file_path: P) -> Result<(), Box<dyn std::error::Error>> {
    let json_str = serde_json::to_string_pretty(blueprint)?;
    let mut file = File::create(file_path)?;
    file.write_all(json_str.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blueprint_string_decode() {
        // Test basic decoding functionality with the vanilla mall blueprint
        let vanilla_mall = include_str!("../../blueprints/vanilla_mall.txt");
        
        // Test raw decoding first
        let result = from_str(vanilla_mall.trim());
        assert!(result.is_ok(), "Failed to decode blueprint string: {:?}", result.err());
        
        let json_data = result.unwrap();
        let json_str = String::from_utf8(json_data).unwrap();
        
        // Verify we got a reasonable amount of JSON data
        assert!(json_str.len() > 1000, "JSON data seems too small");
        
        // Verify it's valid JSON
        let json_value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(json_value.is_object(), "JSON should be an object");
        
        // Test parsing with our BlueprintString struct
        let blueprint = parse_blueprint(vanilla_mall.trim()).expect("Failed to parse blueprint");
        
        // Verify the parsed structure
        assert!(blueprint.blueprint.is_some(), "Should have a blueprint");
        assert!(blueprint.blueprint_book.is_none(), "Should not have a blueprint book");
        
        let bp = blueprint.blueprint.unwrap();
        assert_eq!(bp.item, "blueprint", "Item should be 'blueprint'");
        assert!(bp.entities.is_some(), "Blueprint should have entities");
        assert!(bp.label.is_some(), "Blueprint should have a label");
        
        let entities = bp.entities.unwrap();
        assert!(entities.len() > 100, "Should have many entities in this mall blueprint");
        
        // Verify some entities have the complex structures we added support for
        let has_request_filters = entities.iter().any(|e| e.request_filters.is_some());
        let has_filters = entities.iter().any(|e| e.filter.is_some());
        
        assert!(has_request_filters, "Should have entities with request_filters");
        assert!(has_filters, "Should have entities with filters");
    }

    #[test]
    fn test_complex_structures() {
        // Test that our complex data structures (RequestFilters and Filter) serialize/deserialize correctly
        let vanilla_mall = include_str!("../../blueprints/vanilla_mall.txt");
        let blueprint = parse_blueprint(vanilla_mall.trim()).expect("Failed to parse blueprint");
        
        let bp = blueprint.blueprint.unwrap();
        let entities = bp.entities.unwrap();
        
        // Find an entity with request_filters to test the structure
        let entity_with_request_filters = entities.iter()
            .find(|e| e.request_filters.is_some())
            .expect("Should find an entity with request_filters");
        
        let request_filters = entity_with_request_filters.request_filters.as_ref().unwrap();
        assert!(!request_filters.sections.is_empty(), "Should have sections");
        
        let first_section = &request_filters.sections[0];
        assert!(first_section.index > 0, "Section should have a valid index");
        
        if let Some(filters) = &first_section.filters {
            if !filters.is_empty() {
                let first_filter = &filters[0];
                assert!(!first_filter.name.is_empty(), "Filter should have a name");
            }
        }
        
        // Find an entity with a filter to test the structure
        if let Some(entity_with_filter) = entities.iter().find(|e| e.filter.is_some()) {
            let filter = entity_with_filter.filter.as_ref().unwrap();
            assert!(!filter.name.is_empty(), "Filter should have a name");
        }
    }

    #[test]
    fn test_save_blueprint_json() {
        use std::fs;
        use std::path::PathBuf;
        
        let vanilla_mall = include_str!("../../blueprints/vanilla_mall.txt");
        
        // Test the save_blueprint_json function
        let temp_file = PathBuf::from("test_output.json");
        
        // Get the parsed blueprint
        let blueprint = parse_blueprint(vanilla_mall.trim())
            .expect("Failed to parse blueprint for saving");
        // Save the JSON
        let result = save_blueprint_json(&blueprint, &temp_file);
        assert!(result.is_ok(), "Failed to save blueprint JSON: {:?}", result.err());
        
        // Verify the file was created and contains valid JSON
        assert!(temp_file.exists(), "Output file should exist");
        
        let saved_content = fs::read_to_string(&temp_file).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(&saved_content).unwrap();
        assert!(json_value.is_object(), "Saved content should be valid JSON");
        
        // Clean up
        let _ = fs::remove_file(&temp_file);
    }
}
