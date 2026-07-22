use super::*;

fn validate_control_line<'a>(
    raw: &'a str,
    line_index: usize,
    filelist_path: &Path,
) -> Result<&'a str> {
    let line = if line_index == 0 {
        raw.strip_prefix('\u{feff}').unwrap_or(raw)
    } else {
        raw
    };
    if line.len() > 1024 * 1024 {
        anyhow::bail!(
            "FileList line {} in {} exceeds the 1 MiB logical line limit",
            line_index + 1,
            filelist_path.display()
        );
    }
    if line.as_bytes().contains(&0) {
        anyhow::bail!(
            "invalid FileList encoding in {}: NUL bytes are not allowed",
            filelist_path.display()
        );
    }
    Ok(line)
}

fn parse_filelist_stream_metadata_probe_count(filelist_path: &Path, root: &Path) -> Result<usize> {
    // This models the probe-heavy path we want to keep out of the fast stream.
    let reader = open_validated_filelist(filelist_path, &|| false)?;
    let filelist_base = filelist_path.parent().unwrap_or(root);
    let mut seen = HashSet::new();
    let mut count = 0usize;

    for (line_index, line_result) in reader.lines().enumerate() {
        let raw =
            line_result.with_context(|| format!("failed to read {}", filelist_path.display()))?;
        let line = validate_control_line(&raw, line_index, filelist_path)?.trim();
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

fn parse_filelist_stream_allocating_lines_count(
    filelist_path: &Path,
    root: &Path,
) -> Result<(usize, Option<PathBuf>)> {
    let reader = open_validated_filelist(filelist_path, &|| false)?;
    let filelist_base = filelist_path.parent().unwrap_or(root);
    let mut seen = HashSet::new();
    let mut count = 0usize;

    for (line_index, line_result) in reader.lines().enumerate() {
        let raw =
            line_result.with_context(|| format!("failed to read {}", filelist_path.display()))?;
        let line = validate_control_line(&raw, line_index, filelist_path)?.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let candidates = resolve_filelist_entry_candidates(line, filelist_base, root);
        if let Some(path) = candidates.into_iter().next() {
            if seen.insert(path) {
                count = count.saturating_add(1);
            }
        }
    }
    let only_path = (count == 1).then(|| seen.into_iter().next()).flatten();
    Ok((count, only_path))
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
    let mut validation_best = Duration::MAX;
    let mut probe_baseline_count = 0usize;
    let mut current_count = 0usize;

    for _ in 0..iterations {
        let validation_start = Instant::now();
        validate_filelist_encoding(&filelist, &|| false).expect("validation-only pass");
        validation_best = validation_best.min(validation_start.elapsed());

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
    let validation_ms = validation_best.as_secs_f64() * 1000.0;
    let speedup = if current_ms > 0.0 {
        probe_baseline_ms / current_ms
    } else {
        f64::INFINITY
    };

    eprintln!(
        "FileList perf control_baseline lines={lines} validation_only_ms={validation_ms:.3} metadata_probe_total_ms={probe_baseline_ms:.3} current_total_ms={current_ms:.3} speedup={speedup:.2}x metadata_probe_count={probe_baseline_count} current_count={current_count}"
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

#[test]
#[ignore = "perf measurement; run explicitly"]
fn perf_filelist_stream_reuses_line_buffer() {
    let root = test_root("perf-filelist-reuse-line-buffer");
    fs::create_dir_all(&root).expect("create dir");
    let filelist = root.join("FileList.txt");
    let lines = 100_000usize;
    let mut text = String::with_capacity(lines * 32);
    for i in 0..lines - 1 {
        text.push_str("# ignored/");
        text.push_str(&i.to_string());
        text.push_str("/entry.txt\n");
    }
    text.push_str("dataset/sentinel/entry.txt\n");
    fs::write(&filelist, text).expect("write synthetic filelist");

    let expected_path = root.join("dataset/sentinel/entry.txt");
    let iterations = 7usize;
    let mut allocating_samples = Vec::with_capacity(iterations);
    let mut current_samples = Vec::with_capacity(iterations);
    let mut paired_speedups = Vec::with_capacity(iterations);
    let mut allocating_count = 0usize;
    let mut current_count = 0usize;
    let mut allocating_path = None;
    let mut current_path = None;

    let validation_start = Instant::now();
    validate_filelist_encoding(&filelist, &|| false).expect("validation-only pass");
    let validation_elapsed = validation_start.elapsed();

    for iteration in 0..iterations {
        let allocating_start = Instant::now();
        let run_allocating = || {
            parse_filelist_stream_allocating_lines_count(&filelist, &root)
                .expect("allocating lines parse")
        };
        let run_current = || {
            let mut count = 0usize;
            let mut only_path = None;
            parse_filelist_stream(
                &filelist,
                &root,
                true,
                true,
                || false,
                |path, _is_dir| {
                    count = count.saturating_add(1);
                    only_path = Some(path);
                },
            )
            .expect("current parse");
            (count, only_path)
        };

        let (allocating_elapsed, current_elapsed) = if iteration % 2 == 0 {
            let (count, path) = run_allocating();
            allocating_count = count;
            allocating_path = path;
            let allocating_elapsed = allocating_start.elapsed();

            let current_start = Instant::now();
            (current_count, current_path) = run_current();
            (allocating_elapsed, current_start.elapsed())
        } else {
            let current_start = Instant::now();
            (current_count, current_path) = run_current();
            let current_elapsed = current_start.elapsed();

            let allocating_start = Instant::now();
            let (count, path) = run_allocating();
            allocating_count = count;
            allocating_path = path;
            (allocating_start.elapsed(), current_elapsed)
        };

        allocating_samples.push(allocating_elapsed);
        current_samples.push(current_elapsed);
        paired_speedups.push(allocating_elapsed.as_secs_f64() / current_elapsed.as_secs_f64());
    }

    allocating_samples.sort_unstable();
    current_samples.sort_unstable();
    paired_speedups.sort_by(f64::total_cmp);
    let allocating_ms = allocating_samples[iterations / 2].as_secs_f64() * 1000.0;
    let current_ms = current_samples[iterations / 2].as_secs_f64() * 1000.0;
    let validation_ms = validation_elapsed.as_secs_f64() * 1000.0;
    let speedup = paired_speedups[iterations / 2];

    eprintln!(
        "FileList line buffer perf lines={lines} validation_only_ms={validation_ms:.3} allocating_lines_median_ms={allocating_ms:.3} current_median_ms={current_ms:.3} paired_median_speedup={speedup:.4}x allocating_count={allocating_count} current_count={current_count}"
    );

    assert_eq!(allocating_count, 1);
    assert_eq!(current_count, 1);
    assert_eq!(allocating_path.as_deref(), Some(expected_path.as_path()));
    assert_eq!(current_path.as_deref(), Some(expected_path.as_path()));
    assert!(
        speedup >= 1.02,
        "reused line buffer did not beat allocating lines baseline enough: {speedup:.2}x"
    );
    let _ = fs::remove_dir_all(&root);
}
