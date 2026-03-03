#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
const RUN_KEY_PATH: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const RUN_VALUE_NAME: &str = "FanControlWindows";

pub fn get_launch_at_login_enabled() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("reg")
            .args(["query", RUN_KEY_PATH, "/v", RUN_VALUE_NAME])
            .output()
            .map_err(|error| format!("Failed to query startup registry key: {}", error))?;

        if output.status.success() {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

pub fn set_launch_at_login_enabled(enabled: bool) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        if enabled {
            let executable = std::env::current_exe()
                .map_err(|error| format!("Failed to resolve current executable path: {}", error))?;
            let data_value = format!("\"{}\"", executable.display());

            let output = Command::new("reg")
                .args([
                    "add",
                    RUN_KEY_PATH,
                    "/v",
                    RUN_VALUE_NAME,
                    "/t",
                    "REG_SZ",
                    "/d",
                    &data_value,
                    "/f",
                ])
                .output()
                .map_err(|error| format!("Failed to write startup registry key: {}", error))?;

            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
            }
        } else {
            let output = Command::new("reg")
                .args(["delete", RUN_KEY_PATH, "/v", RUN_VALUE_NAME, "/f"])
                .output()
                .map_err(|error| format!("Failed to remove startup registry key: {}", error))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

                if !stderr.to_ascii_lowercase().contains("unable to find") {
                    return Err(stderr);
                }
            }
        }

        get_launch_at_login_enabled()
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        Ok(false)
    }
}
