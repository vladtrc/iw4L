pub fn convert_offset_to_pointer(offset: u32, blocks: &[u32; 16]) -> u32 {
    let adjusted = offset.wrapping_sub(1);
    let block = blocks[(adjusted >> 28) as usize];
    block.wrapping_add(adjusted & 0x0fff_ffff)
}

pub fn convert_offset_to_alias(offset: u32, blocks: &[&[u8]; 16]) -> Option<u32> {
    let adjusted = offset.wrapping_sub(1);
    let block = blocks[(adjusted >> 28) as usize];
    let start = (adjusted & 0x0fff_ffff) as usize;
    let bytes = block.get(start..start.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

pub fn load_stream(
    stream_index: u8,
    active_index: u8,
    destination: &mut [u8],
    source: Option<&[u8]>,
    position: u32,
) -> Option<u32> {
    if stream_index == 0 || destination.is_empty() {
        return Some(position);
    }

    if active_index == 2 {
        destination.fill(0);
    } else {
        let bytes = source?.get(..destination.len())?;
        destination.copy_from_slice(bytes);
    }

    let size = u32::try_from(destination.len()).ok()?;
    Some(position.wrapping_add(size))
}

pub fn alloc_stream_pos(position: u32, alignment: u32) -> u32 {
    position.wrapping_add(alignment) & !alignment
}

pub fn inc_stream_pos(position: u32, size: u32) -> u32 {
    position.wrapping_add(size)
}

pub fn insert_pointer(
    active_index: usize,
    position: u32,
    positions: &mut [u32; 8],
    stack: &mut [(usize, u32)],
    depth: &mut usize,
) -> Option<(u32, usize, u32)> {
    let (insert_stream, stream_position) =
        push_stream_pos(active_index, position, positions, stack, depth, 4)?;
    let pointer_position = alloc_stream_pos(stream_position, 3);
    let advanced_position = inc_stream_pos(pointer_position, 4);
    let (restored_index, restored_position) =
        pop_stream_pos(insert_stream, advanced_position, positions, stack, depth)?;

    Some((pointer_position, restored_index, restored_position))
}

pub fn push_stream_pos(
    active_index: usize,
    position: u32,
    positions: &mut [u32; 8],
    stack: &mut [(usize, u32)],
    depth: &mut usize,
    target_index: usize,
) -> Option<(usize, u32)> {
    if active_index >= positions.len() || target_index >= positions.len() || *depth >= stack.len() {
        return None;
    }

    stack[*depth].0 = active_index;
    *depth += 1;

    let position = if active_index == target_index {
        position
    } else {
        positions[active_index] = position;
        positions[target_index]
    };
    stack[*depth - 1].1 = position;

    Some((target_index, position))
}

pub fn pop_stream_pos(
    active_index: usize,
    mut position: u32,
    positions: &mut [u32; 8],
    stack: &mut [(usize, u32)],
    depth: &mut usize,
) -> Option<(usize, u32)> {
    if active_index >= positions.len() || *depth == 0 {
        return None;
    }

    *depth -= 1;
    let (saved_index, saved_position) = stack[*depth];
    if saved_index >= positions.len() {
        *depth += 1;
        return None;
    }
    if active_index == 0 {
        position = saved_position;
    }
    if active_index != saved_index {
        positions[active_index] = position;
        position = positions[saved_index];
    }

    Some((saved_index, position))
}
