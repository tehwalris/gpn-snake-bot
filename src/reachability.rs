use crate::direction::Direction;

pub fn calculate_reachable(
    size: (usize, usize),
    occupied_mask: &[bool],
    start_pos: (usize, usize),
) -> Vec<bool> {
    let (width, height) = size;

    assert_eq!(occupied_mask.len(), width * height);

    let mut reachable = vec![false; width * height];
    let mut queue = std::collections::VecDeque::new();

    let start_i = start_pos.1 * width + start_pos.0;
    reachable[start_i] = true;
    queue.push_back(start_i);

    while let Some(current) = queue.pop_front() {
        let pos = (current % width, current / width);

        for direction in Direction::all_directions() {
            let new_pos = direction.offset_pos(pos, size);
            let new_i = new_pos.1 * width + new_pos.0;

            if !reachable[new_i] && !occupied_mask[new_i] {
                reachable[new_i] = true;
                queue.push_back(new_i);
            }
        }
    }

    reachable
}
