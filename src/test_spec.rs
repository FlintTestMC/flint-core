use rustc_hash::FxHashMap;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt::Formatter;
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
    pub inventory: HashMap<PlayerSlot, Item>,
    /// Initially selected hotbar slot (1-9), defaults to 1
    #[serde(default = "default_selected_hotbar")]
    pub selected_hotbar: u8,
}

fn default_selected_hotbar() -> u8 {
    1
}

/// An item that can be held or placed in a slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Item identifier, e.g., "minecraft:honeycomb"
    pub id: String,
    /// Stack count (default 1)
    #[serde(default = "default_count")]
    pub count: u8,
}

impl Item {
    /// Create a new item with count 1.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        if id.starts_with("empty") {
            return Item::empty();
        }
        Self { id, count: 1 }
    }

    /// Create an empty item (air with count 0).
    pub fn empty() -> Self {
        Self {
            id: "minecraft:air".to_string(),
            count: 0,
        }
    }

    /// Create an item with a specific count.
    pub fn with_count(id: impl Into<String>, count: u8) -> Self {
        Self {
            id: id.into(),
            count,
        }
    }
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

/// Block specification with ID and properties.
///
/// Deserializes from JSON with backwards compatibility:
/// - `"powered": false` → `"powered": "false"`
/// - `"delay": 2` → `"delay": "2"`
/// - `"facing": "north"` → `"facing": "north"`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Block {
    /// Block identifier, e.g., "minecraft:stone"
    pub id: String,
    /// Block state properties, e.g., {"powered": "true", "facing": "north"}
    #[serde(flatten, skip_serializing_if = "FxHashMap::is_empty")]
    pub properties: FxHashMap<String, String>,
}

impl Block {
    /// Create a new block with no properties.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            properties: FxHashMap::default(),
        }
    }

    /// Create a block with the given properties.
    pub fn with_properties(id: impl Into<String>, properties: FxHashMap<String, String>) -> Self {
        Self {
            id: id.into(),
            properties,
        }
    }

    /// Check if this block is air.
    pub fn is_air(&self) -> bool {
        self.id == "minecraft:air" || self.id == "air"
    }

    /// Generate a Minecraft command string like `minecraft:lever[powered=false,face=floor]`.
    pub fn to_command(&self) -> String {
        if self.properties.is_empty() {
            self.id.clone()
        } else {
            let props: Vec<String> = self
                .properties
                .iter()
                .map(|(key, value)| format!("{}={}", key, value))
                .collect();
            format!("{}[{}]", self.id, props.join(","))
        }
    }
}

impl<'de> Deserialize<'de> for Block {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BlockVisitor;

        impl<'de> Visitor<'de> for BlockVisitor {
            type Value = Block;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a block object with 'id' field and optional properties")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Block, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut id: Option<String> = None;
                let mut properties = FxHashMap::default();

                while let Some(key) = map.next_key::<String>()? {
                    if key == "id" {
                        id = Some(map.next_value()?);
                    } else if key == "properties" {
                        // Handle nested properties object
                        let nested: FxHashMap<String, serde_json::Value> = map.next_value()?;
                        for (k, v) in nested {
                            let value_str = json_value_to_string(&v);
                            properties.insert(k, value_str);
                        }
                    } else {
                        // Handle flat properties - convert JSON values to strings
                        let value: serde_json::Value = map.next_value()?;
                        let value_str = json_value_to_string(&value);
                        properties.insert(key, value_str);
                    }
                }

                let id = id.ok_or_else(|| serde::de::Error::missing_field("id"))?;
                Ok(Block { id, properties })
            }
        }

        deserializer.deserialize_map(BlockVisitor)
    }
}

/// Convert a JSON value to a string representation for block properties.
fn json_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        _ => value.to_string(),
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

    #[test]
    fn redstone_lever_with_two_properties_command_string() {
        let mut block = Block::new("minecraft:lever");
        block
            .properties
            .insert("powered".to_string(), "false".to_string());
        block
            .properties
            .insert("face".to_string(), "floor".to_string());
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
        let block = Block::new("minecraft:stone");
        let result = block.to_command();
        assert_eq!(result, "minecraft:stone");
    }

    #[test]
    fn empty_id_command_string() {
        let block = Block::new("");
        let result = block.to_command();
        assert_eq!(result, "");
    }

    #[test]
    fn test_redstone_wire() {
        let mut block = Block::new("minecraft:redstone_wire");
        block
            .properties
            .insert("north".to_string(), "side".to_string());
        block
            .properties
            .insert("east".to_string(), "up".to_string());
        block
            .properties
            .insert("south".to_string(), "none".to_string());
        block
            .properties
            .insert("west".to_string(), "side".to_string());

        let result = block.to_command();
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
        // Values are converted to strings
        assert_eq!(block.properties.get("powered"), Some(&"false".to_string()));
        assert_eq!(block.properties.get("face"), Some(&"floor".to_string()));
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
        // Null is converted to empty string
        assert_eq!(block.properties.get("powered"), Some(&String::new()));
        assert_eq!(block.properties.get("face"), Some(&"floor".to_string()));
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
        assert_eq!(block.properties.get("facing"), Some(&"north".to_string()));
        // Complex objects are serialized as JSON strings
        assert!(block.properties.contains_key("metadata"));
    }

    #[test]
    fn test_parse_array_property() {
        let json = r#"{
        "id": "minecraft:custom_block",
        "colors": ["red", "blue", "green"]
    }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        assert_eq!(block.id, "minecraft:custom_block");
        // Arrays are serialized as JSON strings
        assert!(block.properties.contains_key("colors"));
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
        assert_eq!(block.properties.get("powered"), Some(&"false".to_string()));
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
            Some(&"Test \"quoted\" value".to_string())
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
        // Numbers are converted to strings
        assert_eq!(block.properties.get("integer"), Some(&"42".to_string()));
        assert_eq!(block.properties.get("float"), Some(&"3.14".to_string()));
        assert_eq!(block.properties.get("negative"), Some(&"-10".to_string()));
    }

    #[test]
    fn test_nested_properties_object() {
        let json = r#"{
            "id": "minecraft:lever",
            "properties": {
                "powered": "true",
                "face": "floor"
            }
        }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        let result = block.to_command();

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
        let json = r#"{
            "id": "minecraft:stone",
            "properties": {}
        }"#;

        let block: Block = serde_json::from_str(json).unwrap();
        let result = block.to_command();

        assert_eq!(result, "minecraft:stone");
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

    #[test]
    fn test_is_air() {
        let air = Block::new("minecraft:air");
        assert!(air.is_air());

        let air_short = Block::new("air");
        assert!(air_short.is_air());

        let stone = Block::new("minecraft:stone");
        assert!(!stone.is_air());
    }
}
