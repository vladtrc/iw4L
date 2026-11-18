pub fn capture(root: &asset_transport::GamesRoot) -> asset_game::WeaponSupplementalTables {
    let mut report = Vec::new();
    let tables = read(root, &mut report);
    asset_game::WeaponSupplementalTables { tables, report }
}
fn read(
    root: &asset_transport::GamesRoot,
    report: &mut Vec<String>,
) -> Vec<asset_game::CapturedStringTable> {
    let zone = match asset_transport::find_zone_file_version(
        root,
        "patch_mp",
        ext_t6_fastfile::ZONE_VERSION_PC,
    ) {
        Ok(zone) => zone,
        Err(error) => {
            report.push(format!("t6 class tables: {error}"));
            return Vec::new();
        }
    };
    let image = match crate::zone::decoder::open_t6_zone(&zone.path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 class tables: {error}"));
            return Vec::new();
        }
    };
    let schema = match ext_t6_fastfile::schema::parse() {
        Ok(schema) => schema,
        Err(error) => {
            report.push(format!("t6 class tables: load plan {error:?}"));
            return Vec::new();
        }
    };
    let (load, walked) = ext_t6_fastfile::load_zone(&schema, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 class tables: patch_mp walk stopped: {error:?}"));
    }
    let tables: Vec<_> = load
        .assets
        .iter()
        .filter_map(|asset| crate::weapons::weapon_t6::capture_t6_string_table(&load, asset))
        .filter(|table| {
            asset_game::is_stats_table_name(&table.name)
                || table.name.eq_ignore_ascii_case("mp/attachmentTable.csv")
                || table.name.eq_ignore_ascii_case("mp/mapstable.csv")
                || table.name.eq_ignore_ascii_case("mp/rankTable.csv")
                || table.name.eq_ignore_ascii_case("mp/unlockTable.csv")
        })
        .collect();
    report.push(format!(
        "t6 class tables: {} from {}",
        tables.len(),
        zone.path.display()
    ));
    tables
}
