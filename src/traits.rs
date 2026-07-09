//! Core traits that server implementations must provide.
//!
//! Servers implement `FlintAdapter` to create test worlds, and `FlintWorld`/`FlintPlayer`
//! to provide the actual block and player operations.

use crate::Block;
use crate::test_spec::{GameMode, Item, PlayerSlot};
use std::any::Any;

/// Position in world coordinates [x, y, z]
pub type BlockPos = [i32; 3];

/// Server metadata
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub minecraft_version: String,
}

#[derive(Debug, Clone, Default)]
pub struct EntityState {
    pub exists: bool,
    pub entity_type: Option<String>,
    pub pos: Option<[f64; 3]>,
}

// =============================================================================
// Core Traits
// =============================================================================

/// Main adapter trait - server implements this to create test worlds
pub trait FlintAdapter: Send + Sync {
    /// Create a new disposable in-memory test world
    fn create_test_world(&self) -> Box<dyn FlintWorld>;

    /// Server metadata for logging
    fn server_info(&self) -> ServerInfo;
}

/// World operations - server implements this
///
/// This is the minimal interface servers must provide.
/// Flint handles fill/clear by iterating `set_block()`.
pub trait FlintWorld: Send + Sync {
    /// Execute exactly one game tick
    fn do_tick(&mut self);

    /// Get current tick count
    fn current_tick(&self) -> u64;

    /// Get block at position
    fn get_block(&self, pos: BlockPos) -> Block;

    /// Set block at position (with neighbor updates)
    fn set_block(&mut self, pos: BlockPos, block: &Block);

    /// Summon an entity with a stable test-local alias.
    fn summon_entity(
        &mut self,
        _alias: &str,
        _entity_type: &str,
        _pos: [f64; 3],
        _nbt: Option<&str>,
    ) {
    }

    /// Teleport an implementation-managed entity alias.
    fn teleport_entity(&mut self, _alias: &str, _pos: [f64; 3], _rot: Option<[f32; 2]>) {}

    /// Read the current entity state for a test-local alias.
    fn get_entity(&self, _alias: &str) -> EntityState {
        EntityState::default()
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
    fn set_slot(&mut self, slot: PlayerSlot, item: Option<&Item>);

    /// Get item from a slot (None if empty)
    fn get_slot(&self, slot: PlayerSlot, requested_data: Vec<String>) -> Option<Item>;

    /// Select which hotbar slot is active (1-9)
    fn select_hotbar(&mut self, slot: u8);

    /// Get currently selected hotbar slot (1-9)
    fn selected_hotbar(&self) -> u8;

    /// Teleport the player to a world position and optionally set [yaw, pitch].
    fn teleport(&mut self, pos: [f64; 3], rot: Option<[f32; 2]>);

    /// Use the item in the active hand against the current crosshair target.
    fn interact(&mut self);

    /// Set the game mode of the player (creative, survival, etc.)
    fn set_game_mode(&mut self, mode: GameMode);
}
