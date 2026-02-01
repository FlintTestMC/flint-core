use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSpec {
    #[serde(default)]
    pub flint_version: Option<String>,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub setup: Option<SetupSpec>,
    pub timeline: Vec<TimelineEntry>,
    #[serde(default)]
    pub breakpoints: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupSpec {
    #[serde(default)]
    pub cleanup: Option<CleanupSpec>,
    #[serde(default)]
    pub player: Option<PlayerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupSpec {
    pub region: [[i32; 3]; 2],
}
/// Player inventory slots
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerSlot {
    // Hotbar (9 slots)
    Hotbar1,
    Hotbar2,
    Hotbar3,
    Hotbar4,
    Hotbar5,
    Hotbar6,
    Hotbar7,
    Hotbar8,
    Hotbar9,

    // Off-hand
    OffHand,

    // Armor
    Helmet,
    Chestplate,
    Leggings,
    Boots,
}

impl PlayerSlot {
    /// Convert hotbar number (1-9) to PlayerSlot
    pub fn hotbar(n: u8) -> Option<Self> {
        match n {
            1 => Some(Self::Hotbar1),
            2 => Some(Self::Hotbar2),
            3 => Some(Self::Hotbar3),
            4 => Some(Self::Hotbar4),
            5 => Some(Self::Hotbar5),
            6 => Some(Self::Hotbar6),
            7 => Some(Self::Hotbar7),
            8 => Some(Self::Hotbar8),
            9 => Some(Self::Hotbar9),
            _ => None,
        }
    }
}

/// Player configuration for advanced mode (initial inventory setup)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlayerConfig {
    /// Initial inventory state (slot name -> item config)
    #[serde(default)]
    pub inventory: HashMap<PlayerSlot, SlotConfig>,
    /// Initially selected hotbar slot (1-9), defaults to 1
    #[serde(default = "default_selected_hotbar")]
    pub selected_hotbar: u8,
}

fn default_selected_hotbar() -> u8 {
    1
}

/// Configuration for a single inventory slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotConfig {
    /// Item identifier, e.g., "minecraft:honeycomb"
    pub item: String,
    /// Stack count, defaults to 1
    #[serde(default = "default_count")]
    pub count: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    #[serde(rename = "at")]
    pub at: TickSpec,
    #[serde(flatten)]
    pub action_type: ActionType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TickSpec {
    Single(u32),
    Multiple(Vec<u32>),
}

impl TickSpec {
    pub fn to_vec(&self) -> Vec<u32> {
        match self {
            TickSpec::Single(t) => vec![*t],
            TickSpec::Multiple(v) => v.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Block identifier, e.g., "minecraft:stone"
    pub id: String,
    /// Block state properties, e.g., {"powered": "true", "facing": "north"}
    #[serde(flatten)]
    pub properties: FxHashMap<String, serde_json::Value>,
}

impl Block {
    pub fn to_command(&self) -> String {
        if self.properties.is_empty() {
            self.id.clone()
        } else {
            let mut props: Vec<String> = Vec::new();

            for (key, value) in &self.properties {
                if key == "properties" {
                    if let serde_json::Value::Object(nested) = value {
                        for (nested_key, nested_value) in nested {
                            let val = match nested_value {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Bool(b) => b.to_string(),
                                serde_json::Value::Number(n) => n.to_string(),
                                _ => nested_value.to_string(),
                            };
                            props.push(format!("{}={}", nested_key, val));
                        }
                    }
                } else {
                    let val = match value {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        serde_json::Value::Number(n) => n.to_string(),
                        _ => value.to_string(),
                    };
                    props.push(format!("{}={}", key, val));
                }
            }

            if props.is_empty() {
                self.id.clone()
            } else {
                format!("{}[{}]", self.id, props.join(","))
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockFace {
    Top,    // +Y
    Bottom, // -Y
    North,  // -Z
    South,  // +Z
    East,   // +X
    West,   // -X
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum ActionType {
    // Block actions
    Place {
        pos: [i32; 3],
        block: Block,
    },
    PlaceEach {
        blocks: Vec<BlockPlacement>,
    },
    Fill {
        region: [[i32; 3]; 2],
        with: Block,
    },
    Remove {
        pos: [i32; 3],
    },

    // Assertion actions
    Assert {
        checks: Vec<BlockCheck>,
    },

    // Player actions (for item interactions)
    /// Use an item on a block face (e.g., honeycomb on copper, axe on log)
    UseItemOn {
        pos: [i32; 3],
        face: BlockFace,
        /// Item to use (for simple mode). If not specified, uses player's active item.
        #[serde(default)]
        item: Option<String>,
    },

    /// Set an item in a player slot
    SetSlot {
        slot: PlayerSlot,
        #[serde(default)]
        item: Option<String>,
        #[serde(default = "default_count")]
        count: u8,
    },

    /// Select which hotbar slot is active (1-9)
    SelectHotbar {
        slot: u8,
    },
}

fn default_count() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPlacement {
    pub pos: [i32; 3],
    pub block: Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCheck {
    pub pos: [i32; 3],
    pub is: Block,
}

impl TestSpec {
    // Maximum allowed test dimensions
    pub const MAX_WIDTH: i32 = 15;
    pub const MAX_HEIGHT: i32 = 384;
    pub const MAX_DEPTH: i32 = 15;

    pub fn from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let spec: TestSpec = serde_json::from_str(&content).map_err(|e| {
            anyhow::anyhow!("{}:{}:{}: {}", path.display(), e.line(), e.column(), e)
        })?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn max_tick(&self) -> u32 {
        self.timeline
            .iter()
            .flat_map(|entry| entry.at.to_vec())
            .max()
            .unwrap_or(0)
    }

    pub fn cleanup_region(&self) -> [[i32; 3]; 2] {
        self.setup
            .as_ref()
            .ok_or_else(|| panic!("setup is missing"))
            .unwrap()
            .cleanup
            .as_ref()
            .map(|s| s.region)
            .expect("Cleanup region is required but not present")
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        // Ensure setup with cleanup is present
        let setup = self.setup.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Test '{}' missing required 'setup' section", self.name)
        })?;
        if setup.cleanup.is_none() {
            anyhow::bail!("Test '{}' missing 'cleanup' section", self.name);
        }
        let region = setup.cleanup.as_ref().unwrap().region;
        let min = region[0];
        let max = region[1];

        // Calculate dimensions
        let width = max[0] - min[0] + 1;
        let height = max[1] - min[1] + 1;
        let depth = max[2] - min[2] + 1;

        // Validate region forms valid bounds
        if min[0] > max[0] || min[1] > max[1] || min[2] > max[2] {
            anyhow::bail!(
                "Test '{}': Invalid cleanup region - min coordinates must be <= max coordinates. Got min=[{},{},{}], max=[{},{},{}]",
                self.name,
                min[0],
                min[1],
                min[2],
                max[0],
                max[1],
                max[2]
            );
        }

        // Validate dimensions don't exceed max size
        if width > Self::MAX_WIDTH {
            anyhow::bail!(
                "Test '{}': Cleanup region width {} exceeds maximum {}",
                self.name,
                width,
                Self::MAX_WIDTH
            );
        }
        if height > Self::MAX_HEIGHT {
            anyhow::bail!(
                "Test '{}': Cleanup region height {} exceeds maximum {}",
                self.name,
                height,
                Self::MAX_HEIGHT
            );
        }
        if depth > Self::MAX_DEPTH {
            anyhow::bail!(
                "Test '{}': Cleanup region depth {} exceeds maximum {}",
                self.name,
                depth,
                Self::MAX_DEPTH
            );
        }

        // Validate all test coordinates are within cleanup region
        for entry in &self.timeline {
            match &entry.action_type {
                ActionType::Place { pos, .. } => {
                    self.validate_position(*pos, &region)?;
                }
                ActionType::PlaceEach { blocks } => {
                    for block in blocks {
                        self.validate_position(block.pos, &region)?;
                    }
                }
                ActionType::Fill {
                    region: fill_region,
                    ..
                } => {
                    self.validate_position(fill_region[0], &region)?;
                    self.validate_position(fill_region[1], &region)?;
                }
                ActionType::Remove { pos } => {
                    self.validate_position(*pos, &region)?;
                }
                ActionType::Assert { checks } => {
                    for check in checks {
                        self.validate_position(check.pos, &region)?;
                    }
                }
                ActionType::UseItemOn { pos, .. } => {
                    self.validate_position(*pos, &region)?;
                }
                // SetSlot and SelectHotbar don't have positions to validate
                ActionType::SetSlot { .. } | ActionType::SelectHotbar { .. } => {}
            }
        }

        Ok(())
    }

    fn validate_position(&self, pos: [i32; 3], region: &[[i32; 3]; 2]) -> anyhow::Result<()> {
        let min = region[0];
        let max = region[1];

        if pos[0] < min[0]
            || pos[0] > max[0]
            || pos[1] < min[1]
            || pos[1] > max[1]
            || pos[2] < min[2]
            || pos[2] > max[2]
        {
            anyhow::bail!(
                "Test '{}': Position [{},{},{}] is outside cleanup region [{},{},{}] to [{},{},{}]",
                self.name,
                pos[0],
                pos[1],
                pos[2],
                min[0],
                min[1],
                min[2],
                max[0],
                max[1],
                max[2]
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn redstone_lever_with_two_properties_command_string() {
        let mut block = Block {
            id: "minecraft:lever".to_string(),
            properties: FxHashMap::default(),
        };
        block
            .properties
            .insert("powered".to_string(), Value::from(false));
        block
            .properties
            .insert("face".to_string(), Value::from("floor"));
        let result = block.to_command();
        assert!(
            result == "minecraft:lever[powered=false,face=floor]"
                || result == "minecraft:lever[face=floor,powered=false]",
            "Got: {}",
            result
        );
    }
    #[test]
    fn only_id_command_string() {
        let block = Block {
            id: "minecraft:stone".to_string(),
            properties: FxHashMap::default(),
        };
        let result = block.to_command();
        assert_eq!(result, "minecraft:stone");
    }
    #[test]
    fn empty_id_command_string() {
        let block = Block {
            id: "".to_string(),
            properties: FxHashMap::default(),
        };
        let result = block.to_command();
        assert_eq!(result, "");
    }

    #[test]
    fn test_redstone_wire() {
        let mut block = Block {
            id: "minecraft:redstone_wire".to_string(),
            properties: FxHashMap::default(),
        };
        block
            .properties
            .insert("north".to_string(), Value::from("side"));
        block
            .properties
            .insert("east".to_string(), Value::from("up"));
        block
            .properties
            .insert("south".to_string(), Value::from("none"));
        block
            .properties
            .insert("west".to_string(), Value::from("side"));

        let result = block.to_command();
        // Prüfe dass ID und Properties vorhanden sind
        assert!(result.starts_with("minecraft:redstone_wire["));
        assert!(result.ends_with("]"));
        assert!(result.contains("north=side"));
        assert!(result.contains("east=up"));
        assert!(result.contains("south=none"));
        assert!(result.contains("west=side"));
    }
    #[test]
    fn test_parse_lever() {
        let json = r#"{
            "id": "minecraft:lever",
            "powered": false,
            "face": "floor"
        }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        assert_eq!(block.id, "minecraft:lever");
        assert_eq!(block.properties.get("powered"), Some(&Value::Bool(false)));
        assert_eq!(
            block.properties.get("face"),
            Some(&Value::String("floor".to_string()))
        );
    }
    #[test]
    #[should_panic(expected = "missing field `id`")]
    fn test_parse_missing_id() {
        let json = r#"{
        "powered": false,
        "face": "floor"
    }"#;

        let _block: Block = serde_json::from_str(json).unwrap();
    }
    #[test]
    #[should_panic(expected = "missing field `id`")]
    fn test_parse_missing_object() {
        let json = r#"{}"#;

        let _block: Block = serde_json::from_str(json).unwrap();
    }

    #[test]
    fn test_parse_null_property() {
        let json = r#"{
        "id": "minecraft:lever",
        "powered": null,
        "face": "floor"
    }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        assert_eq!(block.id, "minecraft:lever");
        assert_eq!(block.properties.get("powered"), Some(&Value::Null));
        assert_eq!(
            block.properties.get("face"),
            Some(&Value::String("floor".to_string()))
        );
    }

    #[test]
    fn test_parse_nested_object() {
        let json = r#"{
        "id": "minecraft:chest",
        "facing": "north",
        "metadata": {
            "items": ["diamond", "gold"]
        }
    }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        assert_eq!(block.id, "minecraft:chest");
        assert!(block.properties.get("metadata").unwrap().is_object());
    }

    #[test]
    fn test_parse_array_property() {
        let json = r#"{
        "id": "minecraft:custom_block",
        "colors": ["red", "blue", "green"]
    }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        assert_eq!(block.id, "minecraft:custom_block");
        assert!(block.properties.get("colors").unwrap().is_array());
    }

    #[test]
    fn test_parse_empty_string_id() {
        let json = r#"{
        "id": "",
        "powered": false
    }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        assert_eq!(block.id, "");
        assert_eq!(block.properties.len(), 1);
    }

    #[test]
    fn test_parse_special_characters() {
        let json = r#"{
        "id": "minecraft:custom",
        "name": "Test \"quoted\" value",
        "path": "C:\\Users\\test"
    }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        assert_eq!(block.id, "minecraft:custom");
        assert_eq!(
            block.properties.get("name"),
            Some(&Value::String("Test \"quoted\" value".to_string()))
        );
    }

    #[test]
    fn test_parse_number_types() {
        let json = r#"{
        "id": "minecraft:block",
        "integer": 42,
        "float": 3.14,
        "negative": -10
    }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        assert_eq!(block.id, "minecraft:block");
        assert!(block.properties.get("integer").unwrap().is_number());
        assert!(block.properties.get("float").unwrap().is_number());
        assert!(block.properties.get("negative").unwrap().is_number());
    }

    #[test]
    fn test_nested_properties_object() {
        // Test the new format where properties are nested inside a "properties" key
        let json = r#"{
            "id": "minecraft:lever",
            "properties": {
                "powered": "true",
                "face": "floor"
            }
        }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        let result = block.to_command();

        // Order may vary due to HashMap
        assert!(result.contains("minecraft:lever["));
        assert!(result.contains("powered=true"));
        assert!(result.contains("face=floor"));
    }

    #[test]
    fn test_nested_properties_with_numbers() {
        let json = r#"{
            "id": "minecraft:redstone_wire",
            "properties": {
                "power": 15,
                "north": "side"
            }
        }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        let result = block.to_command();

        assert!(result.contains("minecraft:redstone_wire["));
        assert!(result.contains("power=15"));
        assert!(result.contains("north=side"));
    }

    #[test]
    fn test_empty_nested_properties() {
        // When properties object is empty, should return just the id
        let json = r#"{
            "id": "minecraft:stone",
            "properties": {}
        }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        let result = block.to_command();

        assert_eq!(result, "minecraft:stone");
    }

    #[test]
    fn test_mixed_flat_and_nested_properties() {
        // Test when there's both flat properties and nested ones
        let mut block = Block {
            id: "minecraft:test".to_string(),
            properties: FxHashMap::default(),
        };

        // Add a flat property
        block
            .properties
            .insert("flat_prop".to_string(), Value::from("value1"));

        // Add nested properties
        let mut nested = serde_json::Map::new();
        nested.insert("nested_prop".to_string(), Value::from("value2"));
        block
            .properties
            .insert("properties".to_string(), Value::Object(nested));

        let result = block.to_command();

        assert!(result.contains("minecraft:test["));
        assert!(result.contains("flat_prop=value1"));
        assert!(result.contains("nested_prop=value2"));
    }

    #[test]
    fn test_nested_properties_bool_values() {
        let json = r#"{
            "id": "minecraft:piston",
            "properties": {
                "extended": true,
                "facing": "up"
            }
        }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        let result = block.to_command();

        assert!(result.contains("extended=true"));
        assert!(result.contains("facing=up"));
    }
}
