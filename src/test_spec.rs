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
    pub cleanup: CleanupSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupSpec {
    pub region: [[i32; 3]; 2],
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
    pub id: String,
    #[serde(flatten)]
    pub properties: HashMap<String, serde_json::Value>,
}

impl Block {
    pub fn to_command(&self) -> String {
        if self.properties.is_empty() {
            // Nur ID wenn keine Properties
            self.id.clone()
        } else {
            // ID mit Properties
            let props: Vec<String> = self
                .properties
                .iter()
                .map(|(key, value)| {
                    // Value ohne Quotes formatieren
                    let val = match value {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        serde_json::Value::Number(n) => n.to_string(),
                        _ => value.to_string(),
                    };
                    format!("{}={}", key, val)
                })
                .collect();

            format!("{}[{}]", self.id, props.join(","))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum ActionType {
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
    Assert {
        checks: Vec<BlockCheck>,
    },
    AssertState {
        pos: [i32; 3],
        state: String,
        values: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPlacement {
    pub pos: [i32; 3],
    pub block: Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCheck {
    pub pos: [i32; 3],
    pub is: String,
}

impl TestSpec {
    // Maximum allowed test dimensions
    pub const MAX_WIDTH: i32 = 15;
    pub const MAX_HEIGHT: i32 = 384;
    pub const MAX_DEPTH: i32 = 15;

    pub fn from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let spec: TestSpec = serde_json::from_str(&content)?;
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
            .map(|s| s.cleanup.region)
            .expect("Cleanup region is required but not present")
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        // Ensure setup with cleanup is present
        let setup = self.setup.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Test '{}' missing required 'setup' section", self.name)
        })?;

        let region = setup.cleanup.region;
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
                ActionType::AssertState { pos, .. } => {
                    self.validate_position(*pos, &region)?;
                }
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
            properties: HashMap::new(),
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
            properties: HashMap::new(),
        };
        let result = block.to_command();
        assert_eq!(result, "minecraft:stone");
    }
    #[test]
    fn empty_id_command_string() {
        let block = Block {
            id: "".to_string(),
            properties: HashMap::new(),
        };
        let result = block.to_command();
        assert_eq!(result, "");
    }

    #[test]
    fn test_redstone_wire() {
        let mut block = Block {
            id: "minecraft:redstone_wire".to_string(),
            properties: HashMap::new(),
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
}
