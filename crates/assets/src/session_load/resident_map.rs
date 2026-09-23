use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ZoneStamp {
    pub(super) path: PathBuf,
    pub(super) len: u64,
    modified: Option<std::time::SystemTime>,
}

impl ZoneStamp {
    pub(super) fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

struct ResidentMap {
    pub(super) zone: ZoneStamp,
    pub(super) common: Arc<CommonSet>,
    pub(super) prepared: PreparedMatch,
}

static RESIDENT_MAP: std::sync::Mutex<Option<ResidentMap>> = std::sync::Mutex::new(None);

fn resident_copy(zone: &ZoneStamp, key: &CommonKey) -> Option<PreparedMatch> {
    let common = landed_common(key)?;
    let slot = RESIDENT_MAP
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let resident = slot.as_ref()?;
    (resident.zone == *zone && Arc::ptr_eq(&resident.common, &common))
        .then(|| resident.prepared.clone())
}

pub async fn load_prepared_match(
    zone_ff: Result<PathBuf, String>,
    common_mp: Result<PathBuf, String>,
    progress: LoadProgress,
) -> MatchLoadOutcome {
    let stamp = zone_ff.as_deref().ok().and_then(ZoneStamp::of);
    if let Some(stamp) = &stamp {
        let key = CommonKey::for_match(
            Some(&stamp.path),
            common_mp.as_deref().ok(),
            &mut Vec::new(),
        );
        let copying = std::time::Instant::now();
        if let Some(mut prepared) = resident_copy(stamp, &key) {
            progress.record_reused_scoped(StageId::CommonAssets, "shared common");
            progress.record_reused_scoped(StageId::MapAssets, "resident");
            progress.record_reused_scoped(StageId::Images, "map");
            let line = format!(
                "resident map: reused `{}` walk #{} — no zone opened, no decode; match copy {:.0}ms",
                stamp.path.display(),
                prepared.materials.products_id,
                copying.elapsed().as_secs_f32() * 1000.0
            );
            diag::info!(World, "{line}");
            prepared.report.push(line);
            prepared.report.extend(progress.timing_report());
            return MatchLoadOutcome::Ready(prepared);
        }
    }
    let dropped = RESIDENT_MAP
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .take();
    if let Some(dropped) = dropped {
        diag::info!(
            World,
            "resident map: `{}` walk #{} dropped before the next walk",
            dropped.zone.path.display(),
            dropped.prepared.materials.products_id
        );
    }
    let (outcome, common) = walk_prepared_match(zone_ff, common_mp, progress.clone()).await;
    let MatchLoadOutcome::Ready(mut prepared) = outcome else {
        return outcome;
    };
    if let (Some(zone), Some(common)) = (stamp, common) {
        let keeping = std::time::Instant::now();
        let resident = prepared.clone();
        prepared.report.push(format!(
            "resident map: kept `{}` walk #{} for a same-map load ({:.0}ms)",
            zone.path.display(),
            prepared.materials.products_id,
            keeping.elapsed().as_secs_f32() * 1000.0
        ));
        *RESIDENT_MAP
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(ResidentMap {
            zone,
            common,
            prepared: resident,
        });
    }
    prepared.report.extend(progress.timing_report());
    MatchLoadOutcome::Ready(prepared)
}
