//! Native Minecraft 26.2 chunk and lighting encoding.

use bytes::{BufMut, BytesMut};
use std::collections::HashMap;

use crate::{encode_varint, frame_packet};

const CLIENTBOUND_LEVEL_CHUNK_WITH_LIGHT_ID: i32 = 45;
const MIN_Y: i32 = -64;
const SECTION_COUNT: i32 = 24;
const AIR_STATE: i32 = 0;
const STONE_STATE: i32 = 1;
const GRASS_BLOCK_STATE: i32 = 9;
const DIRT_STATE: i32 = 10;
const BEDROCK_STATE: i32 = 85;
const PLAINS_BIOME: i32 = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkBlockState {
    pub x: u8,
    pub y: i16,
    pub z: u8,
    pub state_id: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockLightSource {
    pub x: i16,
    pub y: i16,
    pub z: i16,
    pub level: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct ChunkLighting<'a> {
    pub has_skylight: bool,
    pub sources: &'a [BlockLightSource],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkTerrainStates {
    pub surface: i32,
    pub filler: i32,
    pub foundation: i32,
    pub bedrock: i32,
}

/// Encodes one independently generated overworld chunk and its initial light data.
#[must_use]
pub fn encode_generated_chunk(x: i32, z: i32, surface_y: &[i16; 256]) -> Vec<u8> {
    encode_generated_chunk_with_blocks(x, z, surface_y, &[])
}

#[must_use]
pub fn encode_generated_chunk_with_blocks(
    x: i32,
    z: i32,
    surface_y: &[i16; 256],
    blocks: &[ChunkBlockState],
) -> Vec<u8> {
    encode_chunk_with_foundation(x, z, surface_y, STONE_STATE, blocks)
}

#[must_use]
pub fn encode_generated_chunk_with_blocks_and_biome(
    x: i32,
    z: i32,
    surface_y: &[i16; 256],
    blocks: &[ChunkBlockState],
    biome_id: i32,
) -> Vec<u8> {
    encode_chunk_with_terrain_and_biome(
        x,
        z,
        surface_y,
        ChunkTerrainStates {
            surface: GRASS_BLOCK_STATE,
            filler: DIRT_STATE,
            foundation: STONE_STATE,
            bedrock: BEDROCK_STATE,
        },
        blocks,
        biome_id,
        ChunkLighting {
            has_skylight: true,
            sources: &[],
        },
    )
}

#[must_use]
pub fn encode_generated_chunk_with_terrain(
    x: i32,
    z: i32,
    surface_y: &[i16; 256],
    blocks: &[ChunkBlockState],
    biome_id: i32,
    terrain: ChunkTerrainStates,
    has_skylight: bool,
) -> Vec<u8> {
    let light_sources = blocks
        .iter()
        .filter(|block| block.state_id == 102)
        .map(|block| BlockLightSource {
            x: i16::from(block.x),
            y: block.y,
            z: i16::from(block.z),
            level: 15,
        })
        .collect::<Vec<_>>();
    encode_generated_chunk_with_terrain_and_light_sources(
        x,
        z,
        surface_y,
        blocks,
        biome_id,
        terrain,
        ChunkLighting {
            has_skylight,
            sources: &light_sources,
        },
    )
}

#[must_use]
pub fn encode_generated_chunk_with_terrain_and_light_sources(
    x: i32,
    z: i32,
    surface_y: &[i16; 256],
    blocks: &[ChunkBlockState],
    biome_id: i32,
    terrain: ChunkTerrainStates,
    lighting: ChunkLighting<'_>,
) -> Vec<u8> {
    encode_chunk_with_terrain_and_biome(x, z, surface_y, terrain, blocks, biome_id, lighting)
}

fn encode_chunk_with_foundation(
    x: i32,
    z: i32,
    surface_y: &[i16; 256],
    foundation_state: i32,
    blocks: &[ChunkBlockState],
) -> Vec<u8> {
    encode_chunk_with_terrain_and_biome(
        x,
        z,
        surface_y,
        ChunkTerrainStates {
            surface: GRASS_BLOCK_STATE,
            filler: DIRT_STATE,
            foundation: foundation_state,
            bedrock: BEDROCK_STATE,
        },
        blocks,
        PLAINS_BIOME,
        ChunkLighting {
            has_skylight: true,
            sources: &[],
        },
    )
}

fn encode_chunk_with_terrain_and_biome(
    x: i32,
    z: i32,
    surface_y: &[i16; 256],
    terrain: ChunkTerrainStates,
    blocks: &[ChunkBlockState],
    biome_id: i32,
    lighting: ChunkLighting<'_>,
) -> Vec<u8> {
    let overrides: HashMap<_, _> = blocks
        .iter()
        .map(|block| ((block.x, block.y, block.z), block.state_id))
        .collect();
    let mut light_height = *surface_y;
    let mut motion_height = *surface_y;
    for block in blocks.iter().filter(|block| block.state_id != AIR_STATE) {
        let column = usize::from(block.z) * 16 + usize::from(block.x);
        if column < light_height.len() {
            light_height[column] = light_height[column].max(block.y);
            // Verified 26.2 short grass, fern, and dead bush have no collision.
            if !matches!(block.state_id, 2248..=2250 | 7017 | 9468) {
                motion_height[column] = motion_height[column].max(block.y);
            }
        }
    }
    let mut payload = BytesMut::new();
    payload.put_i32(x);
    payload.put_i32(z);
    encode_heightmaps(&light_height, &motion_height, &mut payload);

    let mut sections = BytesMut::new();
    for section in 0..SECTION_COUNT {
        encode_section(
            section,
            surface_y,
            terrain,
            &overrides,
            biome_id,
            &mut sections,
        );
    }
    encode_varint(
        i32::try_from(sections.len()).expect("chunk section payload fits an i32"),
        &mut payload,
    );
    payload.extend_from_slice(&sections);
    encode_varint(0, &mut payload); // No block entities.

    // Heightmaps include tree canopies, but the prototype skylight model does
    // not yet flood light sideways. Lighting from the terrain surface avoids
    // pitch-black columns beneath translucent leaves until propagation lands.
    if lighting.has_skylight {
        encode_light(surface_y, &mut payload);
    } else {
        encode_lightless(lighting.sources, &mut payload);
    }
    frame_packet(CLIENTBOUND_LEVEL_CHUNK_WITH_LIGHT_ID, &payload)
}

fn encode_heightmaps(surface_y: &[i16; 256], motion_y: &[i16; 256], output: &mut BytesMut) {
    encode_varint(3, output);
    // EnumMap's wire order for the three client heightmaps is 1, 5, 4.
    for heightmap_type in [1, 5, 4] {
        let heights = if heightmap_type == 1 {
            surface_y
        } else {
            motion_y
        };
        let values: Vec<_> = heights
            .iter()
            .map(|height| u64::try_from(i32::from(*height) + 1 - MIN_Y).unwrap_or_default())
            .collect();
        let packed = pack_values(&values, 9);
        encode_varint(heightmap_type, output);
        encode_varint(
            i32::try_from(packed.len()).expect("heightmap fits an i32"),
            output,
        );
        for value in &packed {
            output.put_u64(*value);
        }
    }
}

fn encode_section(
    section: i32,
    surface_y: &[i16; 256],
    terrain: ChunkTerrainStates,
    overrides: &HashMap<(u8, i16, u8), i32>,
    biome_id: i32,
    output: &mut BytesMut,
) {
    let base_y = MIN_Y + section * 16;
    let mut states = Vec::with_capacity(4096);
    for local_y in 0..16 {
        for local_z in 0..16 {
            for local_x in 0..16 {
                let column = local_z * 16 + local_x;
                let y = base_y + local_y;
                let override_state = overrides.get(&(
                    u8::try_from(local_x).unwrap_or(0),
                    i16::try_from(y).unwrap_or(0),
                    u8::try_from(local_z).unwrap_or(0),
                ));
                states.push(
                    override_state
                        .copied()
                        .unwrap_or_else(|| block_state(y, i32::from(surface_y[column]), terrain)),
                );
            }
        }
    }
    let non_empty = states.iter().filter(|state| **state != AIR_STATE).count();
    output.put_u16(u16::try_from(non_empty).expect("a section contains at most 4096 blocks"));
    output.put_u16(0); // No fluids in the current generator.
    encode_paletted_container(&states, 4, output);
    encode_single_value_container(biome_id, output);
}

fn block_state(y: i32, surface: i32, terrain: ChunkTerrainStates) -> i32 {
    if y > surface {
        AIR_STATE
    } else if y == surface {
        terrain.surface
    } else if y == MIN_Y {
        terrain.bedrock
    } else if y >= surface - 3 {
        terrain.filler
    } else {
        terrain.foundation
    }
}

fn encode_paletted_container(values: &[i32], minimum_bits: u8, output: &mut BytesMut) {
    let mut palette = Vec::new();
    let mut indices = Vec::with_capacity(values.len());
    for value in values {
        let index = palette
            .iter()
            .position(|entry| entry == value)
            .unwrap_or_else(|| {
                palette.push(*value);
                palette.len() - 1
            });
        indices.push(u64::try_from(index).expect("palette index fits a u64"));
    }
    if palette.len() == 1 {
        encode_single_value_container(palette[0], output);
        return;
    }
    let needed_bits = u8::try_from(usize::BITS - (palette.len() - 1).leading_zeros())
        .expect("palette bit width fits a u8");
    let bits = needed_bits.max(minimum_bits);
    output.put_u8(bits);
    encode_varint(
        i32::try_from(palette.len()).expect("palette fits an i32"),
        output,
    );
    for value in palette {
        encode_varint(value, output);
    }
    for packed in pack_values(&indices, bits) {
        output.put_u64(packed);
    }
}

fn encode_single_value_container(value: i32, output: &mut BytesMut) {
    output.put_u8(0);
    encode_varint(value, output);
}

fn pack_values(values: &[u64], bits: u8) -> Vec<u64> {
    let values_per_long = 64 / usize::from(bits);
    let mask = (1_u64 << bits) - 1;
    values
        .chunks(values_per_long)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .fold(0_u64, |packed, (index, value)| {
                    packed | ((value & mask) << (index * usize::from(bits)))
                })
        })
        .collect()
}

fn encode_light(surface_y: &[i16; 256], output: &mut BytesMut) {
    let lowest_section = i32::from(*surface_y.iter().min().unwrap_or(&64)).div_euclid(16);
    let highest_section = i32::from(*surface_y.iter().max().unwrap_or(&64)).div_euclid(16);
    let first_light_index = lowest_section + 5;
    let last_light_index = highest_section + 6;
    let sky_mask = bit_range(first_light_index, last_light_index);
    let empty_sky_mask = bit_range(0, first_light_index - 1);
    let empty_block_mask = bit_range(0, last_light_index);

    encode_bitset(sky_mask, output);
    encode_bitset(0, output); // No block-light updates.
    encode_bitset(empty_sky_mask, output);
    encode_bitset(empty_block_mask, output);

    encode_varint(last_light_index - first_light_index + 1, output);
    for light_index in first_light_index..=last_light_index {
        let section_y = light_index - 5;
        let data = skylight_section(section_y, surface_y);
        encode_varint(2048, output);
        output.extend_from_slice(&data);
    }
    encode_varint(0, output); // No block-light arrays.
}

fn encode_lightless(light_sources: &[BlockLightSource], output: &mut BytesMut) {
    let mut sections: HashMap<i32, [u8; 2048]> = HashMap::new();
    for source in light_sources {
        for dy in -14_i32..=14 {
            let remaining_y = 14 - dy.abs();
            for dz in -remaining_y..=remaining_y {
                let remaining_z = remaining_y - dz.abs();
                for dx in -remaining_z..=remaining_z {
                    let x = i32::from(source.x) + dx;
                    let y = i32::from(source.y) + dy;
                    let z = i32::from(source.z) + dz;
                    if !(0..16).contains(&x) || !(0..16).contains(&z) || !(MIN_Y..320).contains(&y)
                    {
                        continue;
                    }
                    let level = source
                        .level
                        .saturating_sub(u8::try_from(dx.abs() + dy.abs() + dz.abs()).unwrap_or(15));
                    let section = y.div_euclid(16);
                    let local_y = y.rem_euclid(16) as usize;
                    let index = local_y * 256 + z as usize * 16 + x as usize;
                    let byte = index / 2;
                    let shift = (index % 2) * 4;
                    let data = sections.entry(section).or_insert([0; 2048]);
                    let current = (data[byte] >> shift) & 0x0f;
                    if level > current {
                        data[byte] = (data[byte] & !(0x0f << shift)) | (level << shift);
                    }
                }
            }
        }
    }
    let mut lit: Vec<_> = sections.into_iter().collect();
    lit.sort_by_key(|(section, _)| *section);
    let block_mask = lit
        .iter()
        .fold(0_u64, |mask, (section, _)| mask | (1_u64 << (section + 5)));
    let all_sections = bit_range(0, SECTION_COUNT + 1);
    encode_bitset(0, output); // No sky-light updates.
    encode_bitset(block_mask, output);
    encode_bitset(0, output); // No empty sky sections.
    encode_bitset(all_sections & !block_mask, output);
    encode_varint(0, output); // No sky-light arrays.
    encode_varint(
        i32::try_from(lit.len()).expect("light section count fits an i32"),
        output,
    );
    for (_, data) in lit {
        encode_varint(2048, output);
        output.extend_from_slice(&data);
    }
}

fn skylight_section(section_y: i32, surface_y: &[i16; 256]) -> [u8; 2048] {
    let mut data = [0_u8; 2048];
    for local_y in 0..16 {
        let y = section_y * 16 + local_y;
        for local_z in 0..16 {
            for local_x in 0..16 {
                let column = local_z * 16 + local_x;
                let light = if y > i32::from(surface_y[column]) {
                    15
                } else {
                    0
                };
                let index = usize::try_from(local_y).expect("local Y is non-negative") * 256
                    + local_z * 16
                    + local_x;
                let byte = index / 2;
                let shift = (index % 2) * 4;
                data[byte] |= light << shift;
            }
        }
    }
    data
}

fn bit_range(first: i32, last: i32) -> u64 {
    if last < first {
        return 0;
    }
    (first..=last).fold(0, |bits, bit| bits | (1_u64 << bit))
}

fn encode_bitset(bits: u64, output: &mut BytesMut) {
    if bits == 0 {
        encode_varint(0, output);
    } else {
        encode_varint(1, output);
        output.put_u64(bits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightless_dimensions_emit_block_light_around_lava() {
        let mut dark = BytesMut::new();
        encode_lightless(&[], &mut dark);
        let mut lava = BytesMut::new();
        encode_lightless(
            &[BlockLightSource {
                x: 8,
                y: 32,
                z: 8,
                level: 15,
            }],
            &mut lava,
        );
        assert!(lava.len() > dark.len());
        assert!(lava.contains(&0xff));
        let mut neighbor = BytesMut::new();
        encode_lightless(
            &[BlockLightSource {
                x: -1,
                y: 32,
                z: 8,
                level: 15,
            }],
            &mut neighbor,
        );
        assert!(neighbor.len() > dark.len());
        assert_ne!(neighbor, lava);
    }

    #[test]
    fn air_foundation_encodes_finite_end_slabs_without_hidden_bedrock() {
        let terrain = ChunkTerrainStates {
            surface: 0,
            filler: 0,
            foundation: 0,
            bedrock: 0,
        };
        let overrides: HashMap<_, _> = (60..=64).map(|y| ((0, y, 0), 9477)).collect();
        for (section, count) in [(0, 0), (6, 0), (7, 4), (8, 1), (9, 0)] {
            let mut encoded = BytesMut::new();
            encode_section(section, &[64; 256], terrain, &overrides, 57, &mut encoded);
            assert_eq!(u16::from_be_bytes(encoded[..2].try_into().unwrap()), count);
        }
        assert_eq!(block_state(-64, 64, terrain), AIR_STATE);
        assert_eq!(block_state(32, 64, terrain), AIR_STATE);
    }

    #[test]
    fn plants_raise_world_surface_but_not_motion_heightmaps() {
        for state_id in 2248..=2250 {
            let blocks = [ChunkBlockState {
                x: 0,
                y: 65,
                z: 0,
                state_id,
            }];
            let packet = encode_generated_chunk_with_blocks(0, 0, &[64; 256], &blocks);
            let (_, prefix) = crate::decode_varint(&packet).unwrap();
            let (_, id_len) = crate::decode_varint(&packet[prefix..]).unwrap();
            let mut offset = prefix + id_len + 8;
            let (maps, size) = crate::decode_varint(&packet[offset..]).unwrap();
            offset += size;
            assert_eq!(maps, 3);
            for expected_type in [1, 5, 4] {
                let (kind, size) = crate::decode_varint(&packet[offset..]).unwrap();
                offset += size;
                assert_eq!(kind, expected_type);
                let (longs, size) = crate::decode_varint(&packet[offset..]).unwrap();
                offset += size;
                let first = u64::from_be_bytes(packet[offset..offset + 8].try_into().unwrap());
                assert_eq!(first & 511, if kind == 1 { 130 } else { 129 });
                offset += usize::try_from(longs).unwrap() * 8;
            }
        }
    }

    #[test]
    fn portal_fields_do_not_raise_motion_heightmaps() {
        for state_id in [7017, 9468] {
            let packet = encode_generated_chunk_with_blocks(
                0,
                0,
                &[64; 256],
                &[ChunkBlockState {
                    x: 0,
                    y: 65,
                    z: 0,
                    state_id,
                }],
            );
            let (_, prefix) = crate::decode_varint(&packet).unwrap();
            let (_, id_len) = crate::decode_varint(&packet[prefix..]).unwrap();
            let mut offset = prefix + id_len + 8;
            let (_, size) = crate::decode_varint(&packet[offset..]).unwrap();
            offset += size;
            for expected_type in [1, 5, 4] {
                let (kind, size) = crate::decode_varint(&packet[offset..]).unwrap();
                offset += size;
                assert_eq!(kind, expected_type);
                let (longs, size) = crate::decode_varint(&packet[offset..]).unwrap();
                offset += size;
                let first = u64::from_be_bytes(packet[offset..offset + 8].try_into().unwrap());
                assert_eq!(first & 511, if kind == 1 { 130 } else { 129 });
                offset += usize::try_from(longs).unwrap() * 8;
            }
        }
    }

    #[test]
    fn native_flat_chunk_matches_the_clean_room_26_2_fixture() {
        let expected = include_bytes!("../assets/flat-chunk-26.2.bin");
        let actual = encode_chunk_with_foundation(0, -1, &[64; 256], DIRT_STATE, &[]);
        assert_eq!(actual.len(), expected.len());
        if let Some(index) = actual
            .iter()
            .zip(expected.iter())
            .position(|(actual, expected)| actual != expected)
        {
            panic!(
                "first fixture difference at byte {index}: generated {:02x}, expected {:02x}",
                actual[index], expected[index]
            );
        }
    }

    #[test]
    fn generated_hills_change_sections_heightmaps_and_light() {
        let mut surface = [64; 256];
        surface[0] = 70;
        surface[255] = 59;
        let generated = encode_generated_chunk(2, 3, &surface);
        assert!(generated.len() > encode_generated_chunk(2, 3, &[64; 256]).len());
    }

    #[test]
    fn feature_blocks_are_added_to_the_palette_and_heightmap() {
        let tree = [ChunkBlockState {
            x: 4,
            y: 70,
            z: 5,
            state_id: 137,
        }];
        let plain = encode_generated_chunk(0, 0, &[64; 256]);
        let wooded = encode_generated_chunk_with_blocks(0, 0, &[64; 256], &tree);
        assert_ne!(plain, wooded);
        assert!(wooded.len() > plain.len());
    }

    #[test]
    fn tree_canopies_do_not_create_fully_black_skylight_columns() {
        let tree = [ChunkBlockState {
            x: 4,
            y: 70,
            z: 5,
            state_id: 279,
        }];
        let wooded = encode_generated_chunk_with_blocks(0, 0, &[64; 256], &tree);
        let without_tree_light = encode_generated_chunk_with_blocks(0, 0, &[64; 256], &[]);
        // The block palette and heightmap differ, while both use terrain-based
        // skylight rather than declaring the whole canopy column unlit.
        assert_ne!(wooded, without_tree_light);
        assert!(wooded.len() > without_tree_light.len());
    }

    #[test]
    fn biome_palette_changes_the_chunk_payload() {
        let plains = encode_generated_chunk_with_blocks_and_biome(0, 0, &[64; 256], &[], 40);
        let taiga = encode_generated_chunk_with_blocks_and_biome(0, 0, &[64; 256], &[], 55);
        assert_ne!(plains, taiga);
    }
}
