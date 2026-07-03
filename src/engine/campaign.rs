use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct TestCampaign {
    pub title: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub instruments: HashMap<String, InstrumentConfig>,
    pub groups: HashMap<String, TestGroup>,
    pub variables: Option<HashMap<String, serde_yaml::Value>>,
    pub output: Option<HashMap<String, serde_yaml::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstrumentConfig {
    pub kind: String,
    pub resource: String,
    pub timeout: Option<u64>,
    pub options: Option<HashMap<String, serde_yaml::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestGroup {
    #[serde(default)]
    pub name: String,
    pub description: Option<String>,
    pub setup: Option<String>,
    pub teardown: Option<String>,
    pub steps: Vec<TestStep>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestStep {
    pub name: String,
    pub description: Option<String>,
    pub instrument: String,
    pub command: Option<String>,
    #[serde(default)]
    pub measurements: Vec<MeasurementDef>,
    #[serde(default = "default_repeat")]
    pub repeat: u32,
    #[serde(default)]
    pub delay_ms: u64,
}

fn default_repeat() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct MeasurementDef {
    pub name: String,
    #[serde(rename = "type")]
    pub meas_type: String,
    pub channel: Option<String>,
    pub limits: Option<Vec<Limit>>,
    pub options: Option<HashMap<String, serde_yaml::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Limit {
    pub op: String,
    pub value: f64,
    pub tolerance: Option<f64>,
}

impl TestCampaign {
    pub fn from_yaml(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read campaign file '{}': {}", path, e))?;
        let mut data: serde_yaml::Value = serde_yaml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("failed to parse YAML: {}", e))?;

        if let Some(instruments) = data.get_mut("instruments").and_then(|v| v.as_mapping_mut()) {
            for (key, val) in instruments.clone().iter() {
                if let Some(cfg) = val.as_mapping() {
                    if !cfg.contains_key("name") {
                        if let Some(m) = instruments.get_mut(key) {
                            if let Some(m_map) = m.as_mapping_mut() {
                                m_map.insert("name".into(), key.clone());
                            }
                        }
                    }
                }
            }
        }
        if let Some(groups) = data.get_mut("groups").and_then(|v| v.as_mapping_mut()) {
            for (key, val) in groups.clone().iter() {
                if let Some(g) = val.as_mapping() {
                    if !g.contains_key("name") {
                        if let Some(m) = groups.get_mut(key) {
                            if let Some(g_map) = m.as_mapping_mut() {
                                g_map.insert("name".into(), key.clone());
                            }
                        }
                    }
                }
            }
        }

        let campaign: TestCampaign = serde_yaml::from_value(data)
            .map_err(|e| anyhow::anyhow!("campaign validation failed: {}", e))?;
        Ok(campaign)
    }

    pub fn total_steps(&self) -> usize {
        self.groups.values().map(|g| g.steps.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_yaml() -> &'static str {
        r#"
title: "Test Campaign"
version: "1.0"
instruments:
  dmm:
    kind: keysight_34460a
    resource: "TCPIP0::192.168.1.100::inst0::INSTR"
groups:
  smoke:
    steps:
      - name: "Check voltage"
        instrument: dmm
        measurements:
          - name: "Vout"
            type: "volt:dc"
            limits:
              - op: within
                value: 3.3
                tolerance: 0.1
"#
    }

    #[test]
    fn test_parse_campaign() {
        let data: serde_yaml::Value = serde_yaml::from_str(sample_yaml()).unwrap();
        let campaign: TestCampaign = serde_yaml::from_value(data).unwrap();
        assert_eq!(campaign.title, "Test Campaign");
        assert_eq!(campaign.instruments.len(), 1);
        assert!(campaign.instruments.contains_key("dmm"));
        assert_eq!(campaign.groups.len(), 1);
        let group = campaign.groups.get("smoke").unwrap();
        assert_eq!(group.steps.len(), 1);
        assert_eq!(group.steps[0].name, "Check voltage");
        assert_eq!(group.steps[0].measurements.len(), 1);
        assert_eq!(group.steps[0].measurements[0].name, "Vout");
    }

    #[test]
    fn test_total_steps() {
        let data: serde_yaml::Value = serde_yaml::from_str(sample_yaml()).unwrap();
        let campaign: TestCampaign = serde_yaml::from_value(data).unwrap();
        assert_eq!(campaign.total_steps(), 1);
    }

    #[test]
    fn test_from_yaml() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_campaign.yaml");
        std::fs::write(&path, sample_yaml()).unwrap();
        let campaign = TestCampaign::from_yaml(path.to_str().unwrap()).unwrap();
        assert_eq!(campaign.title, "Test Campaign");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_instrument_config_name_injected() {
        let yaml = r#"
title: "Test"
instruments:
  my_dmm:
    kind: keysight_34460a
    resource: "TCPIP0::1.2.3.4::inst0::INSTR"
groups:
  g:
    steps:
      - name: s
        instrument: my_dmm
"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test_instr_campaign.yaml");
        std::fs::write(&path, yaml).unwrap();
        let campaign = TestCampaign::from_yaml(path.to_str().unwrap()).unwrap();
        let instr = campaign.instruments.get("my_dmm").unwrap();
        assert_eq!(instr.kind, "keysight_34460a");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_deserialize_with_group_name() {
        let yaml = r#"
title: "T"
instruments:
  a:
    kind: b
    resource: "c"
groups:
  g1:
    name: "Group One"
    steps:
      - name: s1
        instrument: a
"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test_group_name.yaml");
        std::fs::write(&path, yaml).unwrap();
        let campaign = TestCampaign::from_yaml(path.to_str().unwrap()).unwrap();
        let g = campaign.groups.get("g1").unwrap();
        assert_eq!(g.name, "Group One");
        std::fs::remove_file(&path).unwrap();
    }
}
