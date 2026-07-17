//! Core traits that server implementations must provide.
//!
//! Servers implement `FlintAdapter` to create test worlds, and `FlintWorld`/`FlintPlayer`
//! to provide the actual block and player operations.

use crate::Block;
use crate::test_spec::{EntityNbt, GameMode, Item, PlayerSlot};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;

/// Position in world coordinates [x, y, z]
pub type BlockPos = [i32; 3];

/// Server metadata
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub minecraft_version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub entity_type: Option<String>,
    pub pos: Option<[f64; 3]>,
    pub rot: Option<[f32; 2]>,
    pub nbt: HashMap<String, String>,
}

// =============================================================================
// Core Traits
// =============================================================================

/// Main adapter trait - server implements this to create test worlds
pub trait FlintAdapter: Send + Sync {
    /// Create a new disposable in-memory test world
    fn create_test_world(&self) -> Result<Box<dyn FlintWorld>>;

    /// Server metadata for logging
    fn server_info(&self) -> ServerInfo;
}

/// World operations - server implements this
///
/// This is the minimal interface servers must provide.
/// Flint handles fill/clear by iterating `set_block()`.
pub trait FlintWorld: Send + Sync {
    /// Execute exactly one game tick
    fn do_tick(&mut self) -> Result<()>;

    /// Get current tick count
    fn current_tick(&self) -> u64;

    /// Query the world's current daytime (0..=23999).
    fn get_time(&self) -> Result<u64>;

    /// Get block at position
    fn get_block(&self, pos: BlockPos, requested_nbt: &[String]) -> Result<Block>;

    /// Set block at position (with neighbor updates)
    fn set_block(&mut self, pos: BlockPos, block: &Block) -> Result<()>;

    /// Fill a region with one block. Adapters may override this with an optimized operation.
    fn fill(&mut self, region: [[i32; 3]; 2], block: &Block) -> Result<()> {
        let min = [
            region[0][0].min(region[1][0]),
            region[0][1].min(region[1][1]),
            region[0][2].min(region[1][2]),
        ];
        let max = [
            region[0][0].max(region[1][0]),
            region[0][1].max(region[1][1]),
            region[0][2].max(region[1][2]),
        ];
        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    self.set_block([x, y, z], block)?;
                }
            }
        }
        Ok(())
    }

    /// Summon an entity with a stable test-local alias.
    fn summon_entity(
        &mut self,
        _alias: &str,
        _entity_type: &str,
        _pos: [f64; 3],
        _nbt: Option<&EntityNbt>,
    ) -> Result<()> {
        Ok(())
    }

    /// Teleport an implementation-managed entity alias.
    fn teleport_entity(
        &mut self,
        _alias: &str,
        _pos: [f64; 3],
        _rot: Option<[f32; 2]>,
    ) -> Result<()> {
        Ok(())
    }

    /// Read the current entity state for a test-local alias.
    fn get_entity(&self, _alias: &str, _requested_nbt: &[String]) -> Result<Vec<EntityState>> {
        Ok(Vec::new())
    }

    /// Find an entity created naturally by gameplay using its entity type.
    fn find_entity(
        &self,
        _entity_type: &str,
        _requested_nbt: &[String],
    ) -> Result<Vec<EntityState>> {
        Ok(Vec::new())
    }

    /// Create a simulated player in this world
    ///
    /// Only called when tests use player-related actions.
    /// Pure block tests (place, fill, assert) don't need a player.
    fn create_player(&mut self) -> Box<dyn FlintPlayer>;
}

/// Player operations - server implements this
///
/// Hybrid model: Server owns the player entity, but flint can:
/// - Manipulate inventory slots directly
/// - Select hotbar slots
/// - Trigger item use actions
pub trait FlintPlayer: Send + Sync {
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Set item in a slot (None = empty/clear the slot)
    fn set_slot(&mut self, slot: PlayerSlot, item: Option<&Item>) -> Result<()>;

    /// Get item from a slot (None if empty)
    fn get_slot(&mut self, slot: PlayerSlot, requested_data: Vec<String>) -> Result<Option<Item>>;

    /// Select which hotbar slot is active (1-9)
    fn select_hotbar(&mut self, slot: u8) -> Result<()>;

    /// Get currently selected hotbar slot (1-9)
    fn selected_hotbar(&self) -> u8;

    /// Teleport the player to a world position and optionally set [yaw, pitch].
    fn teleport(&mut self, pos: [f64; 3], rot: Option<[f32; 2]>) -> Result<()>;

    /// Use the item in the active hand against the current crosshair target.
    fn interact(&mut self) -> Result<()>;

    /// Set the game mode of the player (creative, survival, etc.)
    fn set_game_mode(&mut self, mode: GameMode) -> Result<()>;
}
