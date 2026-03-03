use crate::model::{Fan, TemperatureSensor};
use async_trait::async_trait;
use std::sync::Mutex;
use sysinfo::System;

#[cfg(target_os = "windows")]
use serde::de::DeserializeOwned;
#[cfg(target_os = "windows")]
use serde::Deserialize;
#[cfg(target_os = "windows")]
use serde_json::Value;
#[cfg(target_os = "windows")]
use std::collections::HashMap;

pub struct ProviderBootstrap {
    pub provider: Box<dyn HardwareProvider>,
    pub provider_name: String,
    pub initial_notice: Option<String>,
}

#[async_trait]
pub trait HardwareProvider: Send + Sync {
    async fn read_fans(&self) -> Result<Vec<Fan>, String>;
    async fn read_temperature_sensors(&self) -> Result<Vec<TemperatureSensor>, String>;
    async fn set_fan_manual_rpm(&self, fan_id: i32, rpm: f64) -> Result<(), String>;
    async fn set_fan_auto(&self, fan_id: i32) -> Result<(), String>;
    async fn keep_alive_manual_override(&self) -> Result<(), String>;
    async fn processor_name(&self) -> String;
}

pub async fn create_default_provider() -> ProviderBootstrap {
    #[cfg(target_os = "windows")]
    {
        match LibreHardwareMonitorProvider::try_new().await {
            Ok(provider) => ProviderBootstrap {
                provider: Box::new(provider),
                provider_name: "LibreHardwareMonitor".to_string(),
                initial_notice: None,
            },
            Err(error) => ProviderBootstrap {
                provider: Box::new(MockProvider::new()),
                provider_name: "Simulation".to_string(),
                initial_notice: Some(format!(
                    "LibreHardwareMonitor WMI not available, using simulation mode\n{}",
                    error
                )),
            },
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        ProviderBootstrap {
            provider: Box::new(MockProvider::new()),
            provider_name: "Simulation".to_string(),
            initial_notice: Some(
                "Windows hardware provider unavailable on this OS, using simulation mode"
                    .to_string(),
            ),
        }
    }
}

struct MockProvider {
    state: Mutex<MockState>,
}

struct MockState {
    fans: Vec<Fan>,
    sensors: Vec<TemperatureSensor>,
    phase: f64,
}

impl MockProvider {
    fn new() -> Self {
        Self {
            state: Mutex::new(MockState {
                fans: vec![
                    Fan {
                        id: 0,
                        min_rpm: 1200.0,
                        max_rpm: 5200.0,
                        current_rpm: 2100.0,
                        target_rpm: 2100.0,
                        mode: 0,
                        display_name: "Fan 0".to_string(),
                    },
                    Fan {
                        id: 1,
                        min_rpm: 1100.0,
                        max_rpm: 5000.0,
                        current_rpm: 1950.0,
                        target_rpm: 1950.0,
                        mode: 0,
                        display_name: "Fan 1".to_string(),
                    },
                ],
                sensors: vec![
                    TemperatureSensor {
                        key: "cpu-package".to_string(),
                        celsius: 46.0,
                        display_name: "CPU Package".to_string(),
                    },
                    TemperatureSensor {
                        key: "cpu-core-1".to_string(),
                        celsius: 44.0,
                        display_name: "CPU Core 1".to_string(),
                    },
                    TemperatureSensor {
                        key: "gpu-core".to_string(),
                        celsius: 50.0,
                        display_name: "GPU Core".to_string(),
                    },
                    TemperatureSensor {
                        key: "battery".to_string(),
                        celsius: 34.0,
                        display_name: "Battery".to_string(),
                    },
                    TemperatureSensor {
                        key: "vrm".to_string(),
                        celsius: 40.0,
                        display_name: "VRM".to_string(),
                    },
                ],
                phase: 0.0,
            }),
        }
    }
}

#[async_trait]
impl HardwareProvider for MockProvider {
    async fn read_fans(&self) -> Result<Vec<Fan>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Simulation provider state lock failed".to_string())?;

        state.phase += 0.18;
        let phase = state.phase;

        for fan in &mut state.fans {
            if fan.mode == 0 {
                let center = fan.min_rpm + (fan.max_rpm - fan.min_rpm) * 0.55;
                let wave = phase.sin() * 240.0;
                fan.target_rpm = (center + wave).clamp(fan.min_rpm, fan.max_rpm);
            }

            let drift = (fan.target_rpm - fan.current_rpm) * 0.32;
            fan.current_rpm = (fan.current_rpm + drift).clamp(fan.min_rpm, fan.max_rpm);
        }

        Ok(state.fans.clone())
    }

    async fn read_temperature_sensors(&self) -> Result<Vec<TemperatureSensor>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Simulation provider state lock failed".to_string())?;

        let average_fan_rpm = if state.fans.is_empty() {
            1800.0
        } else {
            state.fans.iter().map(|fan| fan.current_rpm).sum::<f64>() / state.fans.len() as f64
        };

        let cooling_bias = ((average_fan_rpm - 1800.0) / 1800.0).clamp(-0.3, 0.3);
        let phase = state.phase;

        for (index, sensor) in state.sensors.iter_mut().enumerate() {
            let wave = ((phase * 0.9) + index as f64).sin() * 0.8;
            let baseline = match index {
                0 => 48.0,
                1 => 45.0,
                2 => 52.0,
                3 => 35.0,
                _ => 42.0,
            };

            sensor.celsius = (baseline + wave - (cooling_bias * 4.5)).clamp(20.0, 98.0);
        }

        Ok(state.sensors.clone())
    }

    async fn set_fan_manual_rpm(&self, fan_id: i32, rpm: f64) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Simulation provider state lock failed".to_string())?;

        let Some(fan) = state.fans.iter_mut().find(|fan| fan.id == fan_id) else {
            return Err(format!("Fan {} not found", fan_id));
        };

        fan.mode = 1;
        fan.target_rpm = rpm.clamp(fan.min_rpm, fan.max_rpm);
        Ok(())
    }

    async fn set_fan_auto(&self, fan_id: i32) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Simulation provider state lock failed".to_string())?;

        let Some(fan) = state.fans.iter_mut().find(|fan| fan.id == fan_id) else {
            return Err(format!("Fan {} not found", fan_id));
        };

        fan.mode = 0;
        fan.target_rpm = fan.min_rpm + ((fan.max_rpm - fan.min_rpm) * 0.5);
        Ok(())
    }

    async fn keep_alive_manual_override(&self) -> Result<(), String> {
        Ok(())
    }

    async fn processor_name(&self) -> String {
        detect_processor_name()
    }
}

#[cfg(target_os = "windows")]
struct LibreHardwareMonitorProvider {
    state: Mutex<WindowsProviderState>,
}

#[cfg(target_os = "windows")]
#[derive(Default)]
struct WindowsProviderState {
    fan_targets: HashMap<i32, f64>,
    fan_modes: HashMap<i32, u8>,
    last_fans: Vec<Fan>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct LhmFanSensor {
    identifier: String,
    #[serde(default)]
    name: String,
    value: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct LhmTemperatureSensor {
    identifier: String,
    #[serde(default)]
    name: String,
    value: Option<f64>,
}

#[cfg(target_os = "windows")]
impl LibreHardwareMonitorProvider {
    async fn try_new() -> Result<Self, String> {
        let provider = Self {
            state: Mutex::new(WindowsProviderState::default()),
        };

        let fans = provider.read_fan_sensors().await?;
        if fans.is_empty() {
            return Err(
                "No fan sensors found in root\\LibreHardwareMonitor namespace\nMake sure LibreHardwareMonitor is running with WMI enabled"
                    .to_string(),
            );
        }

        Ok(provider)
    }

    async fn read_fan_sensors(&self) -> Result<Vec<LhmFanSensor>, String> {
        let script = r#"
$ErrorActionPreference='Stop'
Get-CimInstance -Namespace root\LibreHardwareMonitor -ClassName Sensor |
Where-Object { $_.SensorType -eq 'Fan' } |
Sort-Object Identifier |
ForEach-Object {
  [PSCustomObject]@{
    identifier = [string]$_.Identifier
    name = [string]$_.Name
    value = if ($null -eq $_.Value) { $null } else { [double]$_.Value }
    min = if ($null -eq $_.Min) { $null } else { [double]$_.Min }
    max = if ($null -eq $_.Max) { $null } else { [double]$_.Max }
  }
} |
ConvertTo-Json -Depth 4 -Compress
"#;

        let output = run_powershell(script).await?;
        parse_json_vec::<LhmFanSensor>(&output)
    }

    async fn read_temperature_sensor_rows(&self) -> Result<Vec<LhmTemperatureSensor>, String> {
        let script = r#"
$ErrorActionPreference='Stop'
Get-CimInstance -Namespace root\LibreHardwareMonitor -ClassName Sensor |
Where-Object { $_.SensorType -eq 'Temperature' } |
Sort-Object Identifier |
ForEach-Object {
  [PSCustomObject]@{
    identifier = [string]$_.Identifier
    name = [string]$_.Name
    value = if ($null -eq $_.Value) { $null } else { [double]$_.Value }
  }
} |
ConvertTo-Json -Depth 4 -Compress
"#;

        let output = run_powershell(script).await?;
        parse_json_vec::<LhmTemperatureSensor>(&output)
    }

    async fn set_control_value(&self, fan_id: i32, rpm: Option<f64>) -> Result<(), String> {
        let mode = if rpm.is_some() { 1 } else { 0 };
        let software_value = rpm.unwrap_or_default();

        let script = format!(
            r#"
$ErrorActionPreference='Stop'
$fanIndex = {fan_id}
$mode = {mode}
$softwareValue = {software_value}
$controls = @(Get-CimInstance -Namespace root\LibreHardwareMonitor -ClassName Control | Sort-Object Identifier)
if ($controls.Count -le $fanIndex) {{
  throw "No fan control found for fan index $fanIndex"
}}
$control = $controls[$fanIndex]
if ($control.PSObject.Properties.Name -contains 'ControlMode') {{
  $control.ControlMode = $mode
}}
if ($mode -eq 1) {{
  if ($control.PSObject.Properties.Name -contains 'SoftwareValue') {{
    $control.SoftwareValue = $softwareValue
  }} elseif ($control.PSObject.Properties.Name -contains 'Control') {{
    $control.Control = $softwareValue
  }}
}}
Set-CimInstance -InputObject $control | Out-Null
"#
        );

        run_powershell(&script).await.map(|_| ()).map_err(|error| {
            format!(
                "Failed to set fan control via LibreHardwareMonitor\n{}\nIf this persists, verify your board exposes writable fan controls",
                error
            )
        })
    }
}

#[cfg(target_os = "windows")]
#[async_trait]
impl HardwareProvider for LibreHardwareMonitorProvider {
    async fn read_fans(&self) -> Result<Vec<Fan>, String> {
        let rows = self.read_fan_sensors().await?;

        let mut state = self
            .state
            .lock()
            .map_err(|_| "Windows provider state lock failed".to_string())?;

        let fans: Vec<Fan> = rows
            .into_iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let current_rpm = row.value?;
                let id = index as i32;
                let min_rpm = row.min.unwrap_or(700.0).max(0.0);
                let mut max_rpm = row.max.unwrap_or(5200.0).max(min_rpm + 100.0);

                if max_rpm <= min_rpm {
                    max_rpm = min_rpm + 500.0;
                }

                let mode = state.fan_modes.get(&id).copied().unwrap_or(0);
                let target_rpm = state
                    .fan_targets
                    .get(&id)
                    .copied()
                    .unwrap_or(current_rpm)
                    .clamp(min_rpm, max_rpm);

                let name = if row.name.trim().is_empty() {
                    format!("Fan {}", id)
                } else {
                    row.name
                };

                Some(Fan {
                    id,
                    min_rpm,
                    max_rpm,
                    current_rpm,
                    target_rpm,
                    mode,
                    display_name: if name.contains("Fan") {
                        name
                    } else {
                        format!("{} ({})", name, row.identifier)
                    },
                })
            })
            .collect();

        state.last_fans = fans.clone();
        Ok(fans)
    }

    async fn read_temperature_sensors(&self) -> Result<Vec<TemperatureSensor>, String> {
        let rows = self.read_temperature_sensor_rows().await?;

        let sensors: Vec<TemperatureSensor> = rows
            .into_iter()
            .filter_map(|row| {
                let celsius = row.value?;
                if !(-20.0..=130.0).contains(&celsius) {
                    return None;
                }

                let display_name = if row.name.trim().is_empty() {
                    row.identifier.clone()
                } else {
                    row.name
                };

                Some(TemperatureSensor {
                    key: row.identifier,
                    celsius,
                    display_name,
                })
            })
            .collect();

        Ok(sensors)
    }

    async fn set_fan_manual_rpm(&self, fan_id: i32, rpm: f64) -> Result<(), String> {
        let clamped_rpm = {
            let state = self
                .state
                .lock()
                .map_err(|_| "Windows provider state lock failed".to_string())?;

            let Some(fan) = state.last_fans.iter().find(|fan| fan.id == fan_id) else {
                return Err(format!("Fan {} not found", fan_id));
            };

            rpm.clamp(fan.min_rpm, fan.max_rpm)
        };

        self.set_control_value(fan_id, Some(clamped_rpm)).await?;

        let mut state = self
            .state
            .lock()
            .map_err(|_| "Windows provider state lock failed".to_string())?;

        state.fan_targets.insert(fan_id, clamped_rpm);
        state.fan_modes.insert(fan_id, 1);

        Ok(())
    }

    async fn set_fan_auto(&self, fan_id: i32) -> Result<(), String> {
        self.set_control_value(fan_id, None).await?;

        let mut state = self
            .state
            .lock()
            .map_err(|_| "Windows provider state lock failed".to_string())?;

        state.fan_modes.insert(fan_id, 0);
        state.fan_targets.remove(&fan_id);

        Ok(())
    }

    async fn keep_alive_manual_override(&self) -> Result<(), String> {
        Ok(())
    }

    async fn processor_name(&self) -> String {
        detect_processor_name()
    }
}

#[cfg(target_os = "windows")]
async fn run_powershell(script: &str) -> Result<String, String> {
    use std::process::Command;

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| format!("Failed to launch PowerShell: {}", error))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(if stderr.is_empty() {
            if stdout.is_empty() {
                "PowerShell command failed without output".to_string()
            } else {
                stdout
            }
        } else {
            stderr
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "windows")]
fn parse_json_vec<T>(raw: &str) -> Result<Vec<T>, String>
where
    T: DeserializeOwned,
{
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    let value: Value = serde_json::from_str(raw)
        .map_err(|error| format!("Failed to parse JSON output: {}", error))?;

    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => items
            .into_iter()
            .map(serde_json::from_value::<T>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to decode JSON array entry: {}", error)),
        single => serde_json::from_value::<T>(single)
            .map(|item| vec![item])
            .map_err(|error| format!("Failed to decode JSON object: {}", error)),
    }
}

fn detect_processor_name() -> String {
    let system = System::new_all();

    if let Some(cpu) = system.cpus().first() {
        let brand = cpu.brand().trim();
        if !brand.is_empty() {
            return brand.to_string();
        }
    }

    let architecture = std::env::consts::ARCH;
    format!("Unknown processor ({})", architecture)
}
