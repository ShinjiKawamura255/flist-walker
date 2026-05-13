use super::*;

fn parse_filelist_stream_metadata_probe_count(filelist_path: &Path, root: &Path) -> Result<usize> {
    // This models the probe-heavy path we want to keep out of the fast stream.
    let file = File::open(filelist_path)
        .with_context(|| format!("failed to read {}", filelist_path.display()))?;
    let reader = BufReader::new(file);
    let filelist_base = filelist_path.parent().unwrap_or(root);
    let mut seen = HashSet::new();
    let mut count = 0usize;

    for line_result in reader.lines() {
        let raw =
            line_result.with_context(|| format!("failed to read {}", filelist_path.display()))?;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let candidates = resolve_filelist_entry_candidates(line, filelist_base, root);
        for candidate in candidates {
            let Ok(meta) = candidate.metadata() else {
                continue;
            };
            if !meta.is_file() && !meta.is_dir() {
                continue;
            }
            if seen.insert(candidate) {
                count = count.saturating_add(1);
            }
            break;
        }
    }
    Ok(count)
}

#[test]
#[ignore = "perf measurement; run explicitly"]
fn perf_filelist_stream_is_faster_than_metadata_probe_baseline() {
    let root = test_root("perf-filelist-metadata-probe-baseline");
    fs::create_dir_all(&root).expect("create dir");
    let filelist = root.join("FileList.txt");
    let data_dir = root.join("dataset");
    fs::create_dir_all(&data_dir).expect("create dataset dir");
    let lines = 30_000usize;
    let mut text = String::with_capacity(lines * 24);
    for i in 0..lines {
        let rel = format!("dataset\\f-{i}.txt");
        fs::write(root.join(&rel), "x").expect("write synthetic file");
        text.push_str(&rel);
        text.push('\n');
    }
    fs::write(&filelist, text).expect("write synthetic filelist");

    let iterations = 5usize;
    let mut probe_baseline_best = Duration::MAX;
    let mut current_best = Duration::MAX;
    let mut probe_baseline_count = 0usize;
    let mut current_count = 0usize;

    for _ in 0..iterations {
        let probe_baseline_start = Instant::now();
        probe_baseline_count = parse_filelist_stream_metadata_probe_count(&filelist, &root)
            .expect("metadata probe baseline parse");
        probe_baseline_best = probe_baseline_best.min(probe_baseline_start.elapsed());

        let current_start = Instant::now();
        current_count = 0usize;
        parse_filelist_stream(
            &filelist,
            &root,
            true,
            true,
            || false,
            |_path, _is_dir| {
                current_count = current_count.saturating_add(1);
            },
        )
        .expect("current parse");
        current_best = current_best.min(current_start.elapsed());
    }

    let probe_baseline_ms = probe_baseline_best.as_secs_f64() * 1000.0;
    let current_ms = current_best.as_secs_f64() * 1000.0;
    let speedup = if current_ms > 0.0 {
        probe_baseline_ms / current_ms
    } else {
        f64::INFINITY
    };

    eprintln!(
        "FileList perf control_baseline lines={lines} metadata_probe_ms={probe_baseline_ms:.3} current_ms={current_ms:.3} speedup={speedup:.2}x metadata_probe_count={probe_baseline_count} current_count={current_count}"
    );

    assert_eq!(probe_baseline_count, lines);
    assert_eq!(current_count, lines);
    // Hosted Linux runners vary enough that a 30% floor flakes on otherwise healthy builds.
    // Keep the gate strict enough to catch real regressions, but wide enough to absorb runner noise.
    assert!(
        speedup >= 1.20,
        "line-only FileList parse did not beat the metadata-probe control baseline enough: {speedup:.2}x"
    );
    let _ = fs::remove_dir_all(&root);
}
