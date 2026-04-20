use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::Sample;

pub struct RunArtifacts {
    pub csv: PathBuf,
    pub json: PathBuf,
}

pub fn export_run(
    results_dir: &Path,
    label: &str,
    samples: &[Sample],
) -> io::Result<RunArtifacts> {
    fs::create_dir_all(results_dir)?;
    let ts = timestamp_label();
    let stem = sanitize(&format!("{label}_{ts}"));
    let csv_path = results_dir.join(format!("{stem}.csv"));
    let json_path = results_dir.join(format!("{stem}.json"));
    write_csv(&csv_path, samples)?;
    write_json(&json_path, label, samples)?;
    Ok(RunArtifacts {
        csv: csv_path,
        json: json_path,
    })
}

pub fn write_csv(path: &Path, samples: &[Sample]) -> io::Result<()> {
    let metric_names: Vec<String> = all_metric_names(samples).into_iter().collect();
    let mut out = String::new();
    out.push_str("timestamp_ms,scenario");
    for m in &metric_names {
        out.push(',');
        out.push_str(&csv_escape(m));
    }
    out.push('\n');
    for s in samples {
        let ms = system_time_ms(s.ts);
        out.push_str(&ms.to_string());
        out.push(',');
        out.push_str(&csv_escape(&s.scenario));
        for m in &metric_names {
            out.push(',');
            if let Some(v) = s.metrics.get(m) {
                out.push_str(&format_f64(*v));
            }
        }
        out.push('\n');
    }
    fs::write(path, out)
}

pub fn write_json(path: &Path, label: &str, samples: &[Sample]) -> io::Result<()> {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("  \"label\": {},\n", json_string(label)));
    s.push_str(&format!(
        "  \"generated_at_ms\": {},\n",
        system_time_ms(SystemTime::now())
    ));
    s.push_str(&format!("  \"sample_count\": {},\n", samples.len()));
    s.push_str("  \"summary\": ");
    s.push_str(&summary_json(samples));
    s.push_str(",\n");
    s.push_str("  \"samples\": [\n");
    for (i, sample) in samples.iter().enumerate() {
        s.push_str("    {");
        s.push_str(&format!(
            "\"ts_ms\": {}, \"scenario\": {}, \"metrics\": {{",
            system_time_ms(sample.ts),
            json_string(&sample.scenario)
        ));
        let sorted: BTreeMap<&String, &f64> = sample.metrics.iter().collect();
        let mut first = true;
        for (k, v) in sorted {
            if !first {
                s.push_str(", ");
            }
            first = false;
            s.push_str(&format!("{}: {}", json_string(k), format_f64(*v)));
        }
        s.push_str("}}");
        if i + 1 < samples.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("  ]\n}\n");
    fs::write(path, s)
}

pub fn summary_text(samples: &[Sample]) -> String {
    let grouped = group_by_metric(samples);
    if grouped.is_empty() {
        return "(no samples)".into();
    }
    let mut out = String::new();
    for (name, values) in &grouped {
        let st = stats(values);
        out.push_str(&format!(
            "{name}: n={n} mean={mean} min={min} max={max} p95={p95}\n",
            name = name,
            n = values.len(),
            mean = format_f64(st.mean),
            min = format_f64(st.min),
            max = format_f64(st.max),
            p95 = format_f64(st.p95),
        ));
    }
    out.trim_end().to_string()
}

fn summary_json(samples: &[Sample]) -> String {
    let grouped = group_by_metric(samples);
    let mut s = String::from("{");
    let mut first = true;
    for (name, values) in &grouped {
        let st = stats(values);
        if !first {
            s.push_str(", ");
        }
        first = false;
        s.push_str(&format!(
            "{}: {{\"n\": {}, \"mean\": {}, \"min\": {}, \"max\": {}, \"p95\": {}}}",
            json_string(name),
            values.len(),
            format_f64(st.mean),
            format_f64(st.min),
            format_f64(st.max),
            format_f64(st.p95),
        ));
    }
    s.push('}');
    s
}

struct Stats {
    mean: f64,
    min: f64,
    max: f64,
    p95: f64,
}

fn stats(values: &[f64]) -> Stats {
    debug_assert!(!values.is_empty());
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let sum: f64 = sorted.iter().sum();
    let mean = sum / n as f64;
    let min = sorted[0];
    let max = sorted[n - 1];
    let rank = ((n as f64) * 0.95).ceil() as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    let p95 = sorted[idx];
    Stats {
        mean,
        min,
        max,
        p95,
    }
}

fn group_by_metric(samples: &[Sample]) -> BTreeMap<String, Vec<f64>> {
    let mut out: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for s in samples {
        for (k, v) in &s.metrics {
            out.entry(k.clone()).or_default().push(*v);
        }
    }
    out
}

fn all_metric_names(samples: &[Sample]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for s in samples {
        for k in s.metrics.keys() {
            out.insert(k.clone());
        }
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let body = s.replace('"', "\"\"");
        format!("\"{body}\"")
    } else {
        s.to_string()
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn format_f64(v: f64) -> String {
    if v.is_nan() || v.is_infinite() {
        return "null".into();
    }
    if v.fract() == 0.0 && v.abs() < 1e16 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn system_time_ms(t: SystemTime) -> u128 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn timestamp_label() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // crude UTC YYYYMMDD-HHMMSS without external deps
    let (y, mo, d, h, mi, s) = civil_from_secs(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

fn civil_from_secs(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    // Howard Hinnant's civil_from_days
    let days = (secs / 86400) as i64;
    let tod = (secs % 86400) as u32;
    let h = tod / 3600;
    let mi = (tod % 3600) / 60;
    let s = tod % 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i32 + (era as i32) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, h, mi, s)
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;

    fn sample(scn: &str, k: &str, v: f64, ts_secs: u64) -> Sample {
        let mut m = HashMap::new();
        m.insert(k.to_string(), v);
        Sample {
            ts: UNIX_EPOCH + Duration::from_secs(ts_secs),
            scenario: scn.into(),
            metrics: m,
        }
    }

    #[test]
    fn stats_p95_basic() {
        let vals: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        let st = stats(&vals);
        assert_eq!(st.min, 1.0);
        assert_eq!(st.max, 100.0);
        assert!((st.mean - 50.5).abs() < 1e-9);
        assert_eq!(st.p95, 95.0);
    }

    #[test]
    fn civil_converts_epoch() {
        let (y, mo, d, h, mi, s) = civil_from_secs(0);
        assert_eq!((y, mo, d, h, mi, s), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn civil_converts_known_ts() {
        // 2024-01-02T03:04:05Z = 1704164645
        let (y, mo, d, h, mi, s) = civil_from_secs(1704164645);
        assert_eq!((y, mo, d, h, mi, s), (2024, 1, 2, 3, 4, 5));
    }

    #[test]
    fn csv_roundtrip_smoke() {
        let samples = vec![
            sample("a", "mspt", 12.5, 100),
            sample("a", "mspt", 13.25, 101),
        ];
        let dir = std::env::temp_dir().join(format!("obs_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("out.csv");
        write_csv(&path, &samples).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("timestamp_ms,scenario,mspt"));
        assert!(text.contains("100000,a,12.5"));
    }

    #[test]
    fn summary_text_contains_metric() {
        let samples = vec![
            sample("a", "mspt", 10.0, 1),
            sample("a", "mspt", 20.0, 2),
        ];
        let text = summary_text(&samples);
        assert!(text.contains("mspt"));
        assert!(text.contains("mean=15"));
    }
}
