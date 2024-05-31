use crate::direction::Direction;
use petgraph::graph::UnGraph;

pub fn shortest_path_next_direction(
    size: (usize, usize),
    occupied_mask: &[bool],
    start_pos: (usize, usize),
    target_pos: (usize, usize),
) -> Option<Direction> {
    let (width, height) = size;
    let start_i = start_pos.1 * width + start_pos.0;
    let target_i = target_pos.1 * width + target_pos.0;

    assert_eq!(occupied_mask.len(), width * height);

    let g = UnGraph::<(), (), usize>::from_edges((0..occupied_mask.len()).flat_map(|i| {
        let pos = (i % width, i / width);
        [Direction::Right, Direction::Down]
            .iter()
            .flat_map(move |&direction| {
                let new_pos = direction.offset_pos(pos, size);
                let new_i = new_pos.1 * width + new_pos.0;
                if (!occupied_mask[i] || pos == start_pos)
                    && (!occupied_mask[new_i] || new_pos == start_pos)
                {
                    Some((i, new_i))
                } else {
                    None
                }
            })
    }));

    let path = petgraph::algo::astar(
        &g,
        start_i.into(),
        |finish| finish == target_i.into(),
        |_| 1,
        |_| 0,
    );

    if let Some((_, path)) = path {
        if path.len() >= 2 {
            let a_i = path[0].index();
            let b_i = path[1].index();

            let a_pos = (a_i % width, a_i / width);
            let b_pos = (b_i % width, b_i / width);

            for direction in Direction::all_directions() {
                if direction.offset_pos(a_pos, size) == b_pos {
                    return Some(direction);
                }
            }
            panic!("steps in path aren't adjacent")
        } else {
            None
        }
    } else {
        None
    }
}
