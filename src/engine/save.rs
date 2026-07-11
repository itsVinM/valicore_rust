use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

fn fmt_f64(v: f64) -> String {
    let s = format!("{:.10}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

pub fn save_csv(
    path: &Path,
    time_axis: &[f64],
    data_matrix: &[Vec<f64>],
    metadata: &HashMap<String, String>,
    channel_labels: &[String],
) -> Result<String, String> {
    let n_ch = data_matrix.len();
    let n_pts = time_axis.len();

    // Pre-calculate buffer size for zero-alloc write
    // Each line: time,ch1,ch2,...\n  ~20 bytes per value
    let est = metadata.len() * 40 + n_pts * (20 + n_ch * 20);
    let mut buf = String::with_capacity(est);

    // Metadata comments
    for (k, v) in metadata {
        writeln!(&mut buf, "# {k}: {v}").map_err(|e| format!("write: {e}"))?;
    }

    // Header
    write!(&mut buf, "time").map_err(|e| format!("write: {e}"))?;
    for label in channel_labels {
        write!(&mut buf, ",{label}").map_err(|e| format!("write: {e}"))?;
    }
    buf.push('\n');

    // Data rows
    for i in 0..n_pts {
        write!(&mut buf, "{}", fmt_f64(time_axis[i])).map_err(|e| format!("write: {e}"))?;
        for ch in 0..n_ch {
            if i < data_matrix[ch].len() {
                write!(&mut buf, ",{}", fmt_f64(data_matrix[ch][i])).map_err(|e| format!("write: {e}"))?;
            } else {
                buf.push(',');
            }
        }
        buf.push('\n');
    }

    std::fs::write(path, buf.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(feature = "hdf5")]
pub fn save_h5(
    path: &Path,
    time_axis: &[f64],
    data_matrix: &[Vec<f64>],
    metadata: &HashMap<String, String>,
    channel_labels: &[String],
) -> Result<String, String> {
    use hdf5::File;

    let file = File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;

    // Time dataset
    file.new_dataset_builder()
        .with_data(time_axis)
        .create("time")
        .map_err(|e| format!("write time: {e}"))?;

    // Channel datasets
    for (i, label) in channel_labels.iter().enumerate() {
        if i < data_matrix.len() {
            file.new_dataset_builder()
                .with_data(data_matrix[i].as_slice())
                .create(label.as_str())
                .map_err(|e| format!("write {label}: {e}"))?;
        }
    }

    // Metadata as attributes
    let grp = file.group("/").map_err(|e| format!("root group: {e}"))?;
    for (k, v) in metadata {
        grp.attr(k.as_str())
            .and_then(|a| a.write_scalar(v))
            .map_err(|e| format!("attr {k}: {e}"))?;
    }

    Ok(path.to_string_lossy().into_owned())
}

#[cfg(not(feature = "hdf5"))]
pub fn save_h5(
    _path: &Path,
    _time_axis: &[f64],
    _data_matrix: &[Vec<f64>],
    _metadata: &HashMap<String, String>,
    _channel_labels: &[String],
) -> Result<String, String> {
    Err("HDF5 support not compiled: rebuild with --features hdf5".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_data() -> (Vec<f64>, Vec<Vec<f64>>, HashMap<String, String>, Vec<String>) {
        let time = vec![0.0, 0.001, 0.002];
        let ch1 = vec![0.0, 1.0, 0.0];
        let ch2 = vec![1.0, 0.0, -1.0];
        let mut meta = HashMap::new();
        meta.insert("sample_rate".into(), "1000".into());
        meta.insert("instrument".into(), "TEST".into());
        let labels = vec!["ch1".into(), "ch2".into()];
        (time, vec![ch1, ch2], meta, labels)
    }

    #[test]
    fn csv_roundtrip() {
        let (time, data, meta, labels) = test_data();
        let path = std::env::temp_dir().join("valicore_test.csv");
        let result = save_csv(&path, &time, &data, &meta, &labels);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        // 2 metadata lines + 1 header + 3 data = 6
        assert_eq!(lines.len(), 6);
        assert!(lines[0].starts_with('#'));
        assert!(lines[1].starts_with('#'));
        assert_eq!(lines[2], "time,ch1,ch2");
        assert!(lines[3].contains("0,0,1"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn csv_single_channel() {
        let time = vec![0.0, 1.0, 2.0];
        let data = vec![vec![10.0, 20.0, 30.0]];
        let meta = HashMap::new();
        let labels = vec!["voltage".into()];
        let path = std::env::temp_dir().join("valicore_test_single.csv");
        save_csv(&path, &time, &data, &meta, &labels).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("time,voltage"));
        assert!(content.contains("0,10"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn csv_empty_metadata() {
        let time = vec![0.0];
        let data = vec![vec![1.0]];
        let path = std::env::temp_dir().join("valicore_test_nometa.csv");
        save_csv(&path, &time, &data, &HashMap::new(), &["x".into()]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("#"));
        let _ = std::fs::remove_file(&path);
    }
}
