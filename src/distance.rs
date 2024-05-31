use crate::direction::Direction;

pub fn calculate_distances(size: (usize, usize), occupied_mask: &[bool]) -> Vec<usize> {
    let (width, height) = size;

    assert_eq!(occupied_mask.len(), width * height);

    let mut distances = vec![usize::MAX; width * height];
    let mut queue = std::collections::VecDeque::new();

    for (i, &is_occupied) in occupied_mask.iter().enumerate() {
        if is_occupied {
            distances[i] = 0;
            queue.push_back(i);
        }
    }

    while let Some(current) = queue.pop_front() {
        let current_distance = distances[current];
        let pos = (current % width, current / width);

        for direction in Direction::all_directions() {
            let new_pos = direction.offset_pos(pos, size);
            let new_i = new_pos.1 * width + new_pos.0;
            let new_distance = current_distance + 1;

            if new_distance < distances[new_i] {
                distances[new_i] = new_distance;
                queue.push_back(new_i);
            }
        }
    }

    distances
}
