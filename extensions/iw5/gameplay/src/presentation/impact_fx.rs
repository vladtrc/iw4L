pub fn impact_row(impact_type: i32, exit: bool) -> Option<usize> {
    let exit = usize::from(exit);
    Some(match impact_type {
        1 => exit,
        2 => 3 + exit,
        3 => 7 + exit,
        4 => 9 + exit,
        5 => 5 + exit,
        6 => 11,
        7 | 8 => 12,
        9 => 13,
        10 => 14,
        11 => 15,
        12 => 16,
        13 => 17,
        14 => 19,
        15 => 20,
        _ => return None,
    })
}
