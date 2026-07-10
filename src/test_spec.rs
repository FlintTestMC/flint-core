use rustc_hash::FxHashMap;
use semver::{Version, VersionReq};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// Lightweight header parsed before full deserialization.
/// Used for version gating and test indexing.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimalTestSpec {
    #[serde(default)]
    pub flint_version: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub minecraft_ids: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

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
    pub minecraft_ids: Vec<String>,
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

impl Display for PlayerSlot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
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
    /// The gametype of the player, defaults to "creative"
    #[serde(default = "default_game_type", alias = "gamemode")]
    pub game_mode: GameMode,
}

fn default_selected_hotbar() -> u8 {
    1
}

fn default_game_type() -> GameMode {
    GameMode::Creative
}

/// An item that can be held or placed in a slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Item identifier, e.g., "minecraft:honeycomb"
    pub id: String,
    /// Stack count (default 1)
    #[serde(default = "default_count")]
    pub count: u8,
    #[serde(default)]
    #[serde(flatten)]
    pub data: FxHashMap<String, String>,
}

impl Item {
    /// Create a new item with count 1.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        if id.starts_with("empty") {
            return Item::empty();
        }
        Self {
            id,
            count: 1,
            data: FxHashMap::default(),
        }
    }

    /// Create an empty item (air with count 0).
    pub fn empty() -> Self {
        Self {
            id: "minecraft:air".to_string(),
            count: 0,
            data: FxHashMap::default(),
        }
    }

    /// Create an item with a specific count.
    pub fn with_count(id: impl Into<String>, count: u8) -> Self {
        Self {
            id: id.into(),
            count,
            data: FxHashMap::default(),
        }
    }
    /// Create an item with a specific count and data.
    pub fn with_data_and_count(
        id: impl Into<String>,
        count: u8,
        data: FxHashMap<String, String>,
    ) -> Self {
        Self {
            id: id.into(),
            count,
            data,
        }
    }
    pub fn to_command(&self) -> String {
        if self.data.is_empty() {
            self.id.clone()
        } else {
            let props: Vec<String> = self
                .data
                .iter()
                .map(|(key, value)| format!("{}={}", key, value))
                .collect();
            format!("{}[{}]", self.id, props.join(","))
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityNbt(FxHashMap<String, serde_json::Value>);

impl EntityNbt {
    pub fn to_snbt(&self) -> String {
        if self.0.is_empty() {
            "{}".to_string()
        } else {
            let fields = self
                .0
                .iter()
                .map(|(key, value)| format!("{key}:{}", json_value_to_snbt(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{fields}}}")
        }
    }

    pub fn requested_paths(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    pub fn expected_values(&self) -> FxHashMap<String, String> {
        self.0
            .iter()
            .map(|(key, value)| (key.clone(), json_value_to_entity_assert_string(value)))
            .collect()
    }
}

fn json_value_to_snbt(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => {
            if is_raw_snbt_literal(s) {
                s.clone()
            } else {
                serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
            }
        }
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(values) => {
            let values = values
                .iter()
                .map(json_value_to_snbt)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{values}]")
        }
        serde_json::Value::Object(fields) => {
            let fields = fields
                .iter()
                .map(|(key, value)| format!("{key}:{}", json_value_to_snbt(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{fields}}}")
        }
    }
}

fn json_value_to_entity_assert_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => json_value_to_snbt(value),
    }
}

fn is_raw_snbt_literal(value: &str) -> bool {
    let value = value.trim();
    value.starts_with('{')
        || value.starts_with('[')
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("false")
        || value
            .strip_suffix(|c: char| {
                matches!(c, 'b' | 'B' | 's' | 'S' | 'l' | 'L' | 'f' | 'F' | 'd' | 'D')
            })
            .is_some_and(|number| !number.is_empty() && number.parse::<f64>().is_ok())
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

#[derive(Debug, Clone, Serialize)]
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

    Summon {
        entity_alias: String,
        entity_type: String,
        pos: [f64; 3],
        #[serde(default)]
        nbt: Option<EntityNbt>,
    },

    // Assertion actions
    Assert {
        checks: Vec<AssertType>,
    },

    // Entity/player actions
    /// Teleport an entity alias. Use "player" for the backing bot/player.
    Tp {
        entity_alias: String,
        pos: [f64; 3],
        /// Rotation as [yaw, pitch].
        #[serde(default)]
        rot: Option<[f32; 2]>,
    },

    /// Interact using the item in the player's active hand.
    Interact {
        /// Item to use. If not specified, uses the player's active item.
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

impl<'de> Deserialize<'de> for ActionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut fields = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let action = take_required::<String, D::Error>(&mut fields, "do")?;
        match action.as_str() {
            "place" => Ok(ActionType::Place {
                pos: take_required(&mut fields, "pos")?,
                block: take_required(&mut fields, "block")?,
            }),
            "place_each" => Ok(ActionType::PlaceEach {
                blocks: take_required(&mut fields, "blocks")?,
            }),
            "fill" => Ok(ActionType::Fill {
                region: take_required(&mut fields, "region")?,
                with: take_required(&mut fields, "with")?,
            }),
            "remove" => Ok(ActionType::Remove {
                pos: take_required(&mut fields, "pos")?,
            }),
            "summon" => {
                let entity_alias = take_required(&mut fields, "entity_alias")?;
                let entity_type = take_required(&mut fields, "entity_type")?;
                let pos = take_required(&mut fields, "pos")?;
                let explicit_nbt = take_optional(&mut fields, "nbt")?;
                let nbt = merge_entity_nbt(explicit_nbt, entity_nbt_from_fields(fields));
                Ok(ActionType::Summon {
                    entity_alias,
                    entity_type,
                    pos,
                    nbt,
                })
            }
            "assert" => Ok(ActionType::Assert {
                checks: take_required(&mut fields, "checks")?,
            }),
            "tp" => Ok(ActionType::Tp {
                entity_alias: take_required(&mut fields, "entity_alias")?,
                pos: take_required(&mut fields, "pos")?,
                rot: take_optional(&mut fields, "rot")?,
            }),
            "interact" => Ok(ActionType::Interact {
                item: take_optional(&mut fields, "item")?,
            }),
            "set_slot" => Ok(ActionType::SetSlot {
                slot: take_required(&mut fields, "slot")?,
                item: take_optional(&mut fields, "item")?,
                count: take_optional(&mut fields, "count")?.unwrap_or_else(default_count),
            }),
            "select_hotbar" => Ok(ActionType::SelectHotbar {
                slot: take_required(&mut fields, "slot")?,
            }),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &[
                    "place",
                    "place_each",
                    "fill",
                    "remove",
                    "summon",
                    "assert",
                    "tp",
                    "interact",
                    "set_slot",
                    "select_hotbar",
                ],
            )),
        }
    }
}

fn take_required<T, E>(
    fields: &mut serde_json::Map<String, serde_json::Value>,
    key: &'static str,
) -> Result<T, E>
where
    T: serde::de::DeserializeOwned,
    E: serde::de::Error,
{
    take_optional(fields, key)?.ok_or_else(|| E::missing_field(key))
}

fn take_optional<T, E>(
    fields: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<T>, E>
where
    T: serde::de::DeserializeOwned,
    E: serde::de::Error,
{
    fields
        .remove(key)
        .map(serde_json::from_value)
        .transpose()
        .map_err(E::custom)
}

fn entity_nbt_from_fields(
    fields: serde_json::Map<String, serde_json::Value>,
) -> Option<FxHashMap<String, serde_json::Value>> {
    if fields.is_empty() {
        None
    } else {
        Some(fields.into_iter().collect())
    }
}

fn merge_entity_nbt(
    explicit: Option<EntityNbt>,
    flattened: Option<FxHashMap<String, serde_json::Value>>,
) -> Option<EntityNbt> {
    match (explicit, flattened) {
        (None, None) => None,
        (Some(nbt), None) => Some(nbt),
        (None, Some(fields)) => Some(EntityNbt(fields)),
        (Some(EntityNbt(mut explicit)), Some(flattened)) => {
            explicit.extend(flattened);
            Some(EntityNbt(explicit))
        }
    }
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
#[serde(untagged)]
pub enum BlockSpec {
    Single(Block),
    Multiple(Vec<Block>),
}

impl BlockSpec {
    pub fn to_vec(&self) -> Vec<Block> {
        match self {
            BlockSpec::Single(b) => vec![b.clone()],
            BlockSpec::Multiple(v) => v.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCheck {
    pub pos: [i32; 3],
    pub is: BlockSpec,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntityCheck {
    pub entity_alias: String,
    #[serde(default, rename = "is")]
    pub entity_type: Option<String>,
    #[serde(default = "default_exists")]
    pub exists: bool,
    #[serde(default)]
    pub pos: Option<[f64; 3]>,
    #[serde(default)]
    pub position_tolerance: Option<f64>,
    #[serde(default)]
    pub rot: Option<[f32; 2]>,
    #[serde(default)]
    pub rotation_tolerance: Option<f32>,
    #[serde(default)]
    pub nbt: Option<EntityNbt>,
}

impl<'de> Deserialize<'de> for EntityCheck {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut fields = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let entity_alias = take_required(&mut fields, "entity_alias")?;
        let entity_type = take_optional(&mut fields, "is")?;
        let exists = take_optional(&mut fields, "exists")?.unwrap_or_else(default_exists);
        let pos = take_optional(&mut fields, "pos")?;
        let position_tolerance = take_optional(&mut fields, "position_tolerance")?;
        let rot = take_optional(&mut fields, "rot")?;
        let rotation_tolerance = take_optional(&mut fields, "rotation_tolerance")?;
        let explicit_nbt = take_optional(&mut fields, "nbt")?;
        let nbt = merge_entity_nbt(explicit_nbt, entity_nbt_from_fields(fields));

        Ok(EntityCheck {
            entity_alias,
            entity_type,
            exists,
            pos,
            position_tolerance,
            rot,
            rotation_tolerance,
            nbt,
        })
    }
}

fn default_exists() -> bool {
    true
}

/// Result of a two-phase test spec load
pub enum TestSpecLoadResult {
    Loaded(TestSpec),
    Skipped {
        spec: MinimalTestSpec,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryCheck {
    pub slot: PlayerSlot,
    #[serde(default, deserialize_with = "deserialize_item_or_none")]
    pub is: Option<Item>,
}

fn deserialize_item_or_none<'de, D>(deserializer: D) -> Result<Option<Item>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(serde_json::Value::String(s)) if s == "None" || s == "empty" => Ok(None),
        Some(v) => Item::deserialize(v)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssertType {
    Block(BlockCheck),
    Inventory(InventoryCheck),
    Entity(EntityCheck),
}

impl TestSpec {
    // Maximum allowed test dimensions
    pub const MAX_WIDTH: i32 = 15;
    pub const MAX_HEIGHT: i32 = 384;
    pub const MAX_DEPTH: i32 = 15;

    pub fn from_file(path: &PathBuf, validate_cleanup: bool) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let spec: TestSpec = serde_json::from_str(&content).map_err(|e| {
            anyhow::anyhow!("{}:{}:{}: {}", path.display(), e.line(), e.column(), e)
        })?;
        spec.validate(validate_cleanup)?;
        Ok(spec)
    }

    /// Two-phase load: checks `flintVersion` before full deserialization.
    ///
    /// Pass `impl_version = None` to skip version checking (treat as "supports all").
    pub fn try_load(
        json: &str,
        req: VersionReq,
        validate_cleanup: bool,
    ) -> anyhow::Result<TestSpecLoadResult> {
        use serde::Deserialize;
        let value: serde_json::Value = serde_json::from_str(json)?;
        let minimal = MinimalTestSpec::deserialize(&value)?;
        let ver = Version::parse(&minimal.flint_version).unwrap_or(Version::new(0, 0, 0));
        if !req.matches(&ver) {
            return Ok(TestSpecLoadResult::Skipped {
                reason: format!(
                    "requires flint_version {}, implementation supports {}",
                    ver, ver
                ),
                spec: minimal,
            });
        }
        let spec = TestSpec::deserialize(value)?;
        spec.validate(validate_cleanup)?;
        Ok(TestSpecLoadResult::Loaded(spec))
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

    pub fn validate(&self, cleanup: bool) -> anyhow::Result<()> {
        // Ensure setup with cleanup is present
        let setup = self.setup.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Test '{}' missing required 'setup' section", self.name)
        })?;
        if setup.cleanup.is_none() {
            anyhow::bail!("Test '{}' missing 'cleanup' section", self.name);
        }
        let region = setup.cleanup.as_ref().unwrap().region;
        if cleanup {
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
                ActionType::Summon { pos, .. } => {
                    self.validate_position(
                        [
                            pos[0].floor() as i32,
                            pos[1].floor() as i32,
                            pos[2].floor() as i32,
                        ],
                        &region,
                    )?;
                }
                ActionType::Assert { checks } => {
                    for check in checks {
                        match check {
                            AssertType::Block(block) => {
                                self.validate_position(block.pos, &region)?
                            }
                            AssertType::Entity(entity) => {
                                if let Some(pos) = entity.pos {
                                    self.validate_position(
                                        [
                                            pos[0].floor() as i32,
                                            pos[1].floor() as i32,
                                            pos[2].floor() as i32,
                                        ],
                                        &region,
                                    )?;
                                }
                            }
                            // Inventory checks are not validated because there are not any boundings
                            AssertType::Inventory(_) => {}
                        }
                    }
                }
                // Player actions do not address a block in the cleanup region.
                ActionType::Tp { .. }
                | ActionType::Interact { .. }
                | ActionType::SetSlot { .. }
                | ActionType::SelectHotbar { .. } => {}
            }
        }

        crate::timeline_validation::validate_timeline_order(self)?;

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

    #[test]
    fn test_tp_action_deserializes_with_optional_rotation() {
        let action: ActionType = serde_json::from_str(
            r#"{"do":"tp","entity_alias":"player","pos":[1.5,64,2],"rot":[0,90]}"#,
        )
        .unwrap();

        match action {
            ActionType::Tp {
                entity_alias,
                pos,
                rot,
            } => {
                assert_eq!(entity_alias, "player");
                assert_eq!(pos, [1.5, 64.0, 2.0]);
                assert_eq!(rot, Some([0.0, 90.0]));
            }
            _ => panic!("expected tp action"),
        }
    }

    #[test]
    fn test_tp_action_requires_entity_alias() {
        let error =
            serde_json::from_str::<ActionType>(r#"{"do":"tp","pos":[1.5,64,2],"rot":[0,90]}"#)
                .unwrap_err();
        assert!(error.to_string().contains("entity_alias"));
    }

    #[test]
    fn test_tp_action_accepts_entity_alias() {
        let action: ActionType =
            serde_json::from_str(r#"{"do":"tp","entity_alias":"falling","pos":[1.5,64,2]}"#)
                .unwrap();

        match action {
            ActionType::Tp {
                entity_alias, pos, ..
            } => {
                assert_eq!(entity_alias, "falling");
                assert_eq!(pos, [1.5, 64.0, 2.0]);
            }
            _ => panic!("expected tp action"),
        }
    }

    #[test]
    fn test_interact_action_deserializes() {
        let action: ActionType =
            serde_json::from_str(r#"{"do":"interact","item":"minecraft:bone_meal"}"#).unwrap();

        match action {
            ActionType::Interact { item } => {
                assert_eq!(item.as_deref(), Some("minecraft:bone_meal"));
            }
            _ => panic!("expected interact action"),
        }
    }

    #[test]
    fn test_summon_action_deserializes() {
        let action: ActionType = serde_json::from_str(
            r#"{"do":"summon","entity_alias":"falling","entity_type":"minecraft:falling_block","pos":[1.5,64,2],"NoGravity":"1b"}"#,
        )
        .unwrap();

        match action {
            ActionType::Summon {
                entity_alias,
                entity_type,
                pos,
                nbt,
            } => {
                assert_eq!(entity_alias, "falling");
                assert_eq!(entity_type, "minecraft:falling_block");
                assert_eq!(pos, [1.5, 64.0, 2.0]);
                let nbt = nbt.expect("expected nbt");
                assert_eq!(nbt.to_snbt(), "{NoGravity:1b}");
            }
            _ => panic!("expected summon action"),
        }
    }

    #[test]
    fn test_summon_action_rejects_raw_nbt() {
        let error = serde_json::from_str::<ActionType>(
            r#"{"do":"summon","entity_alias":"falling","entity_type":"minecraft:falling_block","pos":[1.5,64,2],"nbt":"{NoGravity:1b}"}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("invalid type"));
    }

    #[test]
    fn test_summon_action_accepts_nested_nbt_field() {
        let action: ActionType = serde_json::from_str(
            r#"{"do":"summon","entity_alias":"falling","entity_type":"minecraft:falling_block","pos":[1.5,64,2],"nbt":{"NoGravity":"1b"}}"#,
        )
        .unwrap();

        match action {
            ActionType::Summon { nbt, .. } => {
                let nbt = nbt.expect("expected nbt");
                assert_eq!(nbt.to_snbt(), "{NoGravity:1b}");
            }
            _ => panic!("expected summon action"),
        }
    }

    #[test]
    fn test_entity_assert_deserializes() {
        let check: AssertType = serde_json::from_str(
            r#"{"entity_alias":"falling","is":"minecraft:falling_block","pos":[1.5,64,2],"position_tolerance":0.5,"rot":[90,0],"rotation_tolerance":1,"NoGravity":"1b"}"#,
        )
        .unwrap();

        match check {
            AssertType::Entity(entity) => {
                assert_eq!(entity.entity_alias, "falling");
                assert_eq!(
                    entity.entity_type.as_deref(),
                    Some("minecraft:falling_block")
                );
                assert!(entity.exists);
                assert_eq!(entity.pos, Some([1.5, 64.0, 2.0]));
                assert_eq!(entity.position_tolerance, Some(0.5));
                assert_eq!(entity.rot, Some([90.0, 0.0]));
                assert_eq!(entity.rotation_tolerance, Some(1.0));
                assert_eq!(
                    entity.nbt.expect("expected nbt").to_snbt(),
                    "{NoGravity:1b}"
                );
            }
            _ => panic!("expected entity assert"),
        }
    }

    #[test]
    fn test_entity_assert_rejects_raw_nbt() {
        let error = serde_json::from_str::<EntityCheck>(
            r#"{"entity_alias":"falling","nbt":"{NoGravity:1b}"}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("invalid type"));
    }
}
