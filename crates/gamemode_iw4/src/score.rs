#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Score {
    pub client: i32,
    pub score: i32,
    pub ping: i32,
    pub deaths: i32,
    pub team: i32,
    pub kills: i32,
    pub rank: i32,
    pub assists: i32,
    pub statusicon: u32,
}
