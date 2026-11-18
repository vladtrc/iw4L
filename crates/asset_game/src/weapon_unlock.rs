use crate::{CapturedStringTable, FamilyId};
pub trait WeaponUnlockContent {
    fn tables(&self) -> &[(FamilyId, CapturedStringTable)];
    fn contains_family(&self, namespace: FamilyId, name: &str) -> bool;
}
pub type WeaponUnlockDecoder =
    fn(
        &dyn WeaponUnlockContent,
        FamilyId,
        &str,
    ) -> Result<gamemode_iw4::progression::UnlockRequirement, &'static str>;

pub fn reference_weapon_unlock(
    content: &dyn WeaponUnlockContent,
    namespace: FamilyId,
    name: &str,
) -> Result<gamemode_iw4::progression::UnlockRequirement, &'static str> {
    let table = content
        .tables()
        .iter()
        .rev()
        .find(|(ns, t)| *ns == namespace && t.name.eq_ignore_ascii_case("mp/unlockTable.csv"))
        .ok_or("unlock.missing_table")?;
    let row = table
        .1
        .lookup_row_in_col(0, name)
        .or_else(|| table.1.lookup_row_in_col(0, &format!("{name}_mp")))
        .or_else(|| {
            table
                .1
                .lookup_row_in_col(0, &format!("specialty_{}", name.replace('_', "")))
        });
    let Some(row) = row else {
        let perk = content.tables().iter().any(|(ns, t)| {
            *ns == namespace
                && t.name.eq_ignore_ascii_case("mp/perkTable.csv")
                && t.lookup_row_in_col(1, name).is_some()
        });
        return if perk || content.contains_family(namespace, name) {
            Ok(Default::default())
        } else {
            Err("unlock.missing_item")
        };
    };
    gamemode_iw4::progression::UnlockRequirement::capture(
        table.1.cell(row, 2),
        table.1.cell(row, 3),
    )
}
