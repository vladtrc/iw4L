use serde_json::{Value, json};
use std::time::Instant;
pub(super) fn write(progress: &asset_transport::LoadProgress) -> Value {
    for lane in progress.lane_timings() {
        perf::instant!("load_stage", "iw4l.frame",
            "label" => lane.label.as_str(), "start_ns" => lane.at.as_nanos() as u64,
            "duration_ns" => lane.elapsed.as_nanos() as u64, "done" => lane.done, "total" => lane.total,
            "rss_delta" => lane.rss_delta, "heap_delta" => lane.heap_delta,
            "running" => lane.running, "outcome" => lane.outcome_suffix());
    }
    let data = audio().to_string();
    perf::instant!("load_audio", "iw4l.frame", "data" => data.as_str());
    let jobs = asset_transport::load_jobs::snapshot();
    let origin = jobs.rows.iter().map(|row| row.discovered_at).min();
    for row in &jobs.rows {
        let time = |at: Option<Instant>| {
            at.map(|at| at.duration_since(row.discovered_at).as_nanos() as u64)
        };
        let data = serde_json::json!({
            "id": row.id, "kind": row.kind.name(), "canonical": row.canonical,
            "deps": row.deps, "namespace": row.namespace,
            "discovered_ns": origin.map(|start| row.discovered_at.duration_since(start).as_nanos() as u64),
            "plan_ready_ns": time(row.plan_ready_at), "enqueued_ns": time(row.enqueued_at),
            "started_ns": time(row.started_at), "decode_started_ns": time(row.decode_started_at),
            "finished_ns": time(row.finished_at), "joined_ns": time(row.joined_at),
            "prepared_bytes": row.prepared_bytes, "reused_bytes": row.reused_bytes,
            "retained_bytes": row.retained_bytes, "discarded_bytes": row.discarded_bytes,
            "source_bytes": row.source_bytes, "output_bytes": row.output_bytes,
            "outstanding_at_decode": row.outstanding_at_decode, "waited_for_budget": row.waited_for_budget,
            "rss_at_finish": row.rss_at_finish, "items": row.items,
            "cache_result": row.cache_result.map(|result| result.name()), "discard_reason": row.discard_reason
        })
        .to_string();
        perf::instant!("load_job", "iw4l.frame", "data" => data.as_str());
    }
    json!({ "rows": jobs.rows.len(), "capacity": jobs.capacity, "dropped": jobs.dropped, "per_asset": jobs.per_asset })
}

fn audio() -> Value {
    let prep = audio::clip_prep_cost();
    let pcm = audio::pcm_memory();
    let decode = audio::decode_memory();
    let decoder_costs = asset_audio::audio_families()
        .iter()
        .filter_map(|(family, policy)| {
            let cost = (policy.decoder_telemetry?)();
            Some(json!({"family":family.as_str(),"backend":cost.backend,
            "cache_hit":cost.cache_hit,"cache_miss":cost.cache_miss,
            "native_decodes":cost.native_decodes,"failed":cost.failed,
            "pcm_bytes":cost.pcm_bytes,"decode_ms":cost.decode_ms,
            "key_ms":cost.key_ms,"cache_io_ms":cost.cache_io_ms}))
        })
        .collect::<Vec<_>>();
    json!({
        "workers": prep.workers,
        "requests": prep.requests,
        "queued": prep.queued,
        "queue_wait_ms": prep.queue_wait_ms,
        "queue_deferred": prep.queue_deferred,
        "request_limit": prep.request_limit,
        "late_prepares": prep.late,
        "decode_memory": { "limit_bytes": decode.limit_bytes, "live_bytes": decode.live_bytes, "peak_bytes": decode.peak_bytes, "refused": decode.refused },
        "pcm_memory": { "limit_bytes": pcm.limit_bytes, "live_bytes": pcm.live_bytes, "peak_bytes": pcm.peak_bytes, "refused": pcm.refused },
        "paths": prep.paths.iter().map(|(path, cost)| json!({
            "decoder": path.name(),
            "prepared": cost.prepared,
            "failed": cost.failed,
            "worker_ms": cost.wall_ms,
            "resident_pcm_bytes": cost.sample_bytes,
        })).collect::<Vec<_>>(),
        "decoder_costs": decoder_costs,
    })
}
