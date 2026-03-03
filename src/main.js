const { invoke } = window.__TAURI__.core
const tauriWindow = window.__TAURI__.window?.getCurrentWindow?.()
const opener = window.__TAURI__.opener

const SETTINGS_KEY = "fancontrol.settings"

const defaultSettings = {
  temperatureUnit: "celsius",
  temperaturePrecision: "whole",
  hideWindowOnLaunch: false,
}

let settings = loadSettings()
let appSnapshot = null
let frontendError = null
let launchAtLoginEnabled = false
let settingsBound = false
let tickerHandle = null
let tickInFlight = false
let showsMoreSensors = false
let selectedPresetRpm = null

function loadSettings() {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY)
    if (!raw) {
      return { ...defaultSettings }
    }

    const parsed = JSON.parse(raw)
    return {
      ...defaultSettings,
      ...parsed,
    }
  } catch {
    return { ...defaultSettings }
  }
}

function saveSettings() {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings))
}

function htmlEscape(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;")
}

function rpmText(value) {
  return `${Math.round(value)} RPM`
}

function formatTemperature(celsius) {
  const digits = settings.temperaturePrecision === "tenths" ? 1 : 0

  if (settings.temperatureUnit === "fahrenheit") {
    return `${((celsius * 9) / 5 + 32).toFixed(digits)} °F`
  }

  if (settings.temperatureUnit === "kelvin") {
    return `${(celsius + 273.15).toFixed(digits)} K`
  }

  return `${celsius.toFixed(digits)} °C`
}

function averageRows(sensors) {
  const categories = [
    { id: "battery", title: "BATTERY" },
    { id: "cpu", title: "CPU" },
    { id: "gpu", title: "GPU" },
  ]

  return categories.map((category) => {
    const values = sensors
      .filter((sensor) => sensor.displayName.toLowerCase().includes(category.id))
      .map((sensor) => sensor.celsius)

    if (values.length === 0) {
      return { title: category.title, value: "--" }
    }

    const average = values.reduce((acc, value) => acc + value, 0) / values.length
    return { title: category.title, value: formatTemperature(average) }
  })
}

function otherSensors(sensors) {
  return sensors.filter((sensor) => {
    const normalized = sensor.displayName.toLowerCase()
    return !normalized.includes("battery") && !normalized.includes("cpu") && !normalized.includes("gpu")
  })
}

function renderMetricRows(rows) {
  return rows
    .map(
      (row) => `
      <div class="metric-row">
        <span class="metric-title">${htmlEscape(row.title)}</span>
        <span class="metric-value mono">${htmlEscape(row.value)}</span>
      </div>
    `,
    )
    .join("")
}

function modeButtonClass(activeMode, buttonMode) {
  return activeMode === buttonMode ? "primary-button" : "secondary-button"
}

function renderControlCard(snapshot) {
  const canSetManual = snapshot.controlMinRpm !== null && snapshot.controlMaxRpm !== null
  const presets = snapshot.controlPresetRpms || []

  if (!selectedPresetRpm || !presets.includes(selectedPresetRpm)) {
    selectedPresetRpm = presets.length > 0 ? presets[0] : null
  }

  const presetLabel = snapshot.activeControlMode === "preset" ? "Presets" : "Preset"

  return `
    <section class="card">
      <div class="card-title-row">
        <h3>Control</h3>
        ${snapshot.showsControlAttemptProgress ? '<span class="pulse" aria-label="Sending control updates"></span>' : ""}
      </div>

      <div class="action-row">
        <button id="set-auto" class="${modeButtonClass(snapshot.activeControlMode, "auto")}">Auto</button>
        <div class="preset-group">
          <select id="preset-rpm" ${presets.length === 0 ? "disabled" : ""}>
            ${presets
              .map(
                (rpm) =>
                  `<option value="${rpm}" ${rpm === selectedPresetRpm ? "selected" : ""}>${rpm} RPM</option>`,
              )
              .join("")}
          </select>
          <button id="set-preset" class="${modeButtonClass(snapshot.activeControlMode, "preset")}" ${presets.length === 0 ? "disabled" : ""}>${presetLabel}</button>
        </div>
      </div>

      <div class="action-row">
        <button id="set-min" class="${modeButtonClass(snapshot.activeControlMode, "min")}" ${canSetManual ? "" : "disabled"}>Min</button>
        <button id="set-max" class="${modeButtonClass(snapshot.activeControlMode, "max")}" ${canSetManual ? "" : "disabled"}>Max</button>
      </div>
    </section>
  `
}

function renderFanDetails(snapshot) {
  if (snapshot.controlsAllFans) {
    const rows = snapshot.fans.map((fan) => ({
      title: fan.displayName,
      value: rpmText(fan.currentRpm),
    }))

    return `
      <section class="card">
        <h3>Current speed</h3>
        ${renderMetricRows(rows)}
      </section>
    `
  }

  if (!snapshot.selectedFan) {
    return ""
  }

  const fan = snapshot.selectedFan
  const rows = [
    { title: "Mode", value: fan.modeName },
    { title: "Current", value: rpmText(fan.currentRpm) },
    { title: "Target", value: rpmText(fan.targetRpm) },
    { title: "Min", value: rpmText(fan.minRpm) },
    { title: "Max", value: rpmText(fan.maxRpm) },
  ]

  return `
    <section class="card">
      <h3>Status</h3>
      ${renderMetricRows(rows)}
    </section>
  `
}

function renderSensors(snapshot) {
  const rows = averageRows(snapshot.temperatureSensors)
  const moreSensors = otherSensors(snapshot.temperatureSensors)

  return `
    <section class="card">
      <div class="card-title-row">
        <h3>Sensors</h3>
        ${
          moreSensors.length > 0
            ? `<button id="toggle-sensors" class="text-button">${showsMoreSensors ? "Show less" : "Show more"}</button>`
            : ""
        }
      </div>
      ${renderMetricRows(rows)}

      ${
        showsMoreSensors
          ? `
          <div class="divider"></div>
          ${
            moreSensors.length === 0
              ? '<p class="muted">No sensors available</p>'
              : renderMetricRows(
                  moreSensors.map((sensor) => ({
                    title: sensor.displayName,
                    value: formatTemperature(sensor.celsius),
                  })),
                )
          }
        `
          : ""
      }
    </section>
  `
}

function renderError(errorText) {
  if (!errorText) {
    return ""
  }

  return `
    <section class="error-banner">
      <div class="error-text">${htmlEscape(errorText)}</div>
      <div class="error-actions">
        <button id="copy-error" class="text-button">Copy</button>
        <button id="dismiss-error" class="text-button">Dismiss</button>
      </div>
    </section>
  `
}

function renderEmptyState() {
  return `
    <section class="card empty-state">
      <h3>No fans detected</h3>
      <p>Fan data appears after hardware sensors are reachable</p>
    </section>
  `
}

function render(snapshot) {
  const root = document.getElementById("app")
  const errorText = frontendError || snapshot.errorText

  const fanOptions = [
    `<option value="${snapshot.allFansId}" ${snapshot.selectedFanId === snapshot.allFansId ? "selected" : ""}>All</option>`,
    ...snapshot.fans.map(
      (fan) =>
        `<option value="${fan.id}" ${fan.id === snapshot.selectedFanId ? "selected" : ""}>${htmlEscape(fan.displayName)}</option>`,
    ),
  ].join("")

  root.innerHTML = `
    <section class="shell">
      <header class="top-bar">
        <div>
          <h1>FanControl</h1>
          <p class="subtitle">${htmlEscape(snapshot.processorName)}</p>
        </div>
        <div class="top-actions">
          <button id="open-settings" class="secondary-button">Settings</button>
          <button id="hide-window" class="secondary-button">Hide window</button>
        </div>
      </header>

      ${renderError(errorText)}

      ${
        snapshot.fans.length === 0
          ? renderEmptyState()
          : `
            <section class="card">
              <h3>Fans</h3>
              <select id="fan-select">${fanOptions}</select>
            </section>
            ${renderFanDetails(snapshot)}
            ${renderSensors(snapshot)}
            ${renderControlCard(snapshot)}
          `
      }
    </section>
  `

  bindRuntimeHandlers()
  syncSettingsPanel(snapshot)
}

function bindRuntimeHandlers() {
  document.getElementById("fan-select")?.addEventListener("change", async (event) => {
    const selectedFanId = Number(event.target.value)
    await refreshSnapshot("set_selected_fan", { selected_fan_id: selectedFanId })
  })

  document.getElementById("set-auto")?.addEventListener("click", async () => {
    await refreshSnapshot("set_auto")
  })

  document.getElementById("set-min")?.addEventListener("click", async () => {
    await refreshSnapshot("set_control_min")
  })

  document.getElementById("set-max")?.addEventListener("click", async () => {
    await refreshSnapshot("set_control_max")
  })

  document.getElementById("preset-rpm")?.addEventListener("change", (event) => {
    selectedPresetRpm = Number(event.target.value)
  })

  document.getElementById("set-preset")?.addEventListener("click", async () => {
    if (!selectedPresetRpm) {
      return
    }

    await refreshSnapshot("set_manual_rpm", {
      rpm: selectedPresetRpm,
      target_mode: "preset",
    })
  })

  document.getElementById("copy-error")?.addEventListener("click", async () => {
    const text = frontendError || appSnapshot?.errorText || ""
    if (!text) {
      return
    }

    await copyToClipboard(text)
    frontendError = null
    await refreshSnapshot("dismiss_error")
  })

  document.getElementById("dismiss-error")?.addEventListener("click", async () => {
    frontendError = null
    await refreshSnapshot("dismiss_error")
  })

  document.getElementById("toggle-sensors")?.addEventListener("click", () => {
    showsMoreSensors = !showsMoreSensors
    if (appSnapshot) {
      render(appSnapshot)
    }
  })

  document.getElementById("open-settings")?.addEventListener("click", () => {
    document.getElementById("settings-dialog")?.showModal()
  })

  document.getElementById("hide-window")?.addEventListener("click", async () => {
    if (tauriWindow) {
      await tauriWindow.minimize()
    }
  })
}

async function copyToClipboard(text) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text)
    return
  }

  const helper = document.createElement("textarea")
  helper.value = text
  helper.style.position = "fixed"
  helper.style.opacity = "0"
  document.body.appendChild(helper)
  helper.focus()
  helper.select()
  document.execCommand("copy")
  document.body.removeChild(helper)
}

function bindSettingsPanel() {
  if (settingsBound) {
    return
  }

  settingsBound = true

  const settingsDialog = document.getElementById("settings-dialog")
  const closeButton = document.getElementById("close-settings")
  const hideWindowOnLaunchInput = document.getElementById("hide-window-on-launch")
  const temperatureUnitInput = document.getElementById("temperature-unit")
  const temperaturePrecisionInput = document.getElementById("temperature-precision")
  const launchAtLoginInput = document.getElementById("launch-at-login")

  closeButton?.addEventListener("click", () => settingsDialog?.close())

  hideWindowOnLaunchInput?.addEventListener("change", (event) => {
    settings.hideWindowOnLaunch = event.target.checked
    saveSettings()
  })

  temperatureUnitInput?.addEventListener("change", (event) => {
    settings.temperatureUnit = event.target.value
    saveSettings()
    if (appSnapshot) {
      render(appSnapshot)
    }
  })

  temperaturePrecisionInput?.addEventListener("change", (event) => {
    settings.temperaturePrecision = event.target.value
    saveSettings()
    if (appSnapshot) {
      render(appSnapshot)
    }
  })

  launchAtLoginInput?.addEventListener("change", async (event) => {
    try {
      launchAtLoginEnabled = await invoke("set_launch_at_login_enabled", {
        enabled: event.target.checked,
      })
      event.target.checked = launchAtLoginEnabled
      frontendError = null
      if (appSnapshot) {
        render(appSnapshot)
      }
    } catch (error) {
      event.target.checked = launchAtLoginEnabled
      frontendError = normalizeError(error)
      if (appSnapshot) {
        render(appSnapshot)
      }
    }
  })

  document.getElementById("share-fancontrol")?.addEventListener("click", async () => {
    if (opener?.openUrl) {
      await opener.openUrl("https://fancontrol.dev")
      return
    }

    window.open("https://fancontrol.dev", "_blank")
  })

  document.getElementById("copy-sensor-data")?.addEventListener("click", async () => {
    try {
      const debugText = await invoke("get_debug_text")
      await copyToClipboard(debugText)
      frontendError = null
    } catch (error) {
      frontendError = normalizeError(error)
    }

    if (appSnapshot) {
      render(appSnapshot)
    }
  })
}

function syncSettingsPanel(snapshot) {
  const hideWindowOnLaunchInput = document.getElementById("hide-window-on-launch")
  const temperatureUnitInput = document.getElementById("temperature-unit")
  const temperaturePrecisionInput = document.getElementById("temperature-precision")
  const launchAtLoginInput = document.getElementById("launch-at-login")
  const processorName = document.getElementById("processor-name")
  const providerName = document.getElementById("provider-name")

  if (hideWindowOnLaunchInput) {
    hideWindowOnLaunchInput.checked = settings.hideWindowOnLaunch
  }

  if (temperatureUnitInput) {
    temperatureUnitInput.value = settings.temperatureUnit
  }

  if (temperaturePrecisionInput) {
    temperaturePrecisionInput.value = settings.temperaturePrecision
  }

  if (launchAtLoginInput) {
    launchAtLoginInput.checked = launchAtLoginEnabled
  }

  if (processorName) {
    processorName.textContent = `Device: ${snapshot.processorName}`
  }

  if (providerName) {
    providerName.textContent = `Provider: ${snapshot.providerName}`
  }
}

function normalizeError(error) {
  if (!error) {
    return "Unknown error"
  }

  if (typeof error === "string") {
    return error
  }

  if (error.message) {
    return String(error.message)
  }

  return JSON.stringify(error)
}

async function refreshSnapshot(command, args = {}) {
  try {
    const snapshot = await invoke(command, args)
    frontendError = null
    appSnapshot = snapshot
    render(snapshot)
    return snapshot
  } catch (error) {
    frontendError = normalizeError(error)

    if (appSnapshot) {
      render(appSnapshot)
      return appSnapshot
    }

    throw error
  }
}

function startTicking() {
  if (tickerHandle) {
    clearInterval(tickerHandle)
  }

  tickerHandle = setInterval(async () => {
    if (tickInFlight) {
      return
    }

    tickInFlight = true

    try {
      await refreshSnapshot("tick")
    } finally {
      tickInFlight = false
    }
  }, 1000)
}

async function bootstrap() {
  bindSettingsPanel()

  try {
    launchAtLoginEnabled = await invoke("get_launch_at_login_enabled")
  } catch {
    launchAtLoginEnabled = false
  }

  const snapshot = await refreshSnapshot("initialize")

  if (settings.hideWindowOnLaunch && tauriWindow) {
    await tauriWindow.minimize()
  }

  render(snapshot)
  startTicking()
}

window.addEventListener("DOMContentLoaded", () => {
  bootstrap().catch((error) => {
    const root = document.getElementById("app")
    root.innerHTML = `
      <section class="shell">
        <section class="error-banner">
          <div class="error-text">${htmlEscape(normalizeError(error))}</div>
        </section>
      </section>
    `
  })
})
