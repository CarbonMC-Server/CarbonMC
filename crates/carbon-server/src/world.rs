use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, VecDeque},
};

use carbon_api::{
    BiomeKind, BlockKind, BlockPlacement, BlockPosition, ChestSnapshot, ChunkPosition,
    ChunkSnapshot, DimensionKind, EntityPosition, ItemEntitySnapshot, ItemKind, ItemStack,
    MobAiState, MobKind, MobSnapshot, PlayerSnapshot, TerrainProfile,
};
use uuid::Uuid;

const CHUNK_WIDTH: i32 = 16;
const CAVE_CELL_WIDTH: i32 = 64;
const CAVE_TUNNEL_RADIUS: i32 = 3;
const CAVE_CHAMBER_RADIUS: i32 = 5;
const CAVE_ENTRANCE_RADIUS: i32 = 2;
const CAVE_ENTRANCE_REACH: i32 = 48;
const AQUIFER_CELL_WIDTH: i32 = CAVE_CELL_WIDTH;
const AQUIFER_MAX_RADIUS: i32 = 11;
const NETHER_LAYER_CELL_WIDTH: i32 = 96;
const NETHER_LAYER_MAX_REACH: i32 = 46;
const NETHER_ROOF_Y: i32 = 127;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GeneratedStructureKind {
    House,
    TrailLookout,
    RuinedPortal,
    BasaltWaymark,
    EndObelisk,
    EndArch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GeneratedStructure {
    pub kind: GeneratedStructureKind,
    pub anchor: BlockPosition,
    pub loot_chest: BlockPosition,
}

struct GeneratedChunk {
    position: ChunkPosition,
    dimension: DimensionKind,
    biome: BiomeKind,
    terrain: TerrainProfile,
    surface_y: [i16; 256],
    nether_ceiling: Option<[i16; 256]>,
    end_bottom: Option<[i16; 256]>,
    blocks: HashMap<BlockPosition, BlockKind>,
    structures: Vec<GeneratedStructure>,
    edits: HashMap<BlockPosition, BlockKind>,
    revision: u64,
}

impl GeneratedChunk {
    #[cfg(test)]
    fn generate(seed: i64, position: ChunkPosition) -> Self {
        Self::generate_in(seed, DimensionKind::Overworld, position)
    }

    fn generate_in(seed: i64, dimension: DimensionKind, position: ChunkPosition) -> Self {
        let mut surface_y = [64; 256];
        let mut nether_ceiling = (dimension == DimensionKind::Nether).then_some([96; 256]);
        let mut end_bottom = (dimension == DimensionKind::End).then_some([320; 256]);
        for local_z in 0..CHUNK_WIDTH {
            for local_x in 0..CHUNK_WIDTH {
                let x = position.x * CHUNK_WIDTH + local_x;
                let z = position.z * CHUNK_WIDTH + local_z;
                let column = (local_z * CHUNK_WIDTH + local_x) as usize;
                if let Some(bottoms) = &mut end_bottom {
                    let (bottom, top) = end_island_column(seed, x, z).unwrap_or((320, -65));
                    bottoms[column] = bottom as i16;
                    surface_y[column] = top as i16;
                } else {
                    surface_y[column] = terrain_height(seed, dimension, x, z) as i16;
                }
                if let Some(ceiling) = &mut nether_ceiling {
                    ceiling[(local_z * CHUNK_WIDTH + local_x) as usize] =
                        nether_ceiling_height(seed, x, z) as i16;
                }
            }
        }
        let biome = biome_at(
            seed,
            dimension,
            position.x * CHUNK_WIDTH + CHUNK_WIDTH / 2,
            position.z * CHUNK_WIDTH + CHUNK_WIDTH / 2,
        );
        let mut chunk = Self {
            position,
            dimension,
            biome,
            terrain: terrain_profile(dimension, biome),
            surface_y,
            nether_ceiling,
            end_bottom,
            blocks: HashMap::new(),
            structures: Vec::new(),
            edits: HashMap::new(),
            revision: 1,
        };
        if dimension == DimensionKind::Overworld {
            chunk.generate_water();
            chunk.generate_caves(seed);
            chunk.generate_cave_network(seed);
            chunk.generate_cave_entrances(seed);
            chunk.generate_cave_surfaces(seed);
            chunk.generate_aquifers(seed);
            chunk.generate_gravel_pockets(seed);
            chunk.generate_ores(seed);
            chunk.generate_trees(seed);
        } else if dimension == DimensionKind::Nether {
            chunk.generate_nether_layers(seed);
            chunk.generate_nether_decorators(seed);
        }
        chunk.generate_structures(seed);
        if dimension == DimensionKind::Overworld {
            chunk.generate_surface_plants(seed);
        }
        chunk
    }

    fn generate_surface_plants(&mut self, seed: i64) {
        let (kind, spacing) = match self.biome.name() {
            "desert" | "badlands" | "eroded_badlands" | "wooded_badlands" => {
                (BlockKind::DeadBush, 35)
            }
            "taiga" | "old_growth_pine_taiga" | "old_growth_spruce_taiga" => (BlockKind::Fern, 10),
            "plains"
            | "sunflower_plains"
            | "forest"
            | "flower_forest"
            | "birch_forest"
            | "old_growth_birch_forest"
            | "meadow"
            | "savanna"
            | "savanna_plateau" => (BlockKind::ShortGrass, 9),
            _ => return,
        };
        for z in self.position.z * 16..self.position.z * 16 + 16 {
            for x in self.position.x * 16..self.position.x * 16 + 16 {
                // Keep the starter clearing and portal approach unobstructed.
                if x.abs() <= 16 && z.abs() <= 16 {
                    continue;
                }
                let ground = self.surface_at(x, z);
                let position = BlockPosition {
                    x,
                    y: ground + 1,
                    z,
                };
                if ground < 63
                    || self.block_at(position) != BlockKind::Air
                    || !kind.plant_survives_on(self.block_at(BlockPosition {
                        y: ground,
                        ..position
                    }))
                {
                    continue;
                }
                let patch = value_noise(seed as u64 ^ 0x706c_616e_7473, x, z, 32);
                let roll = mix(seed as u64 ^ 0x6465_636f_7261, x, z);
                if patch > -0.3 && roll % spacing == 0 {
                    self.blocks.insert(position, kind);
                }
            }
        }
    }

    fn generate_water(&mut self) {
        // Blended coasts and wetland columns can be below sea level even when
        // the chunk's biome label is land. Fill by elevation, not biome label.
        for local_z in 0..CHUNK_WIDTH {
            for local_x in 0..CHUNK_WIDTH {
                let x = self.position.x * CHUNK_WIDTH + local_x;
                let z = self.position.z * CHUNK_WIDTH + local_z;
                let surface = self.surface_at(x, z);
                for y in surface + 1..=63 {
                    self.insert_if_inside(BlockPosition { x, y, z }, BlockKind::Water);
                }
            }
        }
    }

    fn generate_caves(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        for cell_z in (min_z - 7).div_euclid(32)..=(min_z + 22).div_euclid(32) {
            for cell_x in (min_x - 7).div_euclid(32)..=(min_x + 22).div_euclid(32) {
                let value = mix(seed as u64 ^ 0xca7e_5eed, cell_x, cell_z);
                if value % 3 != 0 {
                    continue;
                }
                let center_x = cell_x * 32 + 4 + i32::try_from((value >> 8) % 24).unwrap_or(0);
                let center_z = cell_z * 32 + 4 + i32::try_from((value >> 16) % 24).unwrap_or(0);
                let center_y = -20 + i32::try_from((value >> 24) % 72).unwrap_or(0);
                let radius = 3 + i32::try_from((value >> 32) % 4).unwrap_or(0);
                for z in center_z - radius..=center_z + radius {
                    for x in center_x - radius..=center_x + radius {
                        let surface = self.surface_at(x, z);
                        for y in center_y - radius..=center_y + radius {
                            let dx = x - center_x;
                            let dy = y - center_y;
                            let dz = z - center_z;
                            if dx * dx + dy * dy + dz * dz <= radius * radius
                                && y > -60
                                && y < surface - 5
                            {
                                self.insert_if_inside(BlockPosition { x, y, z }, BlockKind::Air);
                            }
                        }
                    }
                }
            }
        }
    }

    fn generate_cave_network(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        // Every cell owns its east and south edges. Include the previous cells
        // for incoming edges, and expand by the largest carving radius. All
        // geometry uses world coordinates, independent of chunk load order.
        let start_x = (min_x - CAVE_CHAMBER_RADIUS).div_euclid(CAVE_CELL_WIDTH) - 1;
        let end_x = (min_x + CHUNK_WIDTH - 1 + CAVE_CHAMBER_RADIUS).div_euclid(CAVE_CELL_WIDTH);
        let start_z = (min_z - CAVE_CHAMBER_RADIUS).div_euclid(CAVE_CELL_WIDTH) - 1;
        let end_z = (min_z + CHUNK_WIDTH - 1 + CAVE_CHAMBER_RADIUS).div_euclid(CAVE_CELL_WIDTH);
        for cell_z in start_z..=end_z {
            for cell_x in start_x..=end_x {
                let node = cave_network_node(seed, cell_x, cell_z);
                self.carve_cave_ball(node, CAVE_CHAMBER_RADIUS);
                for (next_x, next_z) in [(cell_x + 1, cell_z), (cell_x, cell_z + 1)] {
                    let next = cave_network_node(seed, next_x, next_z);
                    self.carve_cave_tunnel(node, next);
                }
            }
        }
    }

    fn generate_cave_entrances(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        let margin = CAVE_ENTRANCE_REACH + CAVE_ENTRANCE_RADIUS;
        let start_x = (min_x - margin).div_euclid(CAVE_CELL_WIDTH) - 1;
        let end_x = (min_x + CHUNK_WIDTH - 1 + margin).div_euclid(CAVE_CELL_WIDTH) + 1;
        let start_z = (min_z - margin).div_euclid(CAVE_CELL_WIDTH) - 1;
        let end_z = (min_z + CHUNK_WIDTH - 1 + margin).div_euclid(CAVE_CELL_WIDTH) + 1;
        for cell_z in start_z..=end_z {
            for cell_x in start_x..=end_x {
                let Some(entrance) = cave_entrance(seed, cell_x, cell_z) else {
                    continue;
                };
                self.carve_entrance_tunnel(entrance.mouth, entrance.turn);
                self.carve_entrance_tunnel(entrance.turn, entrance.node);
            }
        }
    }

    fn carve_entrance_tunnel(&mut self, start: BlockPosition, end: BlockPosition) {
        let delta = [end.x - start.x, end.y - start.y, end.z - start.z];
        let steps = delta
            .iter()
            .map(|value| value.abs())
            .max()
            .unwrap_or(0)
            .max(1);
        for step in 0..=steps {
            self.carve_entrance_ball(BlockPosition {
                x: start.x + (delta[0] * step).div_euclid(steps),
                y: start.y + (delta[1] * step).div_euclid(steps),
                z: start.z + (delta[2] * step).div_euclid(steps),
            });
        }
    }

    fn carve_entrance_ball(&mut self, center: BlockPosition) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        for z in (center.z - CAVE_ENTRANCE_RADIUS).max(min_z)
            ..=(center.z + CAVE_ENTRANCE_RADIUS).min(min_z + CHUNK_WIDTH - 1)
        {
            for x in (center.x - CAVE_ENTRANCE_RADIUS).max(min_x)
                ..=(center.x + CAVE_ENTRANCE_RADIUS).min(min_x + CHUNK_WIDTH - 1)
            {
                let horizontal = (x - center.x).pow(2) + (z - center.z).pow(2);
                for y in
                    (center.y - CAVE_ENTRANCE_RADIUS).max(-59)..=center.y + CAVE_ENTRANCE_RADIUS
                {
                    if horizontal + (y - center.y).pow(2)
                        <= CAVE_ENTRANCE_RADIUS * CAVE_ENTRANCE_RADIUS
                    {
                        self.blocks
                            .insert(BlockPosition { x, y, z }, BlockKind::Air);
                    }
                }
            }
        }
    }

    fn generate_cave_surfaces(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        let carved: Vec<_> = self
            .blocks
            .iter()
            .filter_map(|(position, kind)| (*kind == BlockKind::Air).then_some(*position))
            .collect();
        let mut replacements = HashMap::new();
        for air in carved {
            for position in [
                BlockPosition {
                    x: air.x - 1,
                    ..air
                },
                BlockPosition {
                    x: air.x + 1,
                    ..air
                },
                BlockPosition {
                    y: air.y - 1,
                    ..air
                },
                BlockPosition {
                    y: air.y + 1,
                    ..air
                },
                BlockPosition {
                    z: air.z - 1,
                    ..air
                },
                BlockPosition {
                    z: air.z + 1,
                    ..air
                },
            ] {
                if position.x < min_x
                    || position.x >= min_x + CHUNK_WIDTH
                    || position.z < min_z
                    || position.z >= min_z + CHUNK_WIDTH
                    || position.y < -52
                    || position.y > 48
                    || self.block_at(position) != BlockKind::Stone
                {
                    continue;
                }
                if let Some(kind) = cave_surface_material(seed, position) {
                    replacements.insert(position, kind);
                }
            }
        }
        self.blocks.extend(replacements);
    }

    fn generate_aquifers(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        let start_x = (min_x - AQUIFER_MAX_RADIUS).div_euclid(AQUIFER_CELL_WIDTH);
        let end_x = (min_x + CHUNK_WIDTH - 1 + AQUIFER_MAX_RADIUS).div_euclid(AQUIFER_CELL_WIDTH);
        let start_z = (min_z - AQUIFER_MAX_RADIUS).div_euclid(AQUIFER_CELL_WIDTH);
        let end_z = (min_z + CHUNK_WIDTH - 1 + AQUIFER_MAX_RADIUS).div_euclid(AQUIFER_CELL_WIDTH);
        for cell_z in start_z..=end_z {
            for cell_x in start_x..=end_x {
                let Some(aquifer) = cave_aquifer(seed, cell_x, cell_z) else {
                    continue;
                };
                for z in (aquifer.center.z - aquifer.radius).max(min_z)
                    ..=(aquifer.center.z + aquifer.radius).min(min_z + CHUNK_WIDTH - 1)
                {
                    for x in (aquifer.center.x - aquifer.radius).max(min_x)
                        ..=(aquifer.center.x + aquifer.radius).min(min_x + CHUNK_WIDTH - 1)
                    {
                        for y in aquifer.center.y - aquifer.radius..=aquifer.center.y {
                            let dx = x - aquifer.center.x;
                            let dy = y - aquifer.center.y;
                            let dz = z - aquifer.center.z;
                            if dx * dx + dy * dy + dz * dz <= aquifer.radius * aquifer.radius {
                                let position = BlockPosition { x, y, z };
                                if self.block_at(position) == BlockKind::Air {
                                    self.blocks.insert(position, BlockKind::Water);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn generate_nether_layers(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        let start_x = (min_x - NETHER_LAYER_MAX_REACH).div_euclid(NETHER_LAYER_CELL_WIDTH) - 1;
        let end_x = (min_x + CHUNK_WIDTH - 1 + NETHER_LAYER_MAX_REACH)
            .div_euclid(NETHER_LAYER_CELL_WIDTH)
            + 1;
        let start_z = (min_z - NETHER_LAYER_MAX_REACH).div_euclid(NETHER_LAYER_CELL_WIDTH) - 1;
        let end_z = (min_z + CHUNK_WIDTH - 1 + NETHER_LAYER_MAX_REACH)
            .div_euclid(NETHER_LAYER_CELL_WIDTH)
            + 1;
        for cell_z in start_z..=end_z {
            for cell_x in start_x..=end_x {
                let Some(layer) = nether_layer(seed, cell_x, cell_z) else {
                    continue;
                };
                self.carve_nether_ball(layer.center, layer.radius);
                self.carve_nether_tunnel(layer.center, layer.mouth);
                self.fill_nether_lava(layer);
            }
        }
    }

    fn carve_nether_tunnel(&mut self, start: BlockPosition, end: BlockPosition) {
        let delta = [end.x - start.x, end.y - start.y, end.z - start.z];
        let steps = delta
            .iter()
            .map(|value| value.abs())
            .max()
            .unwrap_or(0)
            .max(1);
        for step in 0..=steps {
            self.carve_nether_ball(
                BlockPosition {
                    x: start.x + (delta[0] * step).div_euclid(steps),
                    y: start.y + (delta[1] * step).div_euclid(steps),
                    z: start.z + (delta[2] * step).div_euclid(steps),
                },
                3,
            );
        }
    }

    fn carve_nether_ball(&mut self, center: BlockPosition, radius: i32) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        for z in (center.z - radius).max(min_z)..=(center.z + radius).min(min_z + 15) {
            for x in (center.x - radius).max(min_x)..=(center.x + radius).min(min_x + 15) {
                let horizontal = (x - center.x).pow(2) + (z - center.z).pow(2);
                for y in (center.y - radius).max(-59)..=(center.y + radius).min(125) {
                    if horizontal + (y - center.y).pow(2) <= radius * radius {
                        self.blocks
                            .insert(BlockPosition { x, y, z }, BlockKind::Air);
                    }
                }
            }
        }
    }

    fn fill_nether_lava(&mut self, layer: NetherLayer) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        for z in (layer.center.z - layer.radius).max(min_z)
            ..=(layer.center.z + layer.radius).min(min_z + 15)
        {
            for x in (layer.center.x - layer.radius).max(min_x)
                ..=(layer.center.x + layer.radius).min(min_x + 15)
            {
                for y in layer.center.y - layer.radius..=layer.center.y - 2 {
                    let dx = x - layer.center.x;
                    let dy = y - layer.center.y;
                    let dz = z - layer.center.z;
                    if dx * dx + dy * dy + dz * dz <= layer.radius * layer.radius {
                        let position = BlockPosition { x, y, z };
                        if self.block_at(position) == BlockKind::Air {
                            self.blocks.insert(position, BlockKind::Lava);
                        }
                    }
                }
            }
        }
    }

    fn generate_nether_decorators(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        for z in min_z..min_z + CHUNK_WIDTH {
            for x in min_x..min_x + CHUNK_WIDTH {
                // Preserve the starter portal and its approach.
                if x.abs() <= 20 && z.abs() <= 20 {
                    continue;
                }
                let floor = self.surface_at(x, z);
                let above = BlockPosition { x, y: floor + 1, z };
                if self.block_at(above) != BlockKind::Air {
                    continue;
                }
                let patch = value_noise(seed as u64 ^ 0x6e65_7468_6572, x, z, 24);
                let roll = mix(seed as u64 ^ 0x6465_636f_7261, x, z);
                if patch > 0.28 && roll % 5 == 0 {
                    self.blocks
                        .insert(BlockPosition { x, y: floor, z }, BlockKind::SoulSand);
                }
                if patch < -0.48 && roll % 97 == 0 {
                    let height = 2 + i32::try_from((roll >> 16) % 4).unwrap_or(0);
                    for y in floor + 1..=floor + height {
                        let position = BlockPosition { x, y, z };
                        if self.block_at(position) != BlockKind::Air {
                            break;
                        }
                        self.blocks.insert(position, BlockKind::Basalt);
                    }
                }
            }
        }
    }

    fn carve_cave_tunnel(&mut self, start: BlockPosition, end: BlockPosition) {
        let delta = [end.x - start.x, end.y - start.y, end.z - start.z];
        let steps = delta
            .iter()
            .map(|value| value.abs())
            .max()
            .unwrap_or(0)
            .max(1);
        // Adjacent samples differ by at most one block on each axis. Overlapping
        // radius-three balls leave a continuous passage, including chunk seams.
        for step in 0..=steps {
            self.carve_cave_ball(
                BlockPosition {
                    x: start.x + (delta[0] * step).div_euclid(steps),
                    y: start.y + (delta[1] * step).div_euclid(steps),
                    z: start.z + (delta[2] * step).div_euclid(steps),
                },
                CAVE_TUNNEL_RADIUS,
            );
        }
    }

    fn carve_cave_ball(&mut self, center: BlockPosition, radius: i32) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        // Clip before iterating: distant edge samples do no block work.
        for z in (center.z - radius).max(min_z)..=(center.z + radius).min(min_z + CHUNK_WIDTH - 1) {
            for x in
                (center.x - radius).max(min_x)..=(center.x + radius).min(min_x + CHUNK_WIDTH - 1)
            {
                let horizontal = (x - center.x).pow(2) + (z - center.z).pow(2);
                let roof = self.surface_at(x, z) - 6;
                for y in (center.y - radius).max(-59)..=(center.y + radius).min(roof) {
                    if horizontal + (y - center.y).pow(2) <= radius * radius {
                        self.blocks
                            .insert(BlockPosition { x, y, z }, BlockKind::Air);
                    }
                }
            }
        }
    }

    fn generate_ores(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        for vein in 0..24_i32 {
            let value = mix(
                seed as u64 ^ 0x0ae5_2602,
                self.position.x.wrapping_mul(31).wrapping_add(vein),
                self.position.z,
            );
            let x = min_x + i32::try_from((value >> 8) % 16).unwrap_or(0);
            let z = min_z + i32::try_from((value >> 16) % 16).unwrap_or(0);
            let y = -56 + i32::try_from((value >> 24) % 112).unwrap_or(0);
            let kind = match (value >> 40) % 100 {
                0..=2 if y <= 16 => BlockKind::DiamondOre,
                3..=9 if y <= 16 => BlockKind::RedstoneOre,
                10..=14 if y <= 32 => BlockKind::GoldOre,
                15..=20 => BlockKind::LapisOre,
                21..=40 => BlockKind::IronOre,
                41..=58 => BlockKind::CopperOre,
                _ => BlockKind::CoalOre,
            };
            let length = 3 + i32::try_from((value >> 48) % 5).unwrap_or(0);
            for step in 0..length {
                let position = BlockPosition {
                    x: x + (step % 3) - 1,
                    y: y + ((step / 3) % 2),
                    z: z + ((step * 2) % 3) - 1,
                };
                if self.block_at(position) == BlockKind::Stone {
                    self.insert_if_inside(position, kind);
                }
            }
        }
    }

    fn generate_gravel_pockets(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        for pocket in 0..6_i32 {
            let value = mix(
                seed as u64 ^ 0x6772_6176_656c,
                self.position.x.wrapping_mul(17).wrapping_add(pocket),
                self.position.z,
            );
            let center = BlockPosition {
                x: min_x + i32::try_from((value >> 8) % 16).unwrap_or(0),
                y: 16 + i32::try_from((value >> 24) % 40).unwrap_or(0),
                z: min_z + i32::try_from((value >> 16) % 16).unwrap_or(0),
            };
            for offset in [
                (0, 0, 0),
                (1, 0, 0),
                (-1, 0, 0),
                (0, 0, 1),
                (0, 0, -1),
                (0, 1, 0),
            ] {
                let position = BlockPosition {
                    x: center.x + offset.0,
                    y: center.y + offset.1,
                    z: center.z + offset.2,
                };
                if self.block_at(position) == BlockKind::Stone {
                    self.insert_if_inside(position, BlockKind::Gravel);
                }
            }
        }
    }

    fn generate_trees(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        for cell_z in (min_z - 4).div_euclid(8)..=(min_z + 19).div_euclid(8) {
            for cell_x in (min_x - 4).div_euclid(8)..=(min_x + 19).div_euclid(8) {
                let value = mix(seed as u64 ^ 0x5452_4545, cell_x, cell_z);
                let x = cell_x * 8 + i32::try_from((value >> 8) % 8).unwrap_or(0);
                let z = cell_z * 8 + i32::try_from((value >> 16) % 8).unwrap_or(0);
                if x.abs() < 12 && z.abs() < 12 {
                    continue;
                }
                let biome = biome_at(seed, DimensionKind::Overworld, x, z);
                let biome_id = biome.protocol_id();
                let spacing = if matches!(biome_id, 25 | 37 | 38 | 48 | 56) {
                    4
                } else if matches!(
                    biome_id,
                    1 | 4 | 5 | 8 | 20 | 21 | 28 | 30 | 31 | 36 | 39 | 50 | 61
                ) {
                    3
                } else if biome_id == 40 || biome_id == 54 {
                    13
                } else {
                    continue;
                };
                if value % spacing != 0 {
                    continue;
                }
                let ground = terrain_height(seed, DimensionKind::Overworld, x, z);
                if ground < 63 {
                    continue;
                }
                match biome_id {
                    25 | 37 | 38 | 48 | 56 => {
                        let height = 6 + i32::try_from((value >> 24) % 4).unwrap_or(0);
                        self.generate_spruce(x, ground, z, height);
                    }
                    4 | 36 => {
                        let height = 5 + i32::try_from((value >> 24) % 3).unwrap_or(0);
                        self.generate_broadleaf(
                            x,
                            ground,
                            z,
                            height,
                            BlockKind::BirchLog,
                            BlockKind::BirchLeaves,
                        );
                    }
                    _ => {
                        let height = 4 + i32::try_from((value >> 24) % 3).unwrap_or(0);
                        self.generate_broadleaf(
                            x,
                            ground,
                            z,
                            height,
                            BlockKind::OakLog,
                            BlockKind::OakLeaves,
                        );
                    }
                }
            }
        }
    }

    fn generate_broadleaf(
        &mut self,
        x: i32,
        ground: i32,
        z: i32,
        height: i32,
        log: BlockKind,
        leaves: BlockKind,
    ) {
        for y in ground + height - 2..=ground + height + 1 {
            let radius = if y == ground + height + 1 { 1 } else { 2 };
            for leaf_z in z - radius..=z + radius {
                for leaf_x in x - radius..=x + radius {
                    if (leaf_x - x).abs() + (leaf_z - z).abs() <= radius + 1 {
                        self.insert_if_inside(
                            BlockPosition {
                                x: leaf_x,
                                y,
                                z: leaf_z,
                            },
                            leaves,
                        );
                    }
                }
            }
        }
        for y in ground + 1..=ground + height {
            self.insert_if_inside(BlockPosition { x, y, z }, log);
        }
    }

    fn generate_spruce(&mut self, x: i32, ground: i32, z: i32, height: i32) {
        for y in ground + 2..=ground + height + 1 {
            let distance_from_top = ground + height + 1 - y;
            let radius = if distance_from_top == 0 {
                0
            } else {
                1 + (distance_from_top / 2).min(2)
            };
            for leaf_z in z - radius..=z + radius {
                for leaf_x in x - radius..=x + radius {
                    if (leaf_x - x).abs().max((leaf_z - z).abs()) <= radius {
                        self.insert_if_inside(
                            BlockPosition {
                                x: leaf_x,
                                y,
                                z: leaf_z,
                            },
                            BlockKind::SpruceLeaves,
                        );
                    }
                }
            }
        }
        for y in ground + 1..=ground + height {
            self.insert_if_inside(BlockPosition { x, y, z }, BlockKind::SpruceLog);
        }
    }

    fn generate_structures(&mut self, seed: i64) {
        let min_x = self.position.x * CHUNK_WIDTH;
        let min_z = self.position.z * CHUNK_WIDTH;
        for cell_z in (min_z - 12).div_euclid(96)..=(min_z + 27).div_euclid(96) {
            for cell_x in (min_x - 12).div_euclid(96)..=(min_x + 27).div_euclid(96) {
                let value = mix(
                    seed as u64 ^ 0x57a1_c7e5 ^ self.dimension.type_id() as u64,
                    cell_x,
                    cell_z,
                );
                if value % 4 != 0 {
                    continue;
                }
                let x = cell_x * 96 + 24 + i32::try_from((value >> 8) % 48).unwrap_or(0);
                let z = cell_z * 96 + 24 + i32::try_from((value >> 16) % 48).unwrap_or(0);
                if x.abs() < 20 && z.abs() < 20 {
                    continue;
                }
                let ground = terrain_height(seed, self.dimension, x, z);
                if self.dimension == DimensionKind::End
                    && (-3..=3).any(|dz| {
                        (-3..=3).any(|dx| end_island_column(seed, x + dx, z + dz).is_none())
                    })
                {
                    continue;
                }
                match self.dimension {
                    DimensionKind::Overworld => {
                        let biome = biome_at(seed, DimensionKind::Overworld, x, z);
                        let profile = terrain_profile(DimensionKind::Overworld, biome);
                        let (foundation, wall, roof, pillar) = match profile.surface {
                            BlockKind::Sand => (
                                BlockKind::Sandstone,
                                BlockKind::Sandstone,
                                BlockKind::Sandstone,
                                BlockKind::Sandstone,
                            ),
                            BlockKind::SnowBlock => (
                                BlockKind::StoneBricks,
                                BlockKind::SpruceLog,
                                BlockKind::OakPlanks,
                                BlockKind::SpruceLog,
                            ),
                            _ if biome == BiomeKind::BirchForest => (
                                BlockKind::Cobblestone,
                                BlockKind::OakPlanks,
                                BlockKind::OakPlanks,
                                BlockKind::BirchLog,
                            ),
                            _ => (
                                BlockKind::Cobblestone,
                                BlockKind::OakPlanks,
                                BlockKind::OakPlanks,
                                BlockKind::OakLog,
                            ),
                        };
                        if (value >> 24) & 1 == 0 {
                            self.generate_house(x, ground, z, foundation, wall, roof);
                            self.record_structure(
                                GeneratedStructureKind::House,
                                BlockPosition { x, y: ground, z },
                                BlockPosition {
                                    x: x + 2,
                                    y: ground + 2,
                                    z: z + 2,
                                },
                            );
                        } else {
                            self.generate_trail_lookout(x, ground, z, foundation, pillar, roof);
                            self.record_structure(
                                GeneratedStructureKind::TrailLookout,
                                BlockPosition { x, y: ground, z },
                                BlockPosition {
                                    x,
                                    y: ground + 2,
                                    z: z + 1,
                                },
                            );
                        }
                    }
                    DimensionKind::Nether => {
                        if (value >> 24) & 1 == 0 {
                            self.generate_ruined_portal(x, ground, z);
                            self.record_structure(
                                GeneratedStructureKind::RuinedPortal,
                                BlockPosition { x, y: ground, z },
                                BlockPosition {
                                    x,
                                    y: ground + 1,
                                    z: z + 1,
                                },
                            );
                        } else {
                            self.generate_basalt_waymark(x, ground, z);
                            self.record_structure(
                                GeneratedStructureKind::BasaltWaymark,
                                BlockPosition { x, y: ground, z },
                                BlockPosition {
                                    x,
                                    y: ground + 2,
                                    z: z + 1,
                                },
                            );
                        }
                    }
                    DimensionKind::End => {
                        if (value >> 24) & 1 == 0 {
                            self.generate_end_obelisk(x, ground, z);
                            self.record_structure(
                                GeneratedStructureKind::EndObelisk,
                                BlockPosition { x, y: ground, z },
                                BlockPosition {
                                    x: x + 2,
                                    y: ground + 1,
                                    z: z + 2,
                                },
                            );
                        } else {
                            self.generate_end_arch(x, ground, z);
                            self.record_structure(
                                GeneratedStructureKind::EndArch,
                                BlockPosition { x, y: ground, z },
                                BlockPosition {
                                    x: x + 2,
                                    y: ground + 1,
                                    z: z + 2,
                                },
                            );
                        }
                    }
                }
            }
        }
        match self.dimension {
            DimensionKind::Overworld => {
                self.generate_active_portal(0, terrain_height(seed, self.dimension, 0, -8), -8);
                self.generate_active_end_portal(8, terrain_height(seed, self.dimension, 8, -8), -8);
            }
            DimensionKind::Nether => {
                self.generate_active_portal(0, terrain_height(seed, self.dimension, 0, -8), -8)
            }
            DimensionKind::End => {
                self.generate_active_end_portal(8, terrain_height(seed, self.dimension, 8, -8), -8)
            }
        }
    }

    fn generate_house(
        &mut self,
        x: i32,
        ground: i32,
        z: i32,
        foundation: BlockKind,
        wall: BlockKind,
        roof: BlockKind,
    ) {
        for dz in -3_i32..=3 {
            for dx in -3_i32..=3 {
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground + 1,
                        z: z + dz,
                    },
                    foundation,
                );
            }
        }
        for y in ground + 2..=ground + 5 {
            for edge in -3..=3 {
                for (dx, dz) in [(edge, -3), (edge, 3), (-3, edge), (3, edge)] {
                    let doorway = dz == -3 && dx == 0 && y <= ground + 3;
                    self.insert_if_inside(
                        BlockPosition {
                            x: x + dx,
                            y,
                            z: z + dz,
                        },
                        if doorway { BlockKind::Air } else { wall },
                    );
                }
            }
        }
        for dz in -4..=4 {
            for dx in -4..=4 {
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground + 6,
                        z: z + dz,
                    },
                    roof,
                );
            }
        }
    }

    fn record_structure(
        &mut self,
        kind: GeneratedStructureKind,
        anchor: BlockPosition,
        loot_chest: BlockPosition,
    ) {
        self.insert_if_inside(loot_chest, BlockKind::Chest);
        if loot_chest.x.div_euclid(CHUNK_WIDTH) == self.position.x
            && loot_chest.z.div_euclid(CHUNK_WIDTH) == self.position.z
        {
            self.structures.push(GeneratedStructure {
                kind,
                anchor,
                loot_chest,
            });
        }
    }

    fn generate_trail_lookout(
        &mut self,
        x: i32,
        ground: i32,
        z: i32,
        foundation: BlockKind,
        pillar: BlockKind,
        roof: BlockKind,
    ) {
        for dz in -2..=2 {
            for dx in -2..=2 {
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground,
                        z: z + dz,
                    },
                    foundation,
                );
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground + 1,
                        z: z + dz,
                    },
                    foundation,
                );
            }
        }
        for (dx, dz) in [(-2, -2), (-2, 2), (2, -2), (2, 2)] {
            for y in ground + 2..=ground + 6 {
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y,
                        z: z + dz,
                    },
                    pillar,
                );
            }
        }
        for dz in -3_i32..=3 {
            for dx in -3_i32..=3 {
                if dx.abs() + dz.abs() <= 5 {
                    self.insert_if_inside(
                        BlockPosition {
                            x: x + dx,
                            y: ground + 7,
                            z: z + dz,
                        },
                        roof,
                    );
                }
            }
        }
        for (dx, dz) in [(-1, 0), (0, 0), (1, 0)] {
            self.insert_if_inside(
                BlockPosition {
                    x: x + dx,
                    y: ground + 2,
                    z: z + dz,
                },
                BlockKind::Cobblestone,
            );
        }
    }

    fn generate_ruined_portal(&mut self, x: i32, ground: i32, z: i32) {
        for y in 0..=5 {
            for dx in -2_i32..=2 {
                if y == 0 || y == 5 || dx.abs() == 2 {
                    self.insert_if_inside(
                        BlockPosition {
                            x: x + dx,
                            y: ground + 1 + y,
                            z,
                        },
                        BlockKind::Obsidian,
                    );
                }
            }
        }
        for y in 1..=4 {
            for dx in -1..=1 {
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground + 1 + y,
                        z,
                    },
                    BlockKind::NetherPortal,
                );
            }
        }
    }

    fn generate_basalt_waymark(&mut self, x: i32, ground: i32, z: i32) {
        for dz in -1..=1 {
            for dx in -3..=3 {
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground,
                        z: z + dz,
                    },
                    BlockKind::Basalt,
                );
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground + 1,
                        z: z + dz,
                    },
                    BlockKind::SoulSand,
                );
            }
        }
        for dx in [-3, 3] {
            for y in ground + 2..=ground + 7 {
                self.insert_if_inside(BlockPosition { x: x + dx, y, z }, BlockKind::Basalt);
            }
        }
        for dx in -3..=3 {
            self.insert_if_inside(
                BlockPosition {
                    x: x + dx,
                    y: ground + 7,
                    z,
                },
                BlockKind::Basalt,
            );
        }
        self.insert_if_inside(
            BlockPosition {
                x,
                y: ground + 6,
                z,
            },
            BlockKind::Obsidian,
        );
    }

    fn generate_active_portal(&mut self, x: i32, ground: i32, z: i32) {
        for y in 0..=5 {
            for dx in -2_i32..=2 {
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground + 1 + y,
                        z,
                    },
                    if y == 0 || y == 5 || dx.abs() == 2 {
                        BlockKind::Obsidian
                    } else {
                        BlockKind::NetherPortal
                    },
                );
            }
        }
    }

    fn generate_active_end_portal(&mut self, x: i32, ground: i32, z: i32) {
        for dz in -2_i32..=2 {
            for dx in -2_i32..=2 {
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground,
                        z: z + dz,
                    },
                    BlockKind::Obsidian,
                );
                self.insert_if_inside(
                    BlockPosition {
                        x: x + dx,
                        y: ground + 1,
                        z: z + dz,
                    },
                    if dx.abs() == 2 || dz.abs() == 2 {
                        BlockKind::Obsidian
                    } else {
                        BlockKind::EndPortal
                    },
                );
            }
        }
    }

    fn generate_end_obelisk(&mut self, x: i32, ground: i32, z: i32) {
        let height = 8 + i32::try_from(mix(0xe11d, x, z) % 8).unwrap_or(0);
        for y in ground + 1..=ground + height {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    self.insert_if_inside(
                        BlockPosition {
                            x: x + dx,
                            y,
                            z: z + dz,
                        },
                        BlockKind::Obsidian,
                    );
                }
            }
        }
    }

    fn generate_end_arch(&mut self, x: i32, ground: i32, z: i32) {
        for dx in -3..=3 {
            self.insert_if_inside(
                BlockPosition {
                    x: x + dx,
                    y: ground,
                    z,
                },
                BlockKind::EndStone,
            );
        }
        for dx in [-3, 3] {
            for y in ground + 1..=ground + 6 {
                self.insert_if_inside(BlockPosition { x: x + dx, y, z }, BlockKind::Obsidian);
            }
        }
        for dx in -3..=3 {
            self.insert_if_inside(
                BlockPosition {
                    x: x + dx,
                    y: ground + 6,
                    z,
                },
                BlockKind::Obsidian,
            );
        }
        for dx in -1..=1 {
            self.insert_if_inside(
                BlockPosition {
                    x: x + dx,
                    y: ground + 5,
                    z,
                },
                BlockKind::EndStone,
            );
        }
    }

    fn insert_if_inside(&mut self, position: BlockPosition, kind: BlockKind) {
        if position.x.div_euclid(CHUNK_WIDTH) == self.position.x
            && position.z.div_euclid(CHUNK_WIDTH) == self.position.z
        {
            self.blocks.insert(position, kind);
        }
    }

    fn surface_at(&self, x: i32, z: i32) -> i32 {
        let local_x = x.rem_euclid(CHUNK_WIDTH);
        let local_z = z.rem_euclid(CHUNK_WIDTH);
        i32::from(self.surface_y[usize::try_from(local_z * CHUNK_WIDTH + local_x).unwrap_or(0)])
    }

    fn block_at(&self, position: BlockPosition) -> BlockKind {
        if let Some(kind) = self.edits.get(&position) {
            return *kind;
        }
        if let Some(kind) = self.blocks.get(&position) {
            return *kind;
        }
        if let Some(kind) = self.roof_block_at(position) {
            return kind;
        }
        if let Some(bottoms) = &self.end_bottom {
            let column = (position.z.rem_euclid(16) * 16 + position.x.rem_euclid(16)) as usize;
            return if (i32::from(bottoms[column])..=i32::from(self.surface_y[column]))
                .contains(&position.y)
            {
                BlockKind::EndStone
            } else {
                BlockKind::Air
            };
        }
        let surface = self.surface_at(position.x, position.z);
        match position.y {
            y if y > surface => BlockKind::Air,
            y if y == surface => self.terrain.surface,
            y if y >= surface - 3 => self.terrain.filler,
            y if y <= -64 => self.terrain.bedrock,
            _ => self.terrain.foundation,
        }
    }

    fn roof_block_at(&self, position: BlockPosition) -> Option<BlockKind> {
        let ceiling = self.nether_ceiling.as_ref()?;
        let column = (position.z.rem_euclid(16) * 16 + position.x.rem_euclid(16)) as usize;
        if (i32::from(ceiling[column])..=NETHER_ROOF_Y).contains(&position.y) {
            Some(if position.y == NETHER_ROOF_Y {
                BlockKind::Bedrock
            } else {
                self.terrain.foundation
            })
        } else {
            None
        }
    }

    fn placements(&self) -> Vec<BlockPlacement> {
        // Keep ceilings compact in memory; expand only for chunk transmission.
        // Explicit generated features and saved edits retain precedence.
        let mut blocks = HashMap::new();
        if let Some(bottoms) = &self.end_bottom {
            for local_z in 0..16 {
                for local_x in 0..16 {
                    let column = (local_z * 16 + local_x) as usize;
                    for y in i32::from(bottoms[column])..=i32::from(self.surface_y[column]) {
                        blocks.insert(
                            BlockPosition {
                                x: self.position.x * 16 + local_x,
                                y,
                                z: self.position.z * 16 + local_z,
                            },
                            BlockKind::EndStone,
                        );
                    }
                }
            }
        }
        if let Some(ceiling) = &self.nether_ceiling {
            for local_z in 0..16 {
                for local_x in 0..16 {
                    let column = (local_z * 16 + local_x) as usize;
                    for y in i32::from(ceiling[column])..=NETHER_ROOF_Y {
                        let position = BlockPosition {
                            x: self.position.x * 16 + local_x,
                            y,
                            z: self.position.z * 16 + local_z,
                        };
                        blocks.insert(
                            position,
                            self.roof_block_at(position).expect("inside Nether ceiling"),
                        );
                    }
                }
            }
        }
        blocks.extend(
            self.blocks
                .iter()
                .map(|(position, kind)| (*position, *kind)),
        );
        blocks.extend(self.edits.iter().map(|(position, kind)| (*position, *kind)));
        blocks
            .into_iter()
            .map(|(position, kind)| BlockPlacement { position, kind })
            .collect()
    }

    fn snapshot(&self) -> ChunkSnapshot {
        let min_surface_y = self.surface_y.iter().copied().min().unwrap_or(64).into();
        let max_surface_y = self.surface_y.iter().copied().max().unwrap_or(64).into();
        ChunkSnapshot {
            position: self.position,
            dimension: self.dimension,
            biome: self.biome,
            min_surface_y,
            max_surface_y,
            revision: self.revision,
        }
    }
}

struct MobController {
    snapshot: MobSnapshot,
    target_x: f64,
    target_z: f64,
    next_decision_tick: u64,
    path: VecDeque<(i32, i32)>,
    path_goal: Option<(i32, i32)>,
    next_repath_tick: u64,
    stuck_ticks: u16,
}

pub struct PrototypeWorld {
    seed: i64,
    dimension: DimensionKind,
    chunks: HashMap<ChunkPosition, GeneratedChunk>,
    mobs: Vec<MobController>,
    items: Vec<ItemEntitySnapshot>,
    next_entity_id: i32,
}

impl PrototypeWorld {
    pub fn new(seed: i64) -> Self {
        Self::new_dimension(seed, DimensionKind::Overworld)
    }

    pub fn new_dimension(seed: i64, dimension: DimensionKind) -> Self {
        let mut chunks = HashMap::new();
        let (initial_min, initial_max) = if dimension == DimensionKind::Overworld {
            (-9, 7)
        } else {
            (-1, 1)
        };
        for z in initial_min..=initial_max {
            for x in initial_min..=initial_max {
                let position = ChunkPosition { x, z };
                chunks.insert(
                    position,
                    GeneratedChunk::generate_in(seed, dimension, position),
                );
            }
        }
        let mut world = Self {
            seed,
            dimension,
            chunks,
            mobs: Vec::new(),
            items: Vec::new(),
            next_entity_id: 1_000,
        };
        if dimension == DimensionKind::Overworld {
            world.spawn_mob(
                MobKind::Cow,
                BlockPosition {
                    x: -12,
                    y: 65,
                    z: -7,
                },
            );
            world.spawn_mob(
                MobKind::Pig,
                BlockPosition {
                    x: -4,
                    y: 65,
                    z: -12,
                },
            );
            world.spawn_mob(
                MobKind::Zombie,
                BlockPosition {
                    x: -13,
                    y: 65,
                    z: -13,
                },
            );
        }
        world
    }

    pub fn chunks(&self) -> Vec<ChunkSnapshot> {
        let mut chunks: Vec<_> = self.chunks.values().map(GeneratedChunk::snapshot).collect();
        chunks.sort_by_key(|chunk| (chunk.position.z, chunk.position.x));
        chunks
    }

    pub fn chunk_surface(&self, position: ChunkPosition) -> Option<[i16; 256]> {
        self.chunks.get(&position).map(|chunk| chunk.surface_y)
    }

    pub fn ensure_chunk_surface(&mut self, position: ChunkPosition) -> [i16; 256] {
        self.chunks
            .entry(position)
            .or_insert_with(|| GeneratedChunk::generate_in(self.seed, self.dimension, position))
            .surface_y
    }

    pub fn ensure_chunk_biome(&mut self, position: ChunkPosition) -> BiomeKind {
        self.chunks
            .entry(position)
            .or_insert_with(|| GeneratedChunk::generate_in(self.seed, self.dimension, position))
            .biome
    }

    pub fn ensure_chunk_terrain(&mut self, position: ChunkPosition) -> TerrainProfile {
        self.chunks
            .entry(position)
            .or_insert_with(|| GeneratedChunk::generate_in(self.seed, self.dimension, position))
            .terrain
    }

    pub fn mobs(&self) -> Vec<MobSnapshot> {
        self.mobs.iter().map(|mob| mob.snapshot.clone()).collect()
    }

    pub fn spawn_mob(&mut self, kind: MobKind, position: BlockPosition) -> MobSnapshot {
        let entity_id = self.next_entity_id;
        self.next_entity_id = self.next_entity_id.saturating_add(1);
        let ground_y = surface_height(&self.chunks, f64::from(position.x), f64::from(position.z));
        let position = EntityPosition {
            x: f64::from(position.x) + 0.5,
            y: f64::from(ground_y) + 1.0,
            z: f64::from(position.z) + 0.5,
        };
        let snapshot = MobSnapshot {
            entity_id,
            id: Uuid::from_u128(
                0xcab0_0000_0000_4000_8000_0000_0000_0000_u128
                    | u128::from(u32::try_from(entity_id).unwrap_or_default()),
            ),
            kind,
            position,
            velocity: EntityPosition::default(),
            yaw: 0.0,
            ai_state: MobAiState::Idle,
            health: 20.0,
            on_fire: false,
        };
        self.mobs.push(MobController {
            snapshot: snapshot.clone(),
            target_x: position.x,
            target_z: position.z,
            next_decision_tick: 1,
            path: VecDeque::new(),
            path_goal: None,
            next_repath_tick: 1,
            stuck_ticks: 0,
        });
        snapshot
    }

    pub fn damage_mob(&mut self, entity_id: i32, amount: f32) -> Option<MobSnapshot> {
        let mob = self
            .mobs
            .iter_mut()
            .find(|mob| mob.snapshot.entity_id == entity_id)?;
        mob.snapshot.health = (mob.snapshot.health - amount.max(0.0)).max(0.0);
        Some(mob.snapshot.clone())
    }

    pub fn tick(&mut self, tick: u64, players: &[PlayerSnapshot]) {
        let seed = self.seed;
        let chunks = &self.chunks;
        let daylight = tick % 24_000 < 12_000;
        for mob in &mut self.mobs {
            mob.snapshot.on_fire =
                mob.snapshot.kind == MobKind::Zombie && daylight && !players.is_empty();
            if mob.snapshot.on_fire && tick % 20 == 0 {
                mob.snapshot.health = (mob.snapshot.health - 1.0).max(0.0);
            }

            let chase_target = if mob.snapshot.kind == MobKind::Zombie {
                nearest_player(&mob.snapshot, players, 16.0)
            } else {
                None
            };

            if let Some(target) = chase_target {
                mob.target_x = f64::from(target.x) + 0.5;
                mob.target_z = f64::from(target.z) + 0.5;
                mob.snapshot.ai_state = MobAiState::Chasing;
            } else if tick >= mob.next_decision_tick {
                let value = mix(seed as u64 ^ mob.snapshot.entity_id as u64, tick as i32, 0);
                let angle = (value as f64 / u64::MAX as f64) * std::f64::consts::TAU;
                mob.target_x = mob.snapshot.position.x + angle.cos() * 6.0;
                mob.target_z = mob.snapshot.position.z + angle.sin() * 6.0;
                mob.next_decision_tick = tick.saturating_add(60 + value % 80);
                mob.snapshot.ai_state = MobAiState::Wandering;
            }

            let goal = (mob.target_x.floor() as i32, mob.target_z.floor() as i32);
            let start = (
                mob.snapshot.position.x.floor() as i32,
                mob.snapshot.position.z.floor() as i32,
            );
            let goal_changed = mob.path_goal != Some(goal);
            if goal_changed || tick >= mob.next_repath_tick || mob.stuck_ticks >= 20 {
                mob.path = find_path(chunks, start, goal, 4_096).unwrap_or_default();
                mob.path_goal = Some(goal);
                mob.next_repath_tick =
                    tick.saturating_add(if mob.snapshot.ai_state == MobAiState::Chasing {
                        10
                    } else {
                        40
                    });
                mob.stuck_ticks = 0;
            }
            while let Some((x, z)) = mob.path.front().copied() {
                let waypoint_x = f64::from(x) + 0.5;
                let waypoint_z = f64::from(z) + 0.5;
                if (waypoint_x - mob.snapshot.position.x)
                    .hypot(waypoint_z - mob.snapshot.position.z)
                    > 0.12
                {
                    break;
                }
                mob.path.pop_front();
            }
            // An empty path at a different goal means A* found no safe route;
            // remain in place instead of falling back to collision-blind movement.
            let (move_x, move_z) = mob.path.front().copied().unwrap_or(start);
            let waypoint_x = f64::from(move_x) + 0.5;
            let waypoint_z = f64::from(move_z) + 0.5;
            let dx = waypoint_x - mob.snapshot.position.x;
            let dz = waypoint_z - mob.snapshot.position.z;
            let distance = dx.hypot(dz);
            let old = mob.snapshot.position;
            if distance > 0.03 {
                let speed: f64 = if mob.snapshot.ai_state == MobAiState::Chasing {
                    0.12
                } else {
                    0.075
                };
                let step = speed.min(distance);
                mob.snapshot.position.x += dx / distance * step;
                mob.snapshot.position.z += dz / distance * step;
                mob.snapshot.yaw = (-dx).atan2(dz).to_degrees() as f32;
            } else {
                mob.snapshot.ai_state = MobAiState::Idle;
            }
            mob.snapshot.position.y = f64::from(surface_height(
                chunks,
                mob.snapshot.position.x,
                mob.snapshot.position.z,
            )) + 1.0;
            mob.snapshot.velocity = EntityPosition {
                x: mob.snapshot.position.x - old.x,
                y: mob.snapshot.position.y - old.y,
                z: mob.snapshot.position.z - old.z,
            };
            if mob.snapshot.velocity.x.hypot(mob.snapshot.velocity.z) < 0.000_1
                && !mob.path.is_empty()
            {
                mob.stuck_ticks = mob.stuck_ticks.saturating_add(1);
            } else {
                mob.stuck_ticks = 0;
            }
        }
        self.mobs.retain(|mob| mob.snapshot.health > 0.0);
        for item in &mut self.items {
            item.age_ticks = item.age_ticks.saturating_add(1);
            let ground = f64::from(collision_ground_below(
                &self.chunks,
                item.position.x,
                item.position.y,
                item.position.z,
            ));
            if item.position.y > ground {
                item.velocity.y -= 0.04;
                item.position.x += item.velocity.x;
                item.position.y = (item.position.y + item.velocity.y).max(ground);
                item.position.z += item.velocity.z;
                item.velocity.x *= 0.98;
                item.velocity.y *= 0.98;
                item.velocity.z *= 0.98;
            } else {
                item.position.y = ground;
                item.velocity = EntityPosition::default();
            }
        }
        self.items.retain(|item| item.age_ticks < 6_000);
        if tick % 200 == 0 {
            self.prune_chunk_cache(players);
        }
    }

    fn prune_chunk_cache(&mut self, players: &[PlayerSnapshot]) {
        let mob_chunks: Vec<_> = self
            .mobs
            .iter()
            .map(|mob| ChunkPosition {
                x: floor_to_chunk(mob.snapshot.position.x),
                z: floor_to_chunk(mob.snapshot.position.z),
            })
            .collect();
        self.chunks.retain(|position, chunk| {
            !chunk.edits.is_empty()
                || players.iter().any(|player| {
                    (player.position.x.div_euclid(CHUNK_WIDTH) - position.x).abs() <= 12
                        && (player.position.z.div_euclid(CHUNK_WIDTH) - position.z).abs() <= 12
                })
                || mob_chunks
                    .iter()
                    .any(|mob| (mob.x - position.x).abs() <= 2 && (mob.z - position.z).abs() <= 2)
        });
    }

    pub fn block_at(&self, position: BlockPosition) -> BlockKind {
        let chunk_position = ChunkPosition {
            x: position.x.div_euclid(CHUNK_WIDTH),
            z: position.z.div_euclid(CHUNK_WIDTH),
        };
        self.chunks
            .get(&chunk_position)
            .map_or(BlockKind::Air, |chunk| chunk.block_at(position))
    }

    pub(crate) fn generated_structure_at(
        &self,
        position: BlockPosition,
    ) -> Option<GeneratedStructure> {
        let chunk_position = ChunkPosition {
            x: position.x.div_euclid(CHUNK_WIDTH),
            z: position.z.div_euclid(CHUNK_WIDTH),
        };
        let chunk = self.chunks.get(&chunk_position)?;
        if chunk.edits.contains_key(&position)
            || chunk.blocks.get(&position) != Some(&BlockKind::Chest)
        {
            return None;
        }
        chunk
            .structures
            .iter()
            .copied()
            .find(|structure| structure.loot_chest == position)
    }

    pub(crate) fn generated_chest_loot(&self, position: BlockPosition) -> Option<ChestSnapshot> {
        let structure = self.generated_structure_at(position)?;
        Some(structure_loot(self.seed, self.dimension, structure))
    }

    pub fn chunk_blocks(&self, position: ChunkPosition) -> Vec<BlockPlacement> {
        self.chunks
            .get(&position)
            .map_or_else(Vec::new, GeneratedChunk::placements)
    }

    pub fn set_block(&mut self, position: BlockPosition, kind: BlockKind) -> bool {
        let seed = self.seed;
        let chunk_position = ChunkPosition {
            x: position.x.div_euclid(CHUNK_WIDTH),
            z: position.z.div_euclid(CHUNK_WIDTH),
        };
        let chunk = self
            .chunks
            .entry(chunk_position)
            .or_insert_with(|| GeneratedChunk::generate_in(seed, self.dimension, chunk_position));
        if chunk.block_at(position) == kind {
            return false;
        }
        chunk.edits.insert(position, kind);
        chunk.revision = chunk.revision.saturating_add(1);
        true
    }

    pub fn edits(&self) -> Vec<BlockPlacement> {
        self.chunks
            .values()
            .flat_map(|chunk| {
                chunk.edits.iter().map(|(position, kind)| BlockPlacement {
                    position: *position,
                    kind: *kind,
                })
            })
            .collect()
    }

    pub fn apply_edit(&mut self, placement: BlockPlacement) {
        let seed = self.seed;
        let chunk_position = ChunkPosition {
            x: placement.position.x.div_euclid(CHUNK_WIDTH),
            z: placement.position.z.div_euclid(CHUNK_WIDTH),
        };
        let chunk = self
            .chunks
            .entry(chunk_position)
            .or_insert_with(|| GeneratedChunk::generate_in(seed, self.dimension, chunk_position));
        chunk.edits.insert(placement.position, placement.kind);
        chunk.revision = chunk.revision.saturating_add(1);
    }

    pub fn items(&self) -> Vec<ItemEntitySnapshot> {
        self.items.clone()
    }

    pub fn drop_item(&mut self, stack: ItemStack, position: BlockPosition) -> ItemEntitySnapshot {
        if self.items.len() >= 4_096 {
            self.items.remove(0);
        }
        let entity = ItemEntitySnapshot {
            entity_id: self.next_entity_id,
            id: Uuid::new_v4(),
            stack,
            position: EntityPosition {
                x: f64::from(position.x) + 0.5,
                y: f64::from(position.y) + 0.5,
                z: f64::from(position.z) + 0.5,
            },
            velocity: EntityPosition {
                x: 0.02,
                y: 0.16,
                z: -0.02,
            },
            age_ticks: 0,
        };
        self.next_entity_id = self.next_entity_id.saturating_add(1);
        self.items.push(entity.clone());
        entity
    }

    pub fn collect_pickups(&mut self, players: &[PlayerSnapshot]) -> Vec<(Uuid, ItemStack)> {
        let mut picked = Vec::new();
        self.items.retain(|item| {
            let collector = (item.age_ticks >= 10)
                .then(|| {
                    players.iter().find(|player| {
                        let dx = f64::from(player.position.x) + 0.5 - item.position.x;
                        let dy = f64::from(player.position.y) + 0.5 - item.position.y;
                        let dz = f64::from(player.position.z) + 0.5 - item.position.z;
                        dx * dx + dy * dy + dz * dz <= 2.25
                    })
                })
                .flatten();
            if let Some(player) = collector {
                picked.push((player.id, item.stack));
                false
            } else {
                true
            }
        });
        picked
    }
}

fn surface_height(chunks: &HashMap<ChunkPosition, GeneratedChunk>, x: f64, z: f64) -> i32 {
    let block_x = x.floor() as i32;
    let block_z = z.floor() as i32;
    let chunk_position = ChunkPosition {
        x: block_x.div_euclid(CHUNK_WIDTH),
        z: block_z.div_euclid(CHUNK_WIDTH),
    };
    chunks
        .get(&chunk_position)
        .map_or(64, |chunk| chunk.surface_at(block_x, block_z))
}

fn collision_ground_below(
    chunks: &HashMap<ChunkPosition, GeneratedChunk>,
    x: f64,
    from_y: f64,
    z: f64,
) -> i32 {
    let block_x = x.floor() as i32;
    let block_z = z.floor() as i32;
    let start_y = from_y.floor().clamp(-64.0, 319.0) as i32;
    for y in (-64..=start_y).rev() {
        if block_kind_at(chunks, block_x, y, block_z).is_some_and(|kind| !kind.is_passable()) {
            return y + 1;
        }
    }
    -63
}

fn floor_to_chunk(value: f64) -> i32 {
    let block = value
        .floor()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
    block.div_euclid(CHUNK_WIDTH)
}

fn surface_height_at(
    chunks: &HashMap<ChunkPosition, GeneratedChunk>,
    x: i32,
    z: i32,
) -> Option<i32> {
    let chunk_position = ChunkPosition {
        x: x.div_euclid(CHUNK_WIDTH),
        z: z.div_euclid(CHUNK_WIDTH),
    };
    chunks
        .get(&chunk_position)
        .map(|chunk| chunk.surface_at(x, z))
}

fn find_path(
    chunks: &HashMap<ChunkPosition, GeneratedChunk>,
    start: (i32, i32),
    goal: (i32, i32),
    max_visited: usize,
) -> Option<VecDeque<(i32, i32)>> {
    if start == goal {
        return Some(VecDeque::new());
    }
    surface_height_at(chunks, start.0, start.1)?;
    surface_height_at(chunks, goal.0, goal.1)?;
    let mut open = BinaryHeap::new();
    let mut costs = HashMap::new();
    let mut came_from = HashMap::new();
    costs.insert(start, 0_u32);
    open.push((Reverse(path_heuristic(start, goal)), Reverse(0_u32), start));
    let mut visited = 0;

    while let Some((_, Reverse(cost), current)) = open.pop() {
        if cost != *costs.get(&current).unwrap_or(&u32::MAX) {
            continue;
        }
        if current == goal {
            let mut path = VecDeque::new();
            let mut cursor = goal;
            while cursor != start {
                path.push_front(cursor);
                cursor = *came_from.get(&cursor)?;
            }
            return Some(path);
        }
        visited += 1;
        if visited > max_visited {
            return None;
        }
        for (dx, dz) in [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (-1, 1),
            (1, -1),
            (1, 1),
        ] {
            let next = (current.0 + dx, current.1 + dz);
            if !can_step(chunks, current, next) {
                continue;
            }
            if dx != 0
                && dz != 0
                && (!can_step(chunks, current, (current.0 + dx, current.1))
                    || !can_step(chunks, current, (current.0, current.1 + dz)))
            {
                continue;
            }
            let step_cost = if dx == 0 || dz == 0 { 10 } else { 14 };
            let next_cost = cost.saturating_add(step_cost);
            if next_cost < *costs.get(&next).unwrap_or(&u32::MAX) {
                costs.insert(next, next_cost);
                came_from.insert(next, current);
                open.push((
                    Reverse(next_cost.saturating_add(path_heuristic(next, goal))),
                    Reverse(next_cost),
                    next,
                ));
            }
        }
    }
    None
}

fn can_step(
    chunks: &HashMap<ChunkPosition, GeneratedChunk>,
    from: (i32, i32),
    to: (i32, i32),
) -> bool {
    let Some(from_y) = surface_height_at(chunks, from.0, from.1) else {
        return false;
    };
    let Some(to_y) = surface_height_at(chunks, to.0, to.1) else {
        return false;
    };
    let feet = block_kind_at(chunks, to.0, to_y + 1, to.1);
    let head = block_kind_at(chunks, to.0, to_y + 2, to.1);
    (-3..=1).contains(&(to_y - from_y))
        && feet.is_some_and(BlockKind::is_passable)
        && head.is_some_and(BlockKind::is_passable)
}

fn block_kind_at(
    chunks: &HashMap<ChunkPosition, GeneratedChunk>,
    x: i32,
    y: i32,
    z: i32,
) -> Option<BlockKind> {
    let position = BlockPosition { x, y, z };
    chunks
        .get(&ChunkPosition {
            x: x.div_euclid(CHUNK_WIDTH),
            z: z.div_euclid(CHUNK_WIDTH),
        })
        .map(|chunk| chunk.block_at(position))
}

fn path_heuristic(from: (i32, i32), to: (i32, i32)) -> u32 {
    let dx = from.0.abs_diff(to.0);
    let dz = from.1.abs_diff(to.1);
    let diagonal = dx.min(dz);
    diagonal * 14 + (dx.max(dz) - diagonal) * 10
}

fn cave_network_node(seed: i64, cell_x: i32, cell_z: i32) -> BlockPosition {
    let value = mix(seed as u64 ^ 0x6361_7665_6e65_7473, cell_x, cell_z);
    BlockPosition {
        x: cell_x * CAVE_CELL_WIDTH + 16 + ((value >> 8) % 32) as i32,
        y: -24 + ((value >> 24) % 13) as i32,
        z: cell_z * CAVE_CELL_WIDTH + 16 + ((value >> 40) % 32) as i32,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CaveEntrance {
    mouth: BlockPosition,
    turn: BlockPosition,
    node: BlockPosition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CaveAquifer {
    center: BlockPosition,
    radius: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NetherLayer {
    center: BlockPosition,
    mouth: BlockPosition,
    radius: i32,
}

fn nether_layer(seed: i64, cell_x: i32, cell_z: i32) -> Option<NetherLayer> {
    let value = mix(seed as u64 ^ 0x6e65_7468_5f6c_6179, cell_x, cell_z);
    if value % 3 != 0 {
        return None;
    }
    let center = BlockPosition {
        x: cell_x * NETHER_LAYER_CELL_WIDTH + 24 + ((value >> 8) % 48) as i32,
        y: 8 + ((value >> 24) % 16) as i32,
        z: cell_z * NETHER_LAYER_CELL_WIDTH + 24 + ((value >> 40) % 48) as i32,
    };
    let radius = 7 + ((value >> 56) % 4) as i32;
    let (direction_x, direction_z) = match (value >> 32) % 4 {
        0 => (1, 0),
        1 => (-1, 0),
        2 => (0, 1),
        _ => (0, -1),
    };
    let mouth_x = center.x + direction_x * 36;
    let mouth_z = center.z + direction_z * 36;
    if center.x.abs().max(center.z.abs()) <= 48 || mouth_x.abs().max(mouth_z.abs()) <= 48 {
        return None;
    }
    Some(NetherLayer {
        center,
        mouth: BlockPosition {
            x: mouth_x,
            y: terrain_height(seed, DimensionKind::Nether, mouth_x, mouth_z) + 2,
            z: mouth_z,
        },
        radius,
    })
}

fn cave_aquifer(seed: i64, cell_x: i32, cell_z: i32) -> Option<CaveAquifer> {
    let value = mix(seed as u64 ^ 0x6171_7569_6665_7273, cell_x, cell_z);
    if value % 4 != 0 {
        return None;
    }
    Some(CaveAquifer {
        center: cave_network_node(seed, cell_x, cell_z),
        radius: 6 + ((value >> 56) % 6) as i32,
    })
}

fn cave_surface_material(seed: i64, position: BlockPosition) -> Option<BlockKind> {
    let region_x = position.x.div_euclid(96);
    let region_z = position.z.div_euclid(96);
    let value = mix(seed as u64 ^ 0x6361_7665_5f62_696f, region_x, region_z);
    match value % 6 {
        0 if position.y <= 0 => Some(BlockKind::Basalt),
        1 if position.y >= 0 => Some(BlockKind::Sandstone),
        2 if position.y >= 20 => Some(BlockKind::Dirt),
        _ => None,
    }
}

fn cave_entrance(seed: i64, cell_x: i32, cell_z: i32) -> Option<CaveEntrance> {
    let value = mix(seed as u64 ^ 0x656e_7472_616e_6365, cell_x, cell_z);
    if value % 6 != 0 {
        return None;
    }
    let node = cave_network_node(seed, cell_x, cell_z);
    let (direction_x, direction_z) = match (value >> 8) % 4 {
        0 => (1, 0),
        1 => (-1, 0),
        2 => (0, 1),
        _ => (0, -1),
    };
    let mouth_x = node.x + direction_x * CAVE_ENTRANCE_REACH;
    let mouth_z = node.z + direction_z * CAVE_ENTRANCE_REACH;
    if (mouth_x + 8).abs().max((mouth_z + 8).abs()) <= 48 {
        return None;
    }
    for dz in -CAVE_ENTRANCE_RADIUS..=CAVE_ENTRANCE_RADIUS {
        for dx in -CAVE_ENTRANCE_RADIUS..=CAVE_ENTRANCE_RADIUS {
            let x = mouth_x + dx;
            let z = mouth_z + dz;
            let biome = biome_at(seed, DimensionKind::Overworld, x, z);
            if is_ocean_biome(biome) || terrain_height(seed, DimensionKind::Overworld, x, z) < 64 {
                return None;
            }
        }
    }
    let mouth = BlockPosition {
        x: mouth_x,
        y: terrain_height(seed, DimensionKind::Overworld, mouth_x, mouth_z) + 1,
        z: mouth_z,
    };
    let turn = BlockPosition {
        x: node.x - direction_z * CAVE_ENTRANCE_REACH,
        y: node.y + (mouth.y - node.y) / 2,
        z: node.z + direction_x * CAVE_ENTRANCE_REACH,
    };
    Some(CaveEntrance { mouth, turn, node })
}

fn end_island_column(seed: i64, x: i32, z: i32) -> Option<(i32, i32)> {
    let distance = f64::from(x).hypot(f64::from(z));
    let mut strength = (1.0 - distance / 96.0).max(0.0);
    // Outer anchors are at least 320 blocks from the origin; radii <=64
    // preserve the void ring outside the central island.
    for cell_z in z.div_euclid(192) - 1..=z.div_euclid(192) + 1 {
        for cell_x in x.div_euclid(192) - 1..=x.div_euclid(192) + 1 {
            let value = mix(seed as u64 ^ 0x656e_6469_736c, cell_x, cell_z);
            let center_x = cell_x * 192 + 56 + ((value >> 8) % 80) as i32;
            let center_z = cell_z * 192 + 56 + ((value >> 24) % 80) as i32;
            if f64::from(center_x).hypot(f64::from(center_z)) < 320.0 {
                continue;
            }
            let radius = 36.0 + ((value >> 40) % 29) as f64;
            let from_center = f64::from(x - center_x).hypot(f64::from(z - center_z));
            strength = strength.max(1.0 - from_center / radius);
        }
    }
    if strength <= 0.0 {
        return None;
    }
    let natural_top = 60.0 + strength * 10.0 + value_noise(seed as u64 ^ 0xe11d, x, z, 32) * 3.0;
    let top = if distance <= 24.0 {
        64
    } else {
        let blend = smooth_blend(((distance - 24.0) / 48.0).clamp(0.0, 1.0));
        (64.0 + (natural_top - 64.0) * blend).round() as i32
    };
    let thickness = 1 + (strength * 35.0).round() as i32;
    Some((top - thickness + 1, top))
}

fn nether_ceiling_height(seed: i64, x: i32, z: i32) -> i32 {
    // Original continuous ceiling field. The existing Nether floor stays at
    // most Y=60, leaving room for portal frames and safe floor-based arrivals.
    (96.0
        + value_noise(seed as u64 ^ 0x0063_6569_6c69_6e67, x, z, 48) * 10.0
        + value_noise(seed as u64 ^ 0x6e65_7468_6572, x, z, 20) * 3.0)
        .round()
        .clamp(83.0, 109.0) as i32
}

fn terrain_height(seed: i64, dimension: DimensionKind, x: i32, z: i32) -> i32 {
    if dimension == DimensionKind::Nether {
        return (48.0 + value_noise(seed as u64 ^ 0x4e37_4e52, x, z, 32) * 12.0)
            .round()
            .clamp(32.0, 72.0) as i32;
    }
    if dimension == DimensionKind::End {
        return end_island_column(seed, x, z).map_or(-65, |(_, top)| top);
    }
    let spawn_distance = (x + 8).abs().max((z + 8).abs());
    if spawn_distance <= 8 {
        return 64;
    }
    let shape = blended_terrain_shape(seed, x, z);
    let generated = shape.base
        + value_noise(seed as u64, x, z, 64) * shape.relief
        + value_noise(seed as u64 ^ 0xa5a5_a5a5_a5a5_a5a5, x, z, 24) * shape.detail;
    let blend = smooth_blend((f64::from(spawn_distance - 8) / 64.0).clamp(0.0, 1.0));
    (64.0 + (generated - 64.0) * blend)
        .round()
        .clamp(32.0, 144.0) as i32
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerrainShape {
    base: f64,
    relief: f64,
    detail: f64,
}

impl TerrainShape {
    fn blend(self, other: Self, weight: f64) -> Self {
        Self {
            base: self.base + (other.base - self.base) * weight,
            relief: self.relief + (other.relief - self.relief) * weight,
            detail: self.detail + (other.detail - self.detail) * weight,
        }
    }
}

fn biome_terrain_shape(biome: BiomeKind) -> TerrainShape {
    // Original prototype parameters, not vanilla density-function constants.
    let (base, relief, detail) = if is_ocean_biome(biome) {
        if biome.name().starts_with("deep_") {
            (42.0, 4.0, 1.0)
        } else {
            (52.0, 5.0, 1.0)
        }
    } else {
        match biome.name() {
            "frozen_peaks" | "jagged_peaks" | "stony_peaks" => (108.0, 26.0, 3.0),
            "snowy_slopes" | "grove" | "meadow" | "cherry_grove" => (88.0, 15.0, 2.0),
            "windswept_hills"
            | "windswept_forest"
            | "windswept_gravelly_hills"
            | "windswept_savanna" => (84.0, 17.0, 3.0),
            "badlands" | "eroded_badlands" | "wooded_badlands" | "savanna_plateau" => {
                (83.0, 11.0, 2.0)
            }
            "plains" | "sunflower_plains" | "snowy_plains" => (68.0, 3.0, 0.75),
            "desert" | "savanna" => (69.0, 5.0, 1.0),
            "swamp" | "mangrove_swamp" => (62.0, 1.5, 0.5),
            "river" | "frozen_river" => (58.0, 1.0, 0.5),
            "beach" | "snowy_beach" | "stony_shore" => (63.0, 2.0, 0.5),
            _ => (72.0, 7.0, 1.5),
        }
    };
    TerrainShape {
        base,
        relief,
        detail,
    }
}

fn smooth_blend(weight: f64) -> f64 {
    weight * weight * (3.0 - 2.0 * weight)
}

fn terrain_region_blend(coordinate: i32) -> (i32, i32, f64) {
    let region = coordinate.div_euclid(384);
    let local = coordinate.rem_euclid(384);
    // A 96-block blend straddles each region boundary. Interior terrain keeps
    // its biome's full profile; Euclidean division also handles negative space.
    if local < 48 {
        (
            region - 1,
            region,
            smooth_blend(f64::from(local + 48) / 96.0),
        )
    } else if local >= 336 {
        (
            region,
            region + 1,
            smooth_blend(f64::from(local - 336) / 96.0),
        )
    } else {
        (region, region, 0.0)
    }
}

fn blended_terrain_shape(seed: i64, x: i32, z: i32) -> TerrainShape {
    let (left, right, horizontal) = terrain_region_blend(x);
    let (top, bottom, vertical) = terrain_region_blend(z);
    let sample = |region_x, region_z| {
        biome_terrain_shape(biome_at(
            seed,
            DimensionKind::Overworld,
            region_x * 384 + 192,
            region_z * 384 + 192,
        ))
    };
    sample(left, top)
        .blend(sample(right, top), horizontal)
        .blend(
            sample(left, bottom).blend(sample(right, bottom), horizontal),
            vertical,
        )
}

fn is_ocean_biome(biome: BiomeKind) -> bool {
    matches!(
        biome.protocol_id(),
        6 | 9 | 11 | 12 | 13 | 22 | 24 | 29 | 35 | 59
    )
}

fn biome_at(seed: i64, dimension: DimensionKind, x: i32, z: i32) -> BiomeKind {
    const NETHER: [u8; 5] = [2, 7, 34, 49, 60];
    const END: [u8; 6] = [16, 17, 18, 44, 57, 58];
    const OVERWORLD: [u8; 55] = [
        0, 1, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
        30, 31, 32, 33, 35, 36, 37, 38, 39, 40, 41, 42, 43, 45, 46, 47, 48, 50, 51, 52, 53, 54, 55,
        56, 59, 61, 62, 63, 64, 65,
    ];
    let region_x = x.div_euclid(384);
    let region_z = z.div_euclid(384);
    let roll = mix(seed as u64 ^ 0x0b10_b1e5, region_x, region_z);
    let choices: &[u8] = match dimension {
        DimensionKind::Overworld => &OVERWORLD,
        DimensionKind::Nether => &NETHER,
        DimensionKind::End => &END,
    };
    BiomeKind::from_protocol_id(choices[usize::try_from(roll % choices.len() as u64).unwrap_or(0)])
}

fn terrain_profile(dimension: DimensionKind, biome: BiomeKind) -> TerrainProfile {
    let id = biome.protocol_id();
    match dimension {
        DimensionKind::Nether => TerrainProfile {
            surface: if id == 49 {
                BlockKind::SoulSand
            } else {
                BlockKind::Netherrack
            },
            filler: BlockKind::Netherrack,
            foundation: if id == 2 {
                BlockKind::Basalt
            } else {
                BlockKind::Netherrack
            },
            bedrock: BlockKind::Bedrock,
        },
        DimensionKind::End => TerrainProfile {
            surface: BlockKind::EndStone,
            filler: BlockKind::EndStone,
            foundation: BlockKind::EndStone,
            bedrock: BlockKind::EndStone,
        },
        DimensionKind::Overworld if matches!(id, 0 | 3 | 14 | 19 | 45 | 65) => TerrainProfile {
            surface: BlockKind::Sand,
            filler: BlockKind::Sand,
            foundation: BlockKind::Sandstone,
            bedrock: BlockKind::Bedrock,
        },
        DimensionKind::Overworld if matches!(id, 22..=26 | 46..=48) => TerrainProfile {
            surface: BlockKind::SnowBlock,
            filler: BlockKind::Dirt,
            foundation: BlockKind::Stone,
            bedrock: BlockKind::Bedrock,
        },
        DimensionKind::Overworld => TerrainProfile {
            surface: BlockKind::Grass,
            filler: BlockKind::Dirt,
            foundation: BlockKind::Stone,
            bedrock: BlockKind::Bedrock,
        },
    }
}

fn value_noise(seed: u64, x: i32, z: i32, scale: i32) -> f64 {
    let grid_x = x.div_euclid(scale);
    let grid_z = z.div_euclid(scale);
    let tx = f64::from(x.rem_euclid(scale)) / f64::from(scale);
    let tz = f64::from(z.rem_euclid(scale)) / f64::from(scale);
    let smooth_x = tx * tx * (3.0 - 2.0 * tx);
    let smooth_z = tz * tz * (3.0 - 2.0 * tz);
    let sample = |offset_x, offset_z| {
        let value = mix(seed, grid_x + offset_x, grid_z + offset_z);
        (value as f64 / u64::MAX as f64) * 2.0 - 1.0
    };
    let top = sample(0, 0) + (sample(1, 0) - sample(0, 0)) * smooth_x;
    let bottom = sample(0, 1) + (sample(1, 1) - sample(0, 1)) * smooth_x;
    top + (bottom - top) * smooth_z
}

fn nearest_player<'a>(
    mob: &MobSnapshot,
    players: &'a [PlayerSnapshot],
    range: f64,
) -> Option<&'a BlockPosition> {
    players
        .iter()
        .map(|player| {
            let dx = f64::from(player.position.x) + 0.5 - mob.position.x;
            let dz = f64::from(player.position.z) + 0.5 - mob.position.z;
            (&player.position, dx * dx + dz * dz)
        })
        .filter(|(_, distance)| *distance <= range * range)
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(position, _)| position)
}

fn structure_loot(
    seed: i64,
    dimension: DimensionKind,
    structure: GeneratedStructure,
) -> ChestSnapshot {
    let kind_id = match structure.kind {
        GeneratedStructureKind::House => 1,
        GeneratedStructureKind::TrailLookout => 2,
        GeneratedStructureKind::RuinedPortal => 3,
        GeneratedStructureKind::BasaltWaymark => 4,
        GeneratedStructureKind::EndObelisk => 5,
        GeneratedStructureKind::EndArch => 6,
    };
    let roll = mix(
        seed as u64 ^ 0x006c_6f6f_745f_7631 ^ kind_id,
        structure.anchor.x,
        structure.anchor.z,
    );
    let stacks = match dimension {
        DimensionKind::Overworld => [
            ItemStack {
                kind: ItemKind::Apple,
                count: 2 + (roll % 4) as u8,
                damage: 0,
            },
            ItemStack {
                kind: ItemKind::Coal,
                count: 2 + ((roll >> 8) % 5) as u8,
                damage: 0,
            },
            ItemStack {
                kind: ItemKind::IronIngot,
                count: 1 + ((roll >> 16) % 3) as u8,
                damage: 0,
            },
        ],
        DimensionKind::Nether => [
            ItemStack {
                kind: ItemKind::GoldIngot,
                count: 1 + (roll % 3) as u8,
                damage: 0,
            },
            ItemStack {
                kind: ItemKind::Obsidian,
                count: 1 + ((roll >> 8) % 3) as u8,
                damage: 0,
            },
            ItemStack {
                kind: ItemKind::Coal,
                count: 2 + ((roll >> 16) % 5) as u8,
                damage: 0,
            },
        ],
        DimensionKind::End => [
            ItemStack {
                kind: ItemKind::LapisLazuli,
                count: 2 + (roll % 5) as u8,
                damage: 0,
            },
            ItemStack {
                kind: ItemKind::IronIngot,
                count: 1 + ((roll >> 8) % 3) as u8,
                damage: 0,
            },
            ItemStack {
                kind: if (roll >> 16) % 4 == 0 {
                    ItemKind::Diamond
                } else {
                    ItemKind::EndStone
                },
                count: 1,
                damage: 0,
            },
        ],
    };
    let mut chest = ChestSnapshot::default();
    for (offset, stack) in stacks.into_iter().enumerate() {
        let slot = usize::try_from((roll >> (24 + offset * 8)) % 9).unwrap_or(0) + offset * 9;
        chest.slots[slot] = Some(stack);
    }
    chest
}

fn mix(seed: u64, x: i32, z: i32) -> u64 {
    let mut value = seed ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^= (z as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value.wrapping_mul(0x94d0_49bb_1331_11eb) ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbon_api::{GameMode, ItemKind};

    #[test]
    fn chunks_are_generated_deterministically() {
        let first = PrototypeWorld::new(42);
        let second = PrototypeWorld::new(42);
        assert_eq!(first.chunks(), second.chunks());
        assert_eq!(first.chunks().len(), 289);
        assert_eq!(
            first.block_at(BlockPosition {
                x: -8,
                y: -64,
                z: -8
            }),
            BlockKind::Bedrock
        );
        let distinct_heights: std::collections::HashSet<_> = first
            .chunks
            .values()
            .flat_map(|chunk| chunk.surface_y)
            .collect();
        assert!(distinct_heights.len() >= 8);
        let tree_blocks: Vec<_> = first
            .chunks
            .values()
            .flat_map(|chunk| chunk.blocks.values())
            .filter(|kind| {
                matches!(
                    kind,
                    BlockKind::OakLog
                        | BlockKind::OakLeaves
                        | BlockKind::BirchLog
                        | BlockKind::BirchLeaves
                        | BlockKind::SpruceLog
                        | BlockKind::SpruceLeaves
                )
            })
            .collect();
        assert!(tree_blocks.len() > 100);
    }

    #[test]
    fn climate_regions_generate_multiple_biomes_and_tree_species() {
        let mut biomes = std::collections::HashSet::new();
        let mut logs = std::collections::HashSet::new();
        for chunk_z in (-48..=48).step_by(4) {
            for chunk_x in (-48..=48).step_by(4) {
                let chunk = GeneratedChunk::generate(
                    42,
                    ChunkPosition {
                        x: chunk_x,
                        z: chunk_z,
                    },
                );
                biomes.insert(chunk.biome);
                logs.extend(chunk.blocks.values().filter_map(|kind| match kind {
                    BlockKind::OakLog | BlockKind::BirchLog | BlockKind::SpruceLog => Some(*kind),
                    _ => None,
                }));
            }
        }
        assert!(biomes.len() >= 10);
        assert!(!logs.is_empty());
    }

    #[test]
    fn cave_network_has_two_block_clearance_across_positive_and_negative_chunk_seams() {
        for (seed, cell_x, cell_z, offset) in [
            (42, -1, 0, (1, 0)),
            (42, 0, -1, (0, 1)),
            (-7919, 1, 2, (1, 0)),
            (0, -2, -3, (0, 1)),
        ] {
            let start = cave_network_node(seed, cell_x, cell_z);
            let end = cave_network_node(seed, cell_x + offset.0, cell_z + offset.1);
            let min_x = start.x.min(end.x) - CAVE_CHAMBER_RADIUS;
            let max_x = start.x.max(end.x) + CAVE_CHAMBER_RADIUS;
            let min_z = start.z.min(end.z) - CAVE_CHAMBER_RADIUS;
            let max_z = start.z.max(end.z) + CAVE_CHAMBER_RADIUS;
            let mut chunks = HashMap::new();
            // Reverse generation order to exercise independent incoming edges.
            for z in (min_z.div_euclid(16)..=max_z.div_euclid(16)).rev() {
                for x in (min_x.div_euclid(16)..=max_x.div_euclid(16)).rev() {
                    let position = ChunkPosition { x, z };
                    chunks.insert(position, GeneratedChunk::generate(seed, position));
                }
            }
            let is_open = |position: BlockPosition| {
                chunks
                    .get(&ChunkPosition {
                        x: position.x.div_euclid(16),
                        z: position.z.div_euclid(16),
                    })
                    .is_some_and(|chunk| {
                        matches!(chunk.block_at(position), BlockKind::Air | BlockKind::Water)
                    })
            };
            assert!(is_open(start) && is_open(end));
            let mut visited = std::collections::HashSet::from([start]);
            let mut queue = VecDeque::from([start]);
            while let Some(position) = queue.pop_front() {
                if position == end {
                    break;
                }
                for (dx, dy, dz) in [
                    (1, 0, 0),
                    (-1, 0, 0),
                    (0, 1, 0),
                    (0, -1, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                ] {
                    let next = BlockPosition {
                        x: position.x + dx,
                        y: position.y + dy,
                        z: position.z + dz,
                    };
                    let head = BlockPosition {
                        y: next.y + 1,
                        ..next
                    };
                    if (min_x..=max_x).contains(&next.x)
                        && (min_z..=max_z).contains(&next.z)
                        && (-29..=-7).contains(&next.y)
                        && is_open(next)
                        && is_open(head)
                        && visited.insert(next)
                    {
                        queue.push_back(next);
                    }
                }
            }
            assert!(
                visited.contains(&end),
                "disconnected cave edge: {seed}, {cell_x}, {cell_z}"
            );
        }
    }

    #[test]
    fn cave_network_is_seeded_repeatable_bounded_and_preserves_edits() {
        let node = cave_network_node(42, -1, 0);
        assert_ne!(node, cave_network_node(43, -1, 0));
        let position = ChunkPosition {
            x: node.x.div_euclid(16),
            z: node.z.div_euclid(16),
        };
        let mut first = GeneratedChunk::generate(42, position);
        let second = GeneratedChunk::generate(42, position);
        assert_eq!(first.blocks, second.blocks);
        assert_eq!(first.surface_y, second.surface_y);
        assert!(matches!(
            first.block_at(node),
            BlockKind::Air | BlockKind::Water
        ));
        first.edits.insert(node, BlockKind::Cobblestone);
        first.generate_cave_network(42);
        assert_eq!(first.block_at(node), BlockKind::Cobblestone);
        assert!(first
            .placements()
            .iter()
            .any(|block| block.position == node && block.kind == BlockKind::Cobblestone));
        assert!(first
            .blocks
            .keys()
            .all(|block| block.x.div_euclid(16) == position.x
                && block.z.div_euclid(16) == position.z));
        for z in position.z * 16..position.z * 16 + 16 {
            for x in position.x * 16..position.x * 16 + 16 {
                assert_eq!(
                    first.block_at(BlockPosition { x, y: -64, z }),
                    BlockKind::Bedrock
                );
                assert_ne!(
                    first.block_at(BlockPosition {
                        x,
                        y: first.surface_at(x, z),
                        z
                    }),
                    BlockKind::Air
                );
            }
        }
        let mut changed_seed = GeneratedChunk::generate(43, position);
        changed_seed.generate_cave_network(43);
        assert_ne!(first.blocks, changed_seed.blocks);
    }

    #[test]
    fn cave_network_is_overworld_only_and_has_large_junction_chambers() {
        let node = cave_network_node(42, 0, 0);
        let position = ChunkPosition {
            x: node.x.div_euclid(16),
            z: node.z.div_euclid(16),
        };
        for dimension in [DimensionKind::Nether, DimensionKind::End] {
            let chunk = GeneratedChunk::generate_in(42, dimension, position);
            if dimension == DimensionKind::Nether {
                assert_ne!(chunk.block_at(node), BlockKind::Air);
            } else {
                // End columns now have finite undersides, not carved caves.
                assert_eq!(chunk.block_at(node), BlockKind::Air);
                assert!(!chunk.blocks.values().any(|kind| *kind == BlockKind::Air));
            }
        }
        let chunk = GeneratedChunk::generate(42, position);
        // The junction reaches beyond the radius-three tunnel vertically.
        assert_eq!(
            chunk.block_at(BlockPosition {
                y: node.y + 4,
                ..node
            }),
            BlockKind::Air
        );
        assert_eq!(
            chunk.block_at(BlockPosition {
                y: node.y - 4,
                ..node
            }),
            BlockKind::Air
        );
        assert!(chunk
            .placements()
            .iter()
            .any(|block| block.position == node && block.kind == BlockKind::Air));
    }

    #[test]
    fn cave_entrances_are_land_only_connected_and_chunk_order_independent() {
        let seed = 42;
        let (cell_x, cell_z, entrance) = (-32..=32)
            .flat_map(|z| (-32..=32).map(move |x| (x, z)))
            .find_map(|(x, z)| cave_entrance(seed, x, z).map(|entrance| (x, z, entrance)))
            .unwrap();
        assert_ne!(
            entrance,
            cave_entrance(seed + 1, cell_x, cell_z).unwrap_or(CaveEntrance {
                mouth: BlockPosition::default(),
                turn: BlockPosition::default(),
                node: BlockPosition::default(),
            })
        );
        assert!(entrance.mouth.y >= 65);
        assert!(
            (entrance.mouth.x + 8)
                .abs()
                .max((entrance.mouth.z + 8).abs())
                > 48
        );

        let mut chunks = HashMap::new();
        let mut assert_segment = |start: BlockPosition, end: BlockPosition| {
            let delta = [end.x - start.x, end.y - start.y, end.z - start.z];
            let steps = delta
                .iter()
                .map(|value| value.abs())
                .max()
                .unwrap_or(0)
                .max(1);
            let mut previous = start;
            for step in 0..=steps {
                let center = BlockPosition {
                    x: start.x + (delta[0] * step).div_euclid(steps),
                    y: start.y + (delta[1] * step).div_euclid(steps),
                    z: start.z + (delta[2] * step).div_euclid(steps),
                };
                assert!((center.x - previous.x).abs() <= 1);
                assert!((center.y - previous.y).abs() <= 1);
                assert!((center.z - previous.z).abs() <= 1);
                previous = center;
                let chunk_position = ChunkPosition {
                    x: center.x.div_euclid(16),
                    z: center.z.div_euclid(16),
                };
                let chunk = chunks.entry(chunk_position).or_insert_with(|| {
                    GeneratedChunk::generate_in(seed, DimensionKind::Overworld, chunk_position)
                });
                assert!(matches!(
                    chunk.block_at(center),
                    BlockKind::Air | BlockKind::Water
                ));
                assert!(matches!(
                    chunk.block_at(BlockPosition {
                        y: center.y + 1,
                        ..center
                    }),
                    BlockKind::Air | BlockKind::Water
                ));
            }
        };
        assert_segment(entrance.mouth, entrance.turn);
        assert_segment(entrance.turn, entrance.node);
        assert!(chunks.len() > 2);

        let mouth_chunk = ChunkPosition {
            x: entrance.mouth.x.div_euclid(16),
            z: entrance.mouth.z.div_euclid(16),
        };
        let mut edited = GeneratedChunk::generate_in(seed, DimensionKind::Overworld, mouth_chunk);
        edited.edits.insert(entrance.mouth, BlockKind::Stone);
        edited.generate_cave_entrances(seed);
        assert_eq!(edited.block_at(entrance.mouth), BlockKind::Stone);
    }

    #[test]
    fn cave_material_regions_and_junction_aquifers_are_seeded_and_bounded() {
        let seed = 42;
        for (expected, y) in [
            (BlockKind::Basalt, -10),
            (BlockKind::Sandstone, 10),
            (BlockKind::Dirt, 30),
        ] {
            let air = (-32..=32)
                .flat_map(|region_z| (-32..=32).map(move |region_x| (region_x, region_z)))
                .map(|(region_x, region_z)| BlockPosition {
                    x: region_x * 96 + 48,
                    y,
                    z: region_z * 96 + 48,
                })
                .find(|position| {
                    cave_surface_material(seed, *position) == Some(expected)
                        && terrain_profile(
                            DimensionKind::Overworld,
                            biome_at(seed, DimensionKind::Overworld, position.x, position.z),
                        )
                        .foundation
                            == BlockKind::Stone
                })
                .unwrap();
            let chunk_position = ChunkPosition {
                x: air.x.div_euclid(16),
                z: air.z.div_euclid(16),
            };
            let mut chunk = GeneratedChunk::generate(seed, chunk_position);
            chunk.blocks.clear();
            chunk.blocks.insert(air, BlockKind::Air);
            chunk.generate_cave_surfaces(seed);
            assert_eq!(
                chunk.block_at(BlockPosition {
                    x: air.x + 1,
                    ..air
                }),
                expected
            );
        }

        let (cell_x, cell_z, aquifer) = (-32..=32)
            .flat_map(|z| (-32..=32).map(move |x| (x, z)))
            .find_map(|(x, z)| cave_aquifer(seed, x, z).map(|aquifer| (x, z, aquifer)))
            .unwrap();
        assert_ne!(
            aquifer,
            cave_aquifer(seed + 1, cell_x, cell_z).unwrap_or(CaveAquifer {
                center: BlockPosition::default(),
                radius: 0,
            })
        );
        let chunk_position = ChunkPosition {
            x: aquifer.center.x.div_euclid(16),
            z: aquifer.center.z.div_euclid(16),
        };
        let chunk = GeneratedChunk::generate(seed, chunk_position);
        assert_eq!(chunk.block_at(aquifer.center), BlockKind::Water);
        assert_eq!(
            chunk.block_at(BlockPosition {
                y: aquifer.center.y + 1,
                ..aquifer.center
            }),
            BlockKind::Air
        );
        assert!(chunk.blocks.iter().any(|(position, kind)| {
            *kind == BlockKind::Water
                && position.y <= aquifer.center.y
                && (position.x - aquifer.center.x).pow(2)
                    + (position.y - aquifer.center.y).pow(2)
                    + (position.z - aquifer.center.z).pow(2)
                    <= aquifer.radius.pow(2)
        }));
        for dimension in [DimensionKind::Nether, DimensionKind::End] {
            let other = GeneratedChunk::generate_in(seed, dimension, chunk_position);
            assert!(!other
                .blocks
                .iter()
                .any(|(position, kind)| position.y < 0 && *kind == BlockKind::Water));
        }
    }

    #[test]
    fn biome_terrain_interiors_have_distinct_elevations() {
        let mut peaks = Vec::new();
        let mut plains = Vec::new();
        let mut oceans = Vec::new();
        for region_z in -12..=12 {
            for region_x in -12..=12 {
                let x = region_x * 384 + 192;
                let z = region_z * 384 + 192;
                let biome = biome_at(42, DimensionKind::Overworld, x, z);
                assert_eq!(blended_terrain_shape(42, x, z), biome_terrain_shape(biome));
                let height = terrain_height(42, DimensionKind::Overworld, x, z);
                match biome.name() {
                    "jagged_peaks" | "frozen_peaks" | "stony_peaks" => peaks.push(height),
                    "plains" | "sunflower_plains" | "snowy_plains" => plains.push(height),
                    _ if is_ocean_biome(biome) => oceans.push(height),
                    _ => {}
                }
            }
        }
        assert!(!peaks.is_empty() && !plains.is_empty() && !oceans.is_empty());
        assert!(peaks.iter().min().unwrap() > plains.iter().max().unwrap());
        assert!(plains.iter().min().unwrap() > oceans.iter().max().unwrap());
        assert!(oceans.iter().all(|height| *height < 63));
        assert!(
            biome_terrain_shape(BiomeKind::from_protocol_id(27)).relief
                > biome_terrain_shape(BiomeKind::Plains).relief
        );
    }

    #[test]
    fn surface_plants_are_seeded_supported_and_biome_specific() {
        let mut found = std::collections::HashSet::new();
        for z in -8..=8 {
            for x in -8..=8 {
                let position = ChunkPosition {
                    x: x * 24 + 12,
                    z: z * 24 + 12,
                };
                let chunk = GeneratedChunk::generate(42, position);
                for (position, kind) in &chunk.blocks {
                    if !kind.is_surface_plant() {
                        continue;
                    }
                    found.insert(*kind);
                    assert!(position.y >= 64);
                    assert!(kind.plant_survives_on(chunk.block_at(BlockPosition {
                        y: position.y - 1,
                        ..*position
                    })));
                    match kind {
                        BlockKind::DeadBush => assert!(matches!(
                            chunk.biome.name(),
                            "desert" | "badlands" | "eroded_badlands" | "wooded_badlands"
                        )),
                        BlockKind::Fern => assert!(matches!(
                            chunk.biome.name(),
                            "taiga" | "old_growth_pine_taiga" | "old_growth_spruce_taiga"
                        )),
                        _ => {}
                    }
                }
            }
        }
        assert_eq!(found.len(), 3);
        for position in [
            ChunkPosition { x: -1, z: -1 },
            ChunkPosition { x: 24, z: -24 },
        ] {
            let first = GeneratedChunk::generate(42, position);
            let repeated = GeneratedChunk::generate(42, position);
            assert_eq!(first.blocks, repeated.blocks);
            for (position, kind) in first.blocks {
                if position.x.abs() <= 16 && position.z.abs() <= 16 {
                    assert!(!kind.is_surface_plant());
                }
            }
        }
    }

    #[test]
    fn plants_do_not_block_navigation_or_hold_up_dropped_items() {
        let position = ChunkPosition { x: 0, z: 0 };
        let mut chunk = GeneratedChunk::generate(42, position);
        chunk.surface_y = [64; 256];
        chunk.blocks.clear();
        let plant = BlockPosition { x: 1, y: 65, z: 0 };
        chunk.blocks.insert(plant, BlockKind::Fern);
        let chunks = HashMap::from([(position, chunk)]);
        assert!(can_step(&chunks, (0, 0), (1, 0)));
        assert_eq!(collision_ground_below(&chunks, 1.5, 70.0, 0.5), 65);
    }

    #[test]
    fn nether_caverns_have_continuous_seeded_ceilings_and_clear_interiors() {
        let mut heights = std::collections::HashSet::new();
        for seed in [0, 42, -7919] {
            for position in [
                ChunkPosition { x: -1, z: -1 },
                ChunkPosition { x: 0, z: 0 },
                ChunkPosition { x: 23, z: -24 },
            ] {
                let chunk = GeneratedChunk::generate_in(seed, DimensionKind::Nether, position);
                let repeated = GeneratedChunk::generate_in(seed, DimensionKind::Nether, position);
                assert_eq!(chunk.nether_ceiling, repeated.nether_ceiling);
                for z in position.z * 16..position.z * 16 + 16 {
                    for x in position.x * 16..position.x * 16 + 16 {
                        let ceiling = nether_ceiling_height(seed, x, z);
                        heights.insert(ceiling);
                        assert!((83..=109).contains(&ceiling));
                        assert!(ceiling - chunk.surface_at(x, z) >= 23);
                        assert_eq!(
                            chunk.block_at(BlockPosition { x, y: 75, z }),
                            BlockKind::Air
                        );
                        assert_ne!(
                            chunk.block_at(BlockPosition { x, y: ceiling, z }),
                            BlockKind::Air
                        );
                        assert_eq!(
                            chunk.block_at(BlockPosition { x, y: 127, z }),
                            BlockKind::Bedrock
                        );
                        assert_eq!(
                            chunk.block_at(BlockPosition { x, y: 128, z }),
                            BlockKind::Air
                        );
                        assert!((ceiling - nether_ceiling_height(seed, x + 1, z)).abs() <= 2);
                        assert!((ceiling - nether_ceiling_height(seed, x, z + 1)).abs() <= 2);
                    }
                }
            }
        }
        assert!(heights.len() > 8);
        assert!(
            (-100..100).any(|x| nether_ceiling_height(42, x, 0) != nether_ceiling_height(43, x, 0))
        );
    }

    #[test]
    fn nether_lower_layers_connect_to_bounded_lava_chambers_across_chunks() {
        let seed = 42;
        let (cell_x, cell_z, layer) = (-32..=32)
            .flat_map(|z| (-32..=32).map(move |x| (x, z)))
            .find_map(|(x, z)| nether_layer(seed, x, z).map(|layer| (x, z, layer)))
            .unwrap();
        assert_ne!(
            layer,
            nether_layer(seed + 1, cell_x, cell_z).unwrap_or(NetherLayer {
                center: BlockPosition::default(),
                mouth: BlockPosition::default(),
                radius: 0,
            })
        );
        assert!(layer.center.x.abs().max(layer.center.z.abs()) > 48);
        let mut chunks = HashMap::new();
        let delta = [
            layer.mouth.x - layer.center.x,
            layer.mouth.y - layer.center.y,
            layer.mouth.z - layer.center.z,
        ];
        let steps = delta
            .iter()
            .map(|value| value.abs())
            .max()
            .unwrap_or(0)
            .max(1);
        let mut previous = layer.center;
        for step in 0..=steps {
            let position = BlockPosition {
                x: layer.center.x + (delta[0] * step).div_euclid(steps),
                y: layer.center.y + (delta[1] * step).div_euclid(steps),
                z: layer.center.z + (delta[2] * step).div_euclid(steps),
            };
            assert!((position.x - previous.x).abs() <= 1);
            assert!((position.y - previous.y).abs() <= 1);
            assert!((position.z - previous.z).abs() <= 1);
            previous = position;
            let chunk_position = ChunkPosition {
                x: position.x.div_euclid(16),
                z: position.z.div_euclid(16),
            };
            let chunk = chunks.entry(chunk_position).or_insert_with(|| {
                GeneratedChunk::generate_in(seed, DimensionKind::Nether, chunk_position)
            });
            assert_eq!(chunk.block_at(position), BlockKind::Air);
            assert_eq!(
                chunk.block_at(BlockPosition {
                    y: position.y + 1,
                    ..position
                }),
                BlockKind::Air
            );
        }
        assert!(chunks.len() >= 3);
        let center_chunk = chunks
            .entry(ChunkPosition {
                x: layer.center.x.div_euclid(16),
                z: layer.center.z.div_euclid(16),
            })
            .or_insert_with(|| {
                GeneratedChunk::generate_in(
                    seed,
                    DimensionKind::Nether,
                    ChunkPosition {
                        x: layer.center.x.div_euclid(16),
                        z: layer.center.z.div_euclid(16),
                    },
                )
            });
        assert_eq!(
            center_chunk.block_at(BlockPosition {
                y: layer.center.y - 2,
                ..layer.center
            }),
            BlockKind::Lava
        );
        assert_ne!(
            center_chunk.block_at(BlockPosition {
                y: layer.center.y + layer.radius + 1,
                ..layer.center
            }),
            BlockKind::Lava
        );
        for dimension in [DimensionKind::Overworld, DimensionKind::End] {
            let other = GeneratedChunk::generate_in(seed, dimension, center_chunk.position);
            assert!(!other.blocks.values().any(|kind| *kind == BlockKind::Lava));
        }
    }

    #[test]
    fn nether_floor_decorators_are_seeded_sparse_and_keep_spawn_clear() {
        let mut found_soul_sand = false;
        let mut found_basalt = false;
        for chunk_x in -12..=12 {
            for chunk_z in -12..=12 {
                let position = ChunkPosition {
                    x: chunk_x,
                    z: chunk_z,
                };
                let chunk = GeneratedChunk::generate_in(42, DimensionKind::Nether, position);
                let repeated = GeneratedChunk::generate_in(42, DimensionKind::Nether, position);
                assert_eq!(chunk.blocks, repeated.blocks);
                for (position, kind) in &chunk.blocks {
                    if position.x.abs() <= 20 && position.z.abs() <= 20 {
                        assert!(!matches!(kind, BlockKind::SoulSand | BlockKind::Basalt));
                    }
                    found_soul_sand |= *kind == BlockKind::SoulSand;
                    found_basalt |= *kind == BlockKind::Basalt;
                }
            }
        }
        assert!(found_soul_sand);
        assert!(found_basalt);
    }

    #[test]
    fn nether_roof_placements_match_queries_and_keep_edit_precedence() {
        let mut chunk =
            GeneratedChunk::generate_in(42, DimensionKind::Nether, ChunkPosition { x: -1, z: 0 });
        let hole = BlockPosition {
            x: -8,
            y: 120,
            z: 8,
        };
        chunk.edits.insert(hole, BlockKind::Air);
        let placements: HashMap<_, _> = chunk
            .placements()
            .into_iter()
            .map(|block| (block.position, block.kind))
            .collect();
        assert_eq!(placements.get(&hole), Some(&BlockKind::Air));
        for z in 0..16 {
            for x in -16..0 {
                for y in nether_ceiling_height(42, x, z)..=127 {
                    let position = BlockPosition { x, y, z };
                    assert_eq!(placements.get(&position), Some(&chunk.block_at(position)));
                }
            }
        }
        assert!(
            chunk.blocks.len() < 1024,
            "ceiling must not become stored per-block overrides"
        );
        for dimension in [DimensionKind::Overworld, DimensionKind::End] {
            let other = GeneratedChunk::generate_in(42, dimension, chunk.position);
            assert!(other.nether_ceiling.is_none());
        }
    }

    #[test]
    fn end_islands_have_a_central_landing_void_ring_and_seeded_outer_land() {
        for seed in [0, 42, -7919] {
            assert_eq!(end_island_column(seed, -8, -8).unwrap().1, 64);
            for x in [120, 160, 200, 240, -120, -160, -200, -240] {
                assert!(end_island_column(seed, x, 0).is_none());
                assert!(end_island_column(seed, 0, x).is_none());
            }
            let samples: Vec<_> = (-1024..=1024)
                .step_by(32)
                .flat_map(|z| (-1024..=1024).step_by(32).map(move |x| (x, z)))
                .filter(|(x, z)| x * x + z * z > 320 * 320)
                .collect();
            assert!(samples
                .iter()
                .any(|(x, z)| end_island_column(seed, *x, *z).is_some()));
            assert!(samples
                .iter()
                .any(|(x, z)| end_island_column(seed, *x, *z).is_none()));
            assert!(samples.iter().any(
                |(x, z)| end_island_column(seed, *x, *z) != end_island_column(seed + 1, *x, *z)
            ));
        }
    }

    #[test]
    fn end_chunk_slabs_match_world_queries_and_edits_override_void() {
        for position in [
            ChunkPosition { x: -1, z: -1 },
            ChunkPosition { x: 10, z: 0 },
            ChunkPosition { x: -30, z: 30 },
        ] {
            let mut chunk = GeneratedChunk::generate_in(42, DimensionKind::End, position);
            let repeated = GeneratedChunk::generate_in(42, DimensionKind::End, position);
            assert_eq!(chunk.surface_y, repeated.surface_y);
            assert_eq!(chunk.end_bottom, repeated.end_bottom);
            let placements: HashMap<_, _> = chunk
                .placements()
                .into_iter()
                .map(|block| (block.position, block.kind))
                .collect();
            for z in position.z * 16..position.z * 16 + 16 {
                for x in position.x * 16..position.x * 16 + 16 {
                    for y in -64..=100 {
                        let point = BlockPosition { x, y, z };
                        assert_eq!(
                            chunk.block_at(point),
                            placements.get(&point).copied().unwrap_or(BlockKind::Air)
                        );
                    }
                    assert_eq!(
                        chunk.block_at(BlockPosition { x, y: -64, z }),
                        BlockKind::Air
                    );
                    assert_eq!(
                        chunk.surface_at(x, z),
                        terrain_height(42, DimensionKind::End, x, z)
                    );
                }
            }
            let edit = BlockPosition {
                x: position.x * 16,
                y: 0,
                z: position.z * 16,
            };
            chunk.edits.insert(edit, BlockKind::Obsidian);
            assert_eq!(chunk.block_at(edit), BlockKind::Obsidian);
            assert!(chunk
                .placements()
                .iter()
                .any(|block| block.position == edit && block.kind == BlockKind::Obsidian));
        }
    }

    #[test]
    fn end_void_chunks_have_no_generated_obelisks_or_foundation() {
        for x in [8, 10, -10, -12] {
            let chunk =
                GeneratedChunk::generate_in(42, DimensionKind::End, ChunkPosition { x, z: 0 });
            assert!(chunk.surface_y.iter().all(|height| *height == -65));
            assert!(chunk.placements().is_empty());
        }
    }

    #[test]
    fn end_portal_fields_are_supported_deterministic_and_separated_from_arrivals() {
        for dimension in [DimensionKind::Overworld, DimensionKind::End] {
            let chunk = GeneratedChunk::generate_in(42, dimension, ChunkPosition { x: 0, z: -1 });
            let repeated =
                GeneratedChunk::generate_in(42, dimension, ChunkPosition { x: 0, z: -1 });
            let portals: Vec<_> = chunk
                .blocks
                .iter()
                .filter(|(_, kind)| **kind == BlockKind::EndPortal)
                .collect();
            assert_eq!(portals.len(), 9);
            assert_eq!(chunk.blocks, repeated.blocks);
            for (position, _) in portals {
                assert!((6..=10).contains(&position.x));
                assert!((-10..=-6).contains(&position.z));
                assert_eq!(
                    chunk.block_at(BlockPosition {
                        y: position.y - 1,
                        ..*position
                    }),
                    BlockKind::Obsidian
                );
            }
            assert_ne!(
                chunk.block_at(BlockPosition {
                    x: -8,
                    y: 65,
                    z: -8
                }),
                BlockKind::EndPortal
            );
        }
        let nether =
            GeneratedChunk::generate_in(42, DimensionKind::Nether, ChunkPosition { x: 0, z: -1 });
        assert!(!nether
            .blocks
            .values()
            .any(|kind| *kind == BlockKind::EndPortal));
    }

    #[test]
    fn biome_terrain_blends_both_axes_and_keeps_spawn_flat() {
        for seed in [0, 42, -7919] {
            for boundary in -3..=3 {
                for along in (-512..=512).step_by(17) {
                    for offset in -50..=50 {
                        let x = boundary * 384 + offset;
                        let a = terrain_height(seed, DimensionKind::Overworld, x, along);
                        let b = terrain_height(seed, DimensionKind::Overworld, x + 1, along);
                        let c = terrain_height(seed, DimensionKind::Overworld, along, x);
                        let d = terrain_height(seed, DimensionKind::Overworld, along, x + 1);
                        assert!((a - b).abs() <= 4, "x seam at {seed}/{x}/{along}: {a}->{b}");
                        assert!((c - d).abs() <= 4, "z seam at {seed}/{along}/{x}: {c}->{d}");
                        assert!((32..=144).contains(&a));
                    }
                }
            }
            for x in -16..=0 {
                for z in -16..=0 {
                    assert_eq!(terrain_height(seed, DimensionKind::Overworld, x, z), 64);
                }
            }
        }
    }

    #[test]
    fn water_fills_low_land_columns_and_preserves_edits() {
        let mut chunk = GeneratedChunk::generate(42, ChunkPosition { x: 20, z: 20 });
        // Isolate the elevation rule from region selection and decoration.
        chunk.biome = BiomeKind::Plains;
        chunk.surface_y = [62; 256];
        chunk.surface_y[1] = 63;
        chunk.surface_y[2] = 64;
        chunk.blocks.clear();
        let low = BlockPosition {
            x: 320,
            y: 63,
            z: 320,
        };
        chunk.generate_water();
        assert_eq!(chunk.block_at(low), BlockKind::Water);
        assert_eq!(
            chunk.block_at(BlockPosition { y: 64, ..low }),
            BlockKind::Air
        );
        assert_ne!(
            chunk.block_at(BlockPosition { x: 321, ..low }),
            BlockKind::Water
        );
        assert_ne!(
            chunk.block_at(BlockPosition { x: 322, ..low }),
            BlockKind::Water
        );
        chunk.edits.insert(low, BlockKind::Cobblestone);
        chunk.generate_water();
        assert_eq!(chunk.block_at(low), BlockKind::Cobblestone);
        assert!(chunk
            .placements()
            .iter()
            .any(|block| block.position == low && block.kind == BlockKind::Cobblestone));
    }

    #[test]
    fn blended_surface_matches_independent_chunk_generation() {
        for position in [
            ChunkPosition { x: -25, z: -24 },
            ChunkPosition { x: 23, z: 24 },
            ChunkPosition { x: 0, z: 0 },
        ] {
            let first = GeneratedChunk::generate(42, position);
            let neighbor = GeneratedChunk::generate(
                42,
                ChunkPosition {
                    x: position.x + 1,
                    ..position
                },
            );
            let repeated = GeneratedChunk::generate(42, position);
            assert_eq!(first.surface_y, repeated.surface_y);
            assert_eq!(first.blocks, repeated.blocks);
            for z in position.z * 16..position.z * 16 + 16 {
                let x = position.x * 16 + 15;
                assert_eq!(
                    first.surface_at(x, z),
                    terrain_height(42, DimensionKind::Overworld, x, z)
                );
                assert_eq!(
                    neighbor.surface_at(x + 1, z),
                    terrain_height(42, DimensionKind::Overworld, x + 1, z)
                );
                assert!((first.surface_at(x, z) - neighbor.surface_at(x + 1, z)).abs() <= 4);
            }
        }
    }

    #[test]
    fn overworld_generates_water_caves_and_depth_based_ores() {
        let world = PrototypeWorld::new(42);
        let generated: Vec<_> = world
            .chunks
            .values()
            .flat_map(|chunk| chunk.blocks.values().copied())
            .collect();
        assert!(generated.contains(&BlockKind::Air));
        assert!(generated.contains(&BlockKind::Gravel));
        assert!(generated.iter().any(|kind| matches!(
            kind,
            BlockKind::CoalOre
                | BlockKind::IronOre
                | BlockKind::CopperOre
                | BlockKind::GoldOre
                | BlockKind::RedstoneOre
                | BlockKind::LapisOre
                | BlockKind::DiamondOre
        )));

        // Sample region interiors: an ocean-labeled boundary can now blend up
        // toward land, so not every coastal column is below sea level.
        let ocean = (-8..=8)
            .flat_map(|z| {
                (-8..=8).map(move |x| ChunkPosition {
                    x: x * 24 + 12,
                    z: z * 24 + 12,
                })
            })
            .find_map(|position| {
                let chunk = GeneratedChunk::generate(42, position);
                is_ocean_biome(chunk.biome).then_some(chunk)
            })
            .expect("the biome catalog must produce an ocean region");
        let x = ocean.position.x * CHUNK_WIDTH;
        let z = ocean.position.z * CHUNK_WIDTH;
        assert!(ocean.surface_at(x, z) < 63);
        assert_eq!(
            ocean.block_at(BlockPosition { x, y: 63, z }),
            BlockKind::Water
        );
    }

    #[test]
    fn every_synced_26_2_biome_is_assigned_to_a_dimension() {
        let mut ids = std::collections::HashSet::new();
        for dimension in [
            DimensionKind::Overworld,
            DimensionKind::Nether,
            DimensionKind::End,
        ] {
            for region_z in -128..=128 {
                for region_x in -128..=128 {
                    ids.insert(
                        biome_at(42, dimension, region_x * 384, region_z * 384).protocol_id(),
                    );
                }
            }
        }
        assert_eq!(ids.len(), BiomeKind::COUNT);
        assert!((0..BiomeKind::COUNT as i32).all(|id| ids.contains(&id)));
    }

    #[test]
    fn nether_and_end_use_native_terrain_blocks() {
        let nether = PrototypeWorld::new_dimension(42, DimensionKind::Nether);
        let end = PrototypeWorld::new_dimension(42, DimensionKind::End);
        assert!(nether.chunks.values().all(|chunk| matches!(
            chunk.terrain.foundation,
            BlockKind::Netherrack | BlockKind::Basalt
        )));
        assert!(end
            .chunks
            .values()
            .all(|chunk| chunk.terrain.foundation == BlockKind::EndStone));
    }

    #[test]
    fn each_dimension_places_its_deterministic_structure_family() {
        for dimension in [
            DimensionKind::Overworld,
            DimensionKind::Nether,
            DimensionKind::End,
        ] {
            let (cell_x, cell_z, value) = (-32..=32)
                .flat_map(|z| (-32..=32).map(move |x| (x, z)))
                .map(|(x, z)| {
                    (
                        x,
                        z,
                        mix(42_u64 ^ 0x57a1_c7e5 ^ dimension.type_id() as u64, x, z),
                    )
                })
                .find(|(x, z, value)| {
                    let anchor_x = x * 96 + 24 + ((value >> 8) % 48) as i32;
                    let anchor_z = z * 96 + 24 + ((value >> 16) % 48) as i32;
                    value % 4 == 0
                        && (dimension != DimensionKind::End
                            || (-3..=3).all(|dz| {
                                (-3..=3).all(|dx| {
                                    end_island_column(42, anchor_x + dx, anchor_z + dz).is_some()
                                })
                            }))
                        && (x * 96 + 24 + i32::try_from((value >> 8) % 48).unwrap_or(0)).abs() >= 20
                        && (z * 96 + 24 + i32::try_from((value >> 16) % 48).unwrap_or(0)).abs()
                            >= 20
                })
                .unwrap();
            let x = cell_x * 96 + 24 + i32::try_from((value >> 8) % 48).unwrap_or(0);
            let z = cell_z * 96 + 24 + i32::try_from((value >> 16) % 48).unwrap_or(0);
            let ground = terrain_height(42, dimension, x, z);
            let chunk = GeneratedChunk::generate_in(
                42,
                dimension,
                ChunkPosition {
                    x: x.div_euclid(16),
                    z: z.div_euclid(16),
                },
            );
            let (position, expected) = match dimension {
                DimensionKind::Overworld => {
                    let profile =
                        terrain_profile(dimension, biome_at(42, DimensionKind::Overworld, x, z));
                    let expected = if profile.surface == BlockKind::Sand {
                        BlockKind::Sandstone
                    } else if profile.surface == BlockKind::SnowBlock {
                        BlockKind::StoneBricks
                    } else {
                        BlockKind::Cobblestone
                    };
                    (
                        BlockPosition {
                            x,
                            y: ground + 1,
                            z,
                        },
                        expected,
                    )
                }
                DimensionKind::Nether if (value >> 24) & 1 == 0 => (
                    BlockPosition {
                        x,
                        y: ground + 2,
                        z,
                    },
                    BlockKind::NetherPortal,
                ),
                DimensionKind::Nether => (
                    BlockPosition {
                        x,
                        y: ground + 1,
                        z,
                    },
                    BlockKind::SoulSand,
                ),
                DimensionKind::End if (value >> 24) & 1 == 0 => (
                    BlockPosition {
                        x,
                        y: ground + 1,
                        z,
                    },
                    BlockKind::Obsidian,
                ),
                DimensionKind::End => (
                    BlockPosition {
                        x,
                        y: ground + 5,
                        z,
                    },
                    BlockKind::EndStone,
                ),
            };
            assert_eq!(chunk.block_at(position), expected);
        }
    }

    #[test]
    fn overworld_trail_lookouts_are_deterministic_and_reconstruct_across_chunks() {
        let seed = 42;
        let (cell_x, cell_z, value) = (-32..=32)
            .flat_map(|z| (-32..=32).map(move |x| (x, z)))
            .map(|(x, z)| (x, z, mix(seed as u64 ^ 0x57a1_c7e5, x, z)))
            .find(|(cell_x, cell_z, value)| {
                let x = cell_x * 96 + 24 + i32::try_from((value >> 8) % 48).unwrap_or(0);
                let z = cell_z * 96 + 24 + i32::try_from((value >> 16) % 48).unwrap_or(0);
                value % 4 == 0 && (value >> 24) & 1 == 1 && !(x.abs() < 20 && z.abs() < 20)
            })
            .unwrap();
        let x = cell_x * 96 + 24 + i32::try_from((value >> 8) % 48).unwrap_or(0);
        let z = cell_z * 96 + 24 + i32::try_from((value >> 16) % 48).unwrap_or(0);
        let ground = terrain_height(seed, DimensionKind::Overworld, x, z);
        let mut generated = HashMap::new();
        for chunk_z in (z - 3).div_euclid(16)..=(z + 3).div_euclid(16) {
            for chunk_x in (x - 3).div_euclid(16)..=(x + 3).div_euclid(16) {
                generated.extend(
                    GeneratedChunk::generate_in(
                        seed,
                        DimensionKind::Overworld,
                        ChunkPosition {
                            x: chunk_x,
                            z: chunk_z,
                        },
                    )
                    .blocks,
                );
            }
        }
        for (dx, dz) in [(-2, -2), (-2, 2), (2, -2), (2, 2)] {
            for y in ground + 2..=ground + 6 {
                assert_eq!(
                    generated.get(&BlockPosition {
                        x: x + dx,
                        y,
                        z: z + dz,
                    }),
                    Some(&BlockKind::OakLog)
                );
            }
        }
        assert_eq!(
            generated.get(&BlockPosition {
                x,
                y: ground + 7,
                z,
            }),
            Some(&BlockKind::OakPlanks)
        );
        let repeated = GeneratedChunk::generate_in(
            seed,
            DimensionKind::Overworld,
            ChunkPosition {
                x: x.div_euclid(16),
                z: z.div_euclid(16),
            },
        );
        let original = GeneratedChunk::generate_in(
            seed,
            DimensionKind::Overworld,
            ChunkPosition {
                x: x.div_euclid(16),
                z: z.div_euclid(16),
            },
        );
        assert_eq!(original.blocks, repeated.blocks);
    }

    #[test]
    fn overworld_structure_materials_follow_dry_and_snowy_biomes() {
        let seed = 42;
        for (surface, expected) in [
            (BlockKind::Sand, BlockKind::Sandstone),
            (BlockKind::SnowBlock, BlockKind::SpruceLog),
        ] {
            let (x, z, value) = (-96..=96)
                .flat_map(|cell_z| (-96..=96).map(move |cell_x| (cell_x, cell_z)))
                .filter_map(|(cell_x, cell_z)| {
                    let value = mix(seed as u64 ^ 0x57a1_c7e5, cell_x, cell_z);
                    let x = cell_x * 96 + 24 + i32::try_from((value >> 8) % 48).ok()?;
                    let z = cell_z * 96 + 24 + i32::try_from((value >> 16) % 48).ok()?;
                    let profile = terrain_profile(
                        DimensionKind::Overworld,
                        biome_at(seed, DimensionKind::Overworld, x, z),
                    );
                    (value % 4 == 0
                        && !(x.abs() < 20 && z.abs() < 20)
                        && profile.surface == surface)
                        .then_some((x, z, value))
                })
                .next()
                .unwrap();
            let ground = terrain_height(seed, DimensionKind::Overworld, x, z);
            let position = if surface == BlockKind::Sand {
                BlockPosition {
                    x,
                    y: ground + 1,
                    z,
                }
            } else if (value >> 24) & 1 == 0 {
                BlockPosition {
                    x: x + 3,
                    y: ground + 2,
                    z,
                }
            } else {
                BlockPosition {
                    x: x + 2,
                    y: ground + 2,
                    z: z + 2,
                }
            };
            let chunk = GeneratedChunk::generate_in(
                seed,
                DimensionKind::Overworld,
                ChunkPosition {
                    x: position.x.div_euclid(16),
                    z: position.z.div_euclid(16),
                },
            );
            assert_eq!(chunk.block_at(position), expected);
        }
    }

    #[test]
    fn alternate_nether_and_end_landmarks_are_seeded_and_supported() {
        let seed = 42;
        for dimension in [DimensionKind::Nether, DimensionKind::End] {
            let (x, z) = (-64..=64)
                .flat_map(|cell_z| (-64..=64).map(move |cell_x| (cell_x, cell_z)))
                .find_map(|(cell_x, cell_z)| {
                    let value = mix(
                        seed as u64 ^ 0x57a1_c7e5 ^ dimension.type_id() as u64,
                        cell_x,
                        cell_z,
                    );
                    let x = cell_x * 96 + 24 + i32::try_from((value >> 8) % 48).ok()?;
                    let z = cell_z * 96 + 24 + i32::try_from((value >> 16) % 48).ok()?;
                    (value % 4 == 0
                        && (value >> 24) & 1 == 1
                        && !(x.abs() < 20 && z.abs() < 20)
                        && (dimension != DimensionKind::End
                            || (-3..=3).all(|dz| {
                                (-3..=3).all(|dx| end_island_column(seed, x + dx, z + dz).is_some())
                            })))
                    .then_some((x, z))
                })
                .unwrap();
            let ground = terrain_height(seed, dimension, x, z);
            let checks = if dimension == DimensionKind::Nether {
                [
                    (BlockPosition { x, y: ground, z }, BlockKind::Basalt),
                    (
                        BlockPosition {
                            x,
                            y: ground + 1,
                            z,
                        },
                        BlockKind::SoulSand,
                    ),
                    (
                        BlockPosition {
                            x,
                            y: ground + 6,
                            z,
                        },
                        BlockKind::Obsidian,
                    ),
                ]
            } else {
                [
                    (BlockPosition { x, y: ground, z }, BlockKind::EndStone),
                    (
                        BlockPosition {
                            x: x - 3,
                            y: ground + 3,
                            z,
                        },
                        BlockKind::Obsidian,
                    ),
                    (
                        BlockPosition {
                            x,
                            y: ground + 5,
                            z,
                        },
                        BlockKind::EndStone,
                    ),
                ]
            };
            for (position, expected) in checks {
                let chunk = GeneratedChunk::generate_in(
                    seed,
                    dimension,
                    ChunkPosition {
                        x: position.x.div_euclid(16),
                        z: position.z.div_euclid(16),
                    },
                );
                assert_eq!(chunk.block_at(position), expected);
            }
        }
    }

    #[test]
    fn generated_structure_metadata_drives_one_time_seeded_loot() {
        let seed = 42;
        let (x, z, value) = (-32..=32)
            .flat_map(|cell_z| (-32..=32).map(move |cell_x| (cell_x, cell_z)))
            .find_map(|(cell_x, cell_z)| {
                let value = mix(seed as u64 ^ 0x57a1_c7e5, cell_x, cell_z);
                let x = cell_x * 96 + 24 + i32::try_from((value >> 8) % 48).ok()?;
                let z = cell_z * 96 + 24 + i32::try_from((value >> 16) % 48).ok()?;
                (value % 4 == 0 && !(x.abs() < 20 && z.abs() < 20)).then_some((x, z, value))
            })
            .unwrap();
        let ground = terrain_height(seed, DimensionKind::Overworld, x, z);
        let chest_position = if (value >> 24) & 1 == 0 {
            BlockPosition {
                x: x + 2,
                y: ground + 2,
                z: z + 2,
            }
        } else {
            BlockPosition {
                x,
                y: ground + 2,
                z: z + 1,
            }
        };
        let mut world = PrototypeWorld::new(seed);
        world.ensure_chunk_surface(ChunkPosition {
            x: chest_position.x.div_euclid(16),
            z: chest_position.z.div_euclid(16),
        });
        let metadata = world.generated_structure_at(chest_position).unwrap();
        assert_eq!(metadata.anchor, BlockPosition { x, y: ground, z });
        assert_eq!(metadata.loot_chest, chest_position);
        assert_eq!(world.block_at(chest_position), BlockKind::Chest);
        let loot = world.generated_chest_loot(chest_position).unwrap();
        assert_eq!(loot.slots.iter().flatten().count(), 3);
        assert_eq!(world.generated_chest_loot(chest_position), Some(loot));
        assert!(world.set_block(chest_position, BlockKind::Air));
        assert_eq!(world.generated_chest_loot(chest_position), None);
        assert!(world.set_block(chest_position, BlockKind::Chest));
        assert_eq!(world.generated_chest_loot(chest_position), None);
    }

    #[test]
    fn block_edits_override_generated_terrain() {
        let mut world = PrototypeWorld::new(42);
        let position = BlockPosition {
            x: -8,
            y: 64,
            z: -8,
        };
        assert_eq!(world.block_at(position), BlockKind::Grass);
        assert!(world.set_block(position, BlockKind::Air));
        assert_eq!(world.block_at(position), BlockKind::Air);
        assert!(!world.set_block(position, BlockKind::Air));
    }

    #[test]
    fn zombies_chase_a_nearby_player() {
        let mut world = PrototypeWorld::new(7);
        let zombie_before = world
            .mobs()
            .into_iter()
            .find(|mob| mob.kind == MobKind::Zombie)
            .unwrap();
        let players = [PlayerSnapshot {
            id: Uuid::nil(),
            name: "Tester".into(),
            world: "world".into(),
            position: BlockPosition {
                x: -8,
                y: 65,
                z: -8,
            },
            game_mode: GameMode::Survival,
        }];
        world.tick(1, &players);
        let zombie_after = world
            .mobs()
            .into_iter()
            .find(|mob| mob.kind == MobKind::Zombie)
            .unwrap();
        assert_eq!(zombie_after.ai_state, MobAiState::Chasing);
        assert_ne!(zombie_before.position, zombie_after.position);
    }

    #[test]
    fn wandering_mobs_remain_on_generated_ground() {
        let mut world = PrototypeWorld::new(99);
        for tick in 1..2_000 {
            world.tick(tick, &[]);
        }
        for mob in world.mobs() {
            let ground = world.block_at(BlockPosition {
                x: mob.position.x.floor() as i32,
                y: mob.position.y.floor() as i32 - 1,
                z: mob.position.z.floor() as i32,
            });
            assert_ne!(ground, BlockKind::Air);
        }
    }

    #[test]
    fn daylight_burns_and_eventually_removes_zombies_when_players_are_present() {
        let mut world = PrototypeWorld::new(7);
        let players = [PlayerSnapshot {
            id: Uuid::nil(),
            name: "Tester".into(),
            world: "world".into(),
            position: BlockPosition {
                x: -8,
                y: 65,
                z: -8,
            },
            game_mode: GameMode::Survival,
        }];
        world.tick(20, &players);
        let zombie = world
            .mobs()
            .into_iter()
            .find(|mob| mob.kind == MobKind::Zombie)
            .unwrap();
        assert!(zombie.on_fire);
        assert_eq!(zombie.health, 19.0);
        for tick in 21..=400 {
            world.tick(tick, &players);
        }
        assert!(world.mobs().iter().all(|mob| mob.kind != MobKind::Zombie));
    }

    #[test]
    fn a_star_routes_around_an_impassable_ridge() {
        let mut world = PrototypeWorld::new(17);
        for chunk in world.chunks.values_mut() {
            chunk.surface_y.fill(64);
        }
        for z in -3_i32..=3 {
            let chunk_position = ChunkPosition {
                x: 0,
                z: z.div_euclid(CHUNK_WIDTH),
            };
            let chunk = world.chunks.get_mut(&chunk_position).unwrap();
            let index = usize::try_from(z.rem_euclid(CHUNK_WIDTH) * CHUNK_WIDTH).unwrap();
            chunk.surface_y[index] = 70;
        }
        let path = find_path(&world.chunks, (-3, 0), (3, 0), 4_096).unwrap();
        assert_eq!(path.back(), Some(&(3, 0)));
        assert!(path.iter().all(|(x, z)| *x != 0 || z.abs() > 3));
        assert!(path.len() > 6);
    }

    #[test]
    fn dropped_items_keep_their_exact_stack_until_pickup() {
        let mut world = PrototypeWorld::new(42);
        let position = BlockPosition {
            x: -8,
            y: 65,
            z: -8,
        };
        let player = PlayerSnapshot {
            id: Uuid::new_v4(),
            name: "Collector".into(),
            world: "world".into(),
            position,
            game_mode: GameMode::Survival,
        };
        let stack = ItemStack {
            kind: ItemKind::OakLog,
            count: 3,
            damage: 0,
        };
        world.drop_item(stack, position);
        assert!(world
            .collect_pickups(std::slice::from_ref(&player))
            .is_empty());
        for tick in 1..=10 {
            world.tick(tick, std::slice::from_ref(&player));
        }
        let player_id = player.id;
        assert_eq!(world.collect_pickups(&[player]), vec![(player_id, stack)]);
        assert!(world.items().is_empty());
    }

    #[test]
    fn dropped_items_collide_with_placed_blocks() {
        let mut world = PrototypeWorld::new(42);
        let support = BlockPosition {
            x: -8,
            y: 66,
            z: -8,
        };
        assert!(world.set_block(support, BlockKind::StoneBricks));
        world.drop_item(
            ItemStack {
                kind: ItemKind::Cobblestone,
                count: 1,
                damage: 0,
            },
            BlockPosition { y: 72, ..support },
        );
        for tick in 1..=80 {
            world.tick(tick, &[]);
        }
        assert_eq!(world.items()[0].position.y, 67.0);
    }

    #[test]
    fn inactive_unedited_chunks_are_evicted_but_edits_are_retained() {
        let mut world = PrototypeWorld::new(42);
        let original_count = world.chunks.len();
        let edited = BlockPosition {
            x: -9 * CHUNK_WIDTH,
            y: 80,
            z: -9 * CHUNK_WIDTH,
        };
        assert!(world.set_block(edited, BlockKind::StoneBricks));
        world.tick(200, &[]);
        assert!(world.chunks.len() < original_count);
        assert_eq!(world.block_at(edited), BlockKind::StoneBricks);
    }
}
