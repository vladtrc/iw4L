use asset_game::*;
pub fn decode(
    content: &dyn WeaponUnlockContent,
    namespace: asset_core::FamilyId,
    name: &str,
) -> Result<gamemode_iw4::progression::UnlockRequirement, &'static str> {
    let table = &content
        .tables()
        .iter()
        .rev()
        .find(|(ns, t)| *ns == namespace && t.name.eq_ignore_ascii_case("mp/statsTable.csv"))
        .ok_or("unlock.missing_table")?
        .1;
    let row = table
        .lookup_row_in_col(4, name)
        .or_else(|| table.lookup_row_in_col(4, &format!("{name}_mp")))
        .ok_or("unlock.missing_item")?;
    let rank = table.cell(row, 10);
    let requirement = gamemode_iw4::progression::UnlockRequirement::capture(
        if rank.is_empty() { "0" } else { rank },
        "",
    )?;
    Ok(requirement)
}
pub const DECODER: WeaponUnlockDecoder = decode;
