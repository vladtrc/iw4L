use super::composer;
use asset_core::FamilyId;
use asset_game::*;
use asset_iw4::size::weap_anim;
use std::collections::HashMap;

pub fn prepare(content: &mut dyn WeaponPreparationContent) -> ConfigurationPreparationCensus {
    let kinds = attachment_kinds(content.tables());
    let mut census = ConfigurationPreparationCensus::default();
    let mut prepared = Vec::new();
    for (base_id, selection) in content.candidates(crate::ID) {
        if selection.attachments.is_empty() {
            continue;
        }
        let Some(family) = selection
            .family
            .as_ref()
            .and_then(|key| content.family(key))
        else {
            continue;
        };
        let attachments = content.normalize(crate::ID, &selection.attachments);
        let name =
            (super::super::weapon_policy::POLICY.authored_name)(&family.key.base, &attachments);
        if content.authored(crate::ID, &name).is_some() {
            continue;
        }
        let selection = WeaponSelection {
            attachments: attachments.clone(),
            ..selection
        };
        let kinds: Option<Vec<u32>> = attachments.iter().map(|a| kinds.get(a).copied()).collect();
        let row = kinds.and_then(|kinds| {
            content
                .draft(base_id)
                .and_then(|base| composer::compose(&base, &kinds, name))
        });
        match row {
            Some(mut row) => {
                row.attachment_caption_keys = attachments
                    .iter()
                    .filter_map(|name| family.attachments.iter().find(|a| &a.name == name))
                    .map(|a| a.caption_key.trim_start_matches('@').to_owned())
                    .filter(|key| !key.is_empty())
                    .collect();
                row.prepared_attachments = attachments;
                prepared.push((base_id, selection, row));
            }
            None => census.refused.push(selection),
        }
    }
    census.prepared = prepared.len();
    for (base_id, selection, row) in prepared {
        let parent = content.append(base_id, base_id, Some(selection), row);
        let Some(base) = content.draft(base_id) else {
            continue;
        };
        let Some(mut config) = content.draft(parent) else {
            continue;
        };
        let Some(alt_name) = config.alternate_weapon.as_deref() else {
            continue;
        };
        let Some(alt_id) = content.authored(crate::ID, &normalize_name(alt_name)) else {
            continue;
        };
        let Some(alternate) = content.draft(alt_id) else {
            continue;
        };
        if let Some((alternate, raise)) =
            composer::compose_alternate(&base, &config, &alternate, parent as usize)
        {
            config.alternate_weapon = Some(alternate.name.clone());
            config.sz_xanims[weap_anim::ALT_RAISE] = raise;
            config.sz_xanims[weap_anim::ALT_DROP] = None;
            config.facts.alternate_drop_time_ms = 0;
            content.replace(parent, config);
            content.append(alt_id, base_id, None, alternate);
            census.alternates += 1;
        }
    }
    census
}
fn normalize_name(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    name.strip_suffix("_mp").unwrap_or(&name).to_owned()
}
fn attachment_kinds(tables: &[(FamilyId, CapturedStringTable)]) -> HashMap<String, u32> {
    tables
        .iter()
        .filter(|(namespace, table)| {
            *namespace == crate::ID && table.name.eq_ignore_ascii_case("mp/attachmentTable.csv")
        })
        .flat_map(|(_, table)| {
            (1..table.rows as i32)
                .filter(|&row| table.cell(row, 2) == "attachment")
                .filter_map(|row| {
                    Some((
                        table.cell(row, 4).to_ascii_lowercase(),
                        table.cell(row, 0).parse().ok()?,
                    ))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
