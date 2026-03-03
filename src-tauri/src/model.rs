use crate::provider::HardwareProvider;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const ALL_FANS_SELECTION_ID: i32 = -1;
const MANUAL_RETRY_ATTEMPTS: usize = 15;
const MANUAL_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const PRESET_STEP_RPM: i32 = 500;
const RPM_MATCH_TOLERANCE: f64 = 1.0;
const ERROR_DISPLAY_DURATION: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct Fan {
    pub id: i32,
    pub min_rpm: f64,
    pub max_rpm: f64,
    pub current_rpm: f64,
    pub target_rpm: f64,
    pub mode: u8,
    pub display_name: String,
}

impl Fan {
    fn mode_name(&self) -> &'static str {
        match self.mode {
            0 => "Auto",
            1 => "Manual",
            3 => "System",
            _ => "Unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TemperatureSensor {
    pub key: String,
    pub celsius: f64,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FanControlMode {
    Auto,
    Min,
    Max,
    Preset,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FanSnapshot {
    pub id: i32,
    pub min_rpm: f64,
    pub max_rpm: f64,
    pub current_rpm: f64,
    pub target_rpm: f64,
    pub mode: u8,
    pub display_name: String,
    pub mode_name: String,
}

impl From<&Fan> for FanSnapshot {
    fn from(value: &Fan) -> Self {
        Self {
            id: value.id,
            min_rpm: value.min_rpm,
            max_rpm: value.max_rpm,
            current_rpm: value.current_rpm,
            target_rpm: value.target_rpm,
            mode: value.mode,
            display_name: value.display_name.clone(),
            mode_name: value.mode_name().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemperatureSensorSnapshot {
    pub key: String,
    pub celsius: f64,
    pub display_name: String,
}

impl From<&TemperatureSensor> for TemperatureSensorSnapshot {
    fn from(value: &TemperatureSensor) -> Self {
        Self {
            key: value.key.clone(),
            celsius: value.celsius,
            display_name: value.display_name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub fans: Vec<FanSnapshot>,
    pub temperature_sensors: Vec<TemperatureSensorSnapshot>,
    pub selected_fan_id: i32,
    pub all_fans_id: i32,
    pub controls_all_fans: bool,
    pub selected_fan: Option<FanSnapshot>,
    pub is_any_fan_spinning: bool,
    pub control_min_rpm: Option<f64>,
    pub control_max_rpm: Option<f64>,
    pub control_preset_rpms: Vec<i32>,
    pub active_control_mode: Option<FanControlMode>,
    pub is_sending_control_attempts: bool,
    pub control_attempt_target_mode: Option<FanControlMode>,
    pub shows_control_attempt_progress: bool,
    pub error_text: Option<String>,
    pub processor_name: String,
    pub provider_name: String,
}

pub struct AppModel {
    provider: Box<dyn HardwareProvider>,
    provider_name: String,
    fans: Vec<Fan>,
    temperature_sensors: Vec<TemperatureSensor>,
    selected_fan_id: i32,
    is_sending_control_attempts: bool,
    control_attempt_target_mode: Option<FanControlMode>,
    error_text: Option<String>,
    error_expiry: Option<Instant>,
    processor_name: String,
    holding_manual_override: bool,
    control_action_token: u64,
    initial_notice: Option<String>,
    initialized: bool,
}

impl AppModel {
    pub async fn new(
        provider: Box<dyn HardwareProvider>,
        provider_name: String,
        initial_notice: Option<String>,
    ) -> Self {
        let processor_name = provider.processor_name().await;

        Self {
            provider,
            provider_name,
            fans: Vec::new(),
            temperature_sensors: Vec::new(),
            selected_fan_id: 0,
            is_sending_control_attempts: false,
            control_attempt_target_mode: None,
            error_text: None,
            error_expiry: None,
            processor_name,
            holding_manual_override: false,
            control_action_token: 0,
            initial_notice,
            initialized: false,
        }
    }

    pub async fn initialize(&mut self) -> AppSnapshot {
        if !self.initialized {
            self.initialized = true;
            self.refresh().await;

            if let Some(message) = self.initial_notice.take() {
                self.present_error(message);
            }
        }

        self.snapshot()
    }

    pub async fn tick(&mut self) -> AppSnapshot {
        self.clear_expired_error();

        if self.holding_manual_override {
            if let Err(error) = self.provider.keep_alive_manual_override().await {
                self.present_error(error);
            }
        }

        self.refresh().await;
        self.snapshot()
    }

    pub fn dismiss_error(&mut self) -> AppSnapshot {
        self.error_text = None;
        self.error_expiry = None;
        self.snapshot()
    }

    pub fn set_selected_fan(&mut self, selected_fan_id: i32) -> AppSnapshot {
        self.selected_fan_id = selected_fan_id;

        if self.fans.is_empty() {
            self.selected_fan_id = ALL_FANS_SELECTION_ID;
        } else if !self.controls_all_fans()
            && !self.fans.iter().any(|fan| fan.id == self.selected_fan_id)
        {
            self.selected_fan_id = self.fans[0].id;
        }

        self.snapshot()
    }

    pub async fn set_manual_rpm(&mut self, rpm: f64, target_mode: FanControlMode) -> AppSnapshot {
        self.apply_manual_rpm(target_mode, |_| rpm).await;
        self.snapshot()
    }

    pub async fn set_control_min(&mut self) -> AppSnapshot {
        if self.controls_all_fans() {
            self.apply_manual_rpm(FanControlMode::Min, |fan| fan.min_rpm)
                .await;
        } else if self.control_min_rpm().is_some() {
            self.apply_manual_rpm(FanControlMode::Min, |fan| fan.min_rpm)
                .await;
        }

        self.snapshot()
    }

    pub async fn set_control_max(&mut self) -> AppSnapshot {
        if self.controls_all_fans() {
            self.apply_manual_rpm(FanControlMode::Max, |fan| fan.max_rpm)
                .await;
        } else if self.control_max_rpm().is_some() {
            self.apply_manual_rpm(FanControlMode::Max, |fan| fan.max_rpm)
                .await;
        }

        self.snapshot()
    }

    pub async fn set_auto(&mut self) -> AppSnapshot {
        self.clear_expired_error();

        self.start_control_action();

        let target_fans = self.selected_fans_for_control();
        if target_fans.is_empty() {
            return self.snapshot();
        }

        let mut successful_signals = 0usize;
        let mut last_error: Option<String> = None;

        for fan in target_fans {
            match self.provider.set_fan_auto(fan.id).await {
                Ok(_) => successful_signals += 1,
                Err(error) => last_error = Some(error),
            }
        }

        if successful_signals == 0 {
            if let Some(error) = last_error {
                self.present_error(error);
            }

            return self.snapshot();
        }

        self.holding_manual_override = false;
        self.refresh().await;
        self.snapshot()
    }

    pub fn snapshot(&mut self) -> AppSnapshot {
        self.clear_expired_error();

        let fans: Vec<FanSnapshot> = self.fans.iter().map(FanSnapshot::from).collect();
        let selected_fan = self.selected_fan().map(|fan| FanSnapshot::from(fan));
        let temperature_sensors = self
            .temperature_sensors
            .iter()
            .map(TemperatureSensorSnapshot::from)
            .collect();

        let active_control_mode = self.active_control_mode();
        let shows_control_attempt_progress = if !self.is_sending_control_attempts {
            false
        } else if let Some(target_mode) = self.control_attempt_target_mode {
            active_control_mode != Some(target_mode)
        } else {
            true
        };

        AppSnapshot {
            fans,
            temperature_sensors,
            selected_fan_id: self.selected_fan_id,
            all_fans_id: ALL_FANS_SELECTION_ID,
            controls_all_fans: self.controls_all_fans(),
            selected_fan,
            is_any_fan_spinning: self.fans.iter().any(|fan| fan.current_rpm > 0.0),
            control_min_rpm: self.control_min_rpm(),
            control_max_rpm: self.control_max_rpm(),
            control_preset_rpms: self.control_preset_rpms(),
            active_control_mode,
            is_sending_control_attempts: self.is_sending_control_attempts,
            control_attempt_target_mode: self.control_attempt_target_mode,
            shows_control_attempt_progress,
            error_text: self.error_text.clone(),
            processor_name: self.processor_name.clone(),
            provider_name: self.provider_name.clone(),
        }
    }

    pub fn debug_text(&self) -> String {
        let sensor_lines = self
            .temperature_sensors
            .iter()
            .map(|sensor| format!("{}: {:.1}", sensor.key, sensor.celsius,));

        std::iter::once(self.processor_name.clone())
            .chain(std::iter::once(String::new()))
            .chain(sensor_lines)
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn refresh(&mut self) {
        self.clear_expired_error();

        match self.provider.read_fans().await {
            Ok(fans) => self.fans = fans,
            Err(error) => {
                self.present_error(error);
                self.temperature_sensors.clear();
                return;
            }
        }

        match self.provider.read_temperature_sensors().await {
            Ok(sensors) => self.temperature_sensors = sensors,
            Err(error) => {
                self.present_error(error);
                self.temperature_sensors.clear();
            }
        }

        if self.fans.is_empty() {
            self.selected_fan_id = ALL_FANS_SELECTION_ID;
        } else if !self.controls_all_fans()
            && !self.fans.iter().any(|fan| fan.id == self.selected_fan_id)
        {
            self.selected_fan_id = self.fans[0].id;
        }
    }

    async fn apply_manual_rpm<F>(&mut self, target_mode: FanControlMode, rpm_for_fan: F)
    where
        F: Fn(&Fan) -> f64,
    {
        self.clear_expired_error();

        let action_token = self.start_control_action();
        let target_fans = self.selected_fans_for_control();

        if target_fans.is_empty() {
            return;
        }

        self.begin_control_attempt_progress(target_mode);

        let mut successful_signals = 0usize;
        let mut last_error: Option<String> = None;

        for attempt in 1..=MANUAL_RETRY_ATTEMPTS {
            if !self.is_control_action_current(action_token) {
                self.end_control_attempt_progress();
                return;
            }

            for fan in &target_fans {
                let rpm = rpm_for_fan(fan);

                match self.provider.set_fan_manual_rpm(fan.id, rpm).await {
                    Ok(_) => successful_signals += 1,
                    Err(error) => last_error = Some(error),
                }
            }

            if attempt < MANUAL_RETRY_ATTEMPTS {
                tokio::time::sleep(MANUAL_RETRY_INTERVAL).await;
            }
        }

        if !self.is_control_action_current(action_token) {
            self.end_control_attempt_progress();
            return;
        }

        self.end_control_attempt_progress();

        if successful_signals == 0 {
            if let Some(error) = last_error {
                self.present_error(error);
            }

            return;
        }

        self.holding_manual_override = true;
        self.refresh().await;
    }

    fn selected_fan(&self) -> Option<&Fan> {
        if self.controls_all_fans() {
            return None;
        }

        self.fans.iter().find(|fan| fan.id == self.selected_fan_id)
    }

    fn selected_fans_for_control(&self) -> Vec<Fan> {
        if self.controls_all_fans() {
            return self.fans.clone();
        }

        self.selected_fan().cloned().into_iter().collect()
    }

    fn controls_all_fans(&self) -> bool {
        self.selected_fan_id == ALL_FANS_SELECTION_ID
    }

    fn control_min_rpm(&self) -> Option<f64> {
        let target_fans = self.selected_fans_for_control();

        if target_fans.is_empty() {
            return None;
        }

        if self.controls_all_fans() {
            target_fans.iter().map(|fan| fan.min_rpm).reduce(f64::max)
        } else {
            target_fans.first().map(|fan| fan.min_rpm)
        }
    }

    fn control_max_rpm(&self) -> Option<f64> {
        let target_fans = self.selected_fans_for_control();

        if target_fans.is_empty() {
            return None;
        }

        if self.controls_all_fans() {
            target_fans.iter().map(|fan| fan.max_rpm).reduce(f64::min)
        } else {
            target_fans.first().map(|fan| fan.max_rpm)
        }
    }

    fn control_preset_rpms(&self) -> Vec<i32> {
        let (Some(min_rpm), Some(max_rpm)) = (self.control_min_rpm(), self.control_max_rpm())
        else {
            return Vec::new();
        };

        let start = ((min_rpm / PRESET_STEP_RPM as f64).ceil() as i32) * PRESET_STEP_RPM;
        let end = ((max_rpm / PRESET_STEP_RPM as f64).floor() as i32) * PRESET_STEP_RPM;

        if start > end {
            return Vec::new();
        }

        (start..=end).step_by(PRESET_STEP_RPM as usize).collect()
    }

    fn active_control_mode(&self) -> Option<FanControlMode> {
        let target_fans = self.selected_fans_for_control();

        if target_fans.is_empty() {
            return None;
        }

        if self.controls_all_fans() {
            return self.active_control_mode_for_all_fans(&target_fans);
        }

        let preset_rpms: Vec<f64> = self
            .control_preset_rpms()
            .into_iter()
            .map(f64::from)
            .collect();

        let fan_modes: Vec<FanControlMode> = target_fans
            .iter()
            .filter_map(|fan| self.active_control_mode_for_fan(fan, &preset_rpms))
            .collect();

        if fan_modes.len() != target_fans.len() {
            return None;
        }

        let first = fan_modes.first().copied()?;

        if fan_modes.iter().all(|mode| *mode == first) {
            Some(first)
        } else {
            None
        }
    }

    fn active_control_mode_for_fan(
        &self,
        fan: &Fan,
        preset_rpms: &[f64],
    ) -> Option<FanControlMode> {
        if (fan.mode == 0 || fan.mode == 3) && !self.holding_manual_override {
            return Some(FanControlMode::Auto);
        }

        if Self::rpm_matches(fan.target_rpm, fan.max_rpm) {
            return Some(FanControlMode::Max);
        }

        if Self::rpm_matches(fan.target_rpm, fan.min_rpm) {
            return Some(FanControlMode::Min);
        }

        if preset_rpms
            .iter()
            .any(|preset| Self::rpm_matches(fan.target_rpm, *preset))
        {
            return Some(FanControlMode::Preset);
        }

        None
    }

    fn active_control_mode_for_all_fans(&self, fans: &[Fan]) -> Option<FanControlMode> {
        if fans
            .iter()
            .all(|fan| (fan.mode == 0 || fan.mode == 3) && !self.holding_manual_override)
        {
            return Some(FanControlMode::Auto);
        }

        if fans
            .iter()
            .all(|fan| Self::rpm_matches(fan.target_rpm, fan.max_rpm))
        {
            return Some(FanControlMode::Max);
        }

        if fans
            .iter()
            .all(|fan| Self::rpm_matches(fan.target_rpm, fan.min_rpm))
        {
            return Some(FanControlMode::Min);
        }

        let target_rpm = fans.first()?.target_rpm;

        if !fans
            .iter()
            .all(|fan| Self::rpm_matches(fan.target_rpm, target_rpm))
        {
            return None;
        }

        let preset_rpms: Vec<f64> = self
            .control_preset_rpms()
            .into_iter()
            .map(f64::from)
            .collect();

        if preset_rpms
            .iter()
            .any(|preset| Self::rpm_matches(target_rpm, *preset))
        {
            return Some(FanControlMode::Preset);
        }

        None
    }

    fn rpm_matches(lhs: f64, rhs: f64) -> bool {
        (lhs - rhs).abs() <= RPM_MATCH_TOLERANCE
    }

    fn start_control_action(&mut self) -> u64 {
        self.control_action_token = self.control_action_token.saturating_add(1);
        self.end_control_attempt_progress();
        self.control_action_token
    }

    fn is_control_action_current(&self, token: u64) -> bool {
        self.control_action_token == token
    }

    fn begin_control_attempt_progress(&mut self, target_mode: FanControlMode) {
        self.is_sending_control_attempts = true;
        self.control_attempt_target_mode = Some(target_mode);
    }

    fn end_control_attempt_progress(&mut self) {
        self.is_sending_control_attempts = false;
        self.control_attempt_target_mode = None;
    }

    fn present_error(&mut self, message: String) {
        let now = Instant::now();

        if self.error_text.as_deref() == Some(message.as_str())
            && self.error_expiry.is_some_and(|expiry| now < expiry)
        {
            return;
        }

        self.error_text = Some(message);
        self.error_expiry = Some(now + ERROR_DISPLAY_DURATION);
    }

    fn clear_expired_error(&mut self) {
        if self
            .error_expiry
            .is_some_and(|expiry| Instant::now() >= expiry)
        {
            self.error_text = None;
            self.error_expiry = None;
        }
    }
}
