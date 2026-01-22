#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::{Manager, State, WindowEvent};
use tauri_plugin_autostart::{Builder as AutostartBuilder, MacosLauncher, ManagerExt};

fn default_buttons() -> HashMap<String, String> {
    let mut buttons = HashMap::new();
    buttons.insert("left".to_string(), "Default".to_string());
    buttons.insert("right".to_string(), "Default".to_string());
    buttons.insert("middle".to_string(), "Default".to_string());
    buttons.insert("button4".to_string(), "Default".to_string());
    buttons.insert("button5".to_string(), "Default".to_string());
    buttons
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
struct DeviceConfig {
    name: String,
    buttons: HashMap<String, String>,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            buttons: default_buttons(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
struct Settings {
    theme: String,
    startup: bool,
    selected_device: Option<String>,
    devices: HashMap<String, DeviceConfig>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            startup: false,
            selected_device: None,
            devices: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
struct MouseDevice {
    id: String,
    name: String,
}

#[derive(Clone, Default)]
struct AppState {
    settings: Arc<Mutex<Settings>>,
    devices: Arc<Mutex<HashSet<String>>>,
}

impl AppState {
    fn update_settings(&self, settings: Settings) {
        if let Ok(mut guard) = self.settings.lock() {
            *guard = settings;
        }
    }

    fn update_devices(&self, devices: &[MouseDevice]) {
        let mut set = HashSet::new();
        for device in devices {
            set.insert(device.id.clone());
        }
        if let Ok(mut guard) = self.devices.lock() {
            *guard = set;
        }
    }

    fn snapshot_settings(&self) -> Settings {
        self.settings
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn is_selected_device_available(&self, device_id: &str) -> bool {
        self.devices
            .lock()
            .map(|guard| guard.contains(device_id))
            .unwrap_or(false)
    }
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|err| err.to_string())?;
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir.join("settings.json"))
}

fn load_settings(app: &tauri::AppHandle) -> Result<Settings, String> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(Settings::default());
    }
    let data = fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&data).map_err(|err| err.to_string())
}

fn persist_settings(app: &tauri::AppHandle, settings: Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    let data = serde_json::to_string_pretty(&settings).map_err(|err| err.to_string())?;
    fs::write(path, data).map_err(|err| err.to_string())
}

fn list_mouse_devices() -> Result<Vec<MouseDevice>, String> {
    let api = hidapi::HidApi::new().map_err(|err| err.to_string())?;
    let mut devices = Vec::new();

    for device in api.device_list() {
        let usage_page = device.usage_page();
        let usage = device.usage();
        let name = device
            .product_string()
            .map(|value| value.to_string())
            .or_else(|| device.manufacturer_string().map(|value| value.to_string()))
            .unwrap_or_else(|| "Unknown Mouse".to_string());

        let is_mouse_usage = usage_page == 0x01 && usage == 0x02;
        let is_trackpad = name.to_lowercase().contains("trackpad");

        if is_mouse_usage && !is_trackpad {
            let serial = device.serial_number().unwrap_or("noserial");
            let id = format!(
                "{:04x}:{:04x}:{}",
                device.vendor_id(),
                device.product_id(),
                serial
            );
            devices.push(MouseDevice { id, name });
        }
    }

    Ok(devices)
}

fn log_mouse_devices() {
    match hidapi::HidApi::new() {
        Ok(api) => {
            let mut found = false;
            for device in api.device_list() {
                let usage_page = device.usage_page();
                let usage = device.usage();
                let name = device
                    .product_string()
                    .map(|value| value.to_string())
                    .or_else(|| device.manufacturer_string().map(|value| value.to_string()))
                    .unwrap_or_else(|| "Unknown Mouse".to_string());
                let is_mouse_usage = usage_page == 0x01 && usage == 0x02;
                let is_trackpad = name.to_lowercase().contains("trackpad");

                if is_mouse_usage && !is_trackpad {
                    found = true;
                    println!(
                        "mouse-device: name=\"{}\" vendor_id=0x{:04x} product_id=0x{:04x} usage_page=0x{:02x} usage=0x{:02x}",
                        name,
                        device.vendor_id(),
                        device.product_id(),
                        usage_page,
                        usage
                    );
                }
            }

            if !found {
                println!("mouse-device: none found");
            }
        }
        Err(err) => {
            println!("mouse-scan error: {}", err);
        }
    }
}

#[tauri::command]
fn get_mouse_devices(state: State<AppState>) -> Result<Vec<MouseDevice>, String> {
    let devices = list_mouse_devices()?;
    state.update_devices(&devices);
    println!("get_mouse_devices: {} device(s)", devices.len());
    Ok(devices)
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .plugin(
            AutostartBuilder::new()
                .app_name("Edit Mouse")
                .macos_launcher(MacosLauncher::AppleScript)
                .build(),
        )
        .on_menu_event(|app, event| {
            let item_id = event.id().as_ref();
            if item_id == "tray_show" {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            } else if item_id == "tray_hide" {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            } else if item_id == "tray_quit" {
                app.exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_mouse_devices,
            get_autostart_enabled,
            set_autostart_enabled,
            hide_window,
            get_settings,
            save_settings
        ])
        .setup(|app| {
            log_mouse_devices();

            let state = app.state::<AppState>().inner().clone();
            if let Ok(settings) = load_settings(&app.handle()) {
                state.update_settings(settings);
            }
            start_mouse_remap(state);

            #[cfg(target_os = "macos")]
            {
                let _ = app
                    .handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            let show = MenuItem::with_id(app, "tray_show", "Show", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "tray_hide", "Hide", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "tray_quit", "Quit", true, None::<&str>)?;
            let menu: Menu<_> = Menu::with_items(app, &[&show, &hide, &quit])?;
            if let Some(tray) = app.handle().tray_by_id("main") {
                tray.set_menu(Some(menu))?;
                let _ = tray.set_icon_as_template(true);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn get_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    let manager = app.autolaunch();
    manager.is_enabled().map_err(|err| err.to_string())
}

#[tauri::command]
fn set_autostart_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|err| err.to_string())
    } else {
        manager.disable().map_err(|err| err.to_string())
    }
}

#[tauri::command]
fn hide_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_settings(app: tauri::AppHandle, state: State<AppState>) -> Result<Settings, String> {
    let settings = load_settings(&app)?;
    state.update_settings(settings.clone());
    Ok(settings)
}

#[tauri::command]
fn save_settings(
    app: tauri::AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Result<(), String> {
    persist_settings(&app, settings.clone())?;
    state.update_settings(settings);
    Ok(())
}

#[cfg(target_os = "macos")]
fn start_mouse_remap(state: AppState) {
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::event::{
        CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
        CGEventTapPlacement, CGEventType, CGMouseButton, EventField,
    };
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    const KEYCODE_LEFT_BRACKET: u16 = 0x21;
    const KEYCODE_RIGHT_BRACKET: u16 = 0x1E;

    std::thread::spawn(move || {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok();
        let Some(source) = source else {
            return;
        };
        let callback_state = state.clone();
        let tap = CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseUp,
                CGEventType::RightMouseDown,
                CGEventType::RightMouseUp,
                CGEventType::OtherMouseDown,
                CGEventType::OtherMouseUp,
            ],
            move |_proxy, event_type, event| {
                let button = event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);
                let action = resolve_action(&callback_state, button);

                if action == Action::Default {
                    return Some(event.clone());
                }

                if matches!(
                    event_type,
                    CGEventType::LeftMouseDown
                        | CGEventType::RightMouseDown
                        | CGEventType::OtherMouseDown
                ) {
                    match action {
                        Action::Disabled => {}
                        Action::Back => {
                            post_key_combo(&source, KEYCODE_LEFT_BRACKET);
                        }
                        Action::Forward => {
                            post_key_combo(&source, KEYCODE_RIGHT_BRACKET);
                        }
                        Action::MiddleClick => {
                            post_mouse_click(&source, event, 2, false);
                        }
                        Action::DoubleClick => {
                            post_mouse_click(&source, event, 0, true);
                        }
                        Action::Default => {}
                    }
                }

                None
            },
        );

        let Ok(tap) = tap else {
            eprintln!("mouse-remap: failed to create event tap (check Input Monitoring permission)");
            return;
        };

        unsafe {
            let loop_source = tap
                .mach_port
                .create_runloop_source(0)
                .expect("failed to create event tap runloop source");
            let runloop = CFRunLoop::get_current();
            runloop.add_source(&loop_source, kCFRunLoopCommonModes);
            tap.enable();
            CFRunLoop::run_current();
        }
    });

    fn resolve_action(state: &AppState, button: i64) -> Action {
        let settings = state.snapshot_settings();
        let Some(selected) = settings.selected_device.as_ref() else {
            return Action::Default;
        };
        if !state.is_selected_device_available(selected) {
            return Action::Default;
        }
        let device = settings.devices.get(selected);
        let Some(device) = device else {
            return Action::Default;
        };
        let key = match button {
            0 => "left",
            1 => "right",
            2 => "middle",
            3 => "button4",
            4 => "button5",
            _ => return Action::Default,
        };
        let action = device.buttons.get(key).map(String::as_str).unwrap_or("Default");
        Action::from(action)
    }

    fn post_key_combo(source: &CGEventSource, keycode: u16) {
        if let Ok(key_down) = CGEvent::new_keyboard_event(source.clone(), keycode, true) {
            key_down.set_flags(CGEventFlags::CGEventFlagCommand);
            key_down.post(CGEventTapLocation::HID);
        }
        if let Ok(key_up) = CGEvent::new_keyboard_event(source.clone(), keycode, false) {
            key_up.set_flags(CGEventFlags::CGEventFlagCommand);
            key_up.post(CGEventTapLocation::HID);
        }
    }

    fn post_mouse_click(source: &CGEventSource, event: &CGEvent, button: i64, double: bool) {
        let location = event.location();
        let mouse_button = match button {
            0 => CGMouseButton::Left,
            1 => CGMouseButton::Right,
            _ => CGMouseButton::Center,
        };
        let (down_type, up_type) = match button {
            0 => (CGEventType::LeftMouseDown, CGEventType::LeftMouseUp),
            1 => (CGEventType::RightMouseDown, CGEventType::RightMouseUp),
            _ => (CGEventType::OtherMouseDown, CGEventType::OtherMouseUp),
        };

        let clicks = if double { 2 } else { 1 };
        for _ in 0..clicks {
            if let Ok(down) =
                CGEvent::new_mouse_event(source.clone(), down_type, location, mouse_button)
            {
                down.set_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER, button);
                down.post(CGEventTapLocation::HID);
            }
            if let Ok(up) = CGEvent::new_mouse_event(source.clone(), up_type, location, mouse_button)
            {
                up.set_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER, button);
                up.post(CGEventTapLocation::HID);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn start_mouse_remap(_state: AppState) {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Default,
    Disabled,
    Back,
    Forward,
    MiddleClick,
    DoubleClick,
}

impl Action {
    fn from(value: &str) -> Self {
        match value {
            "Disabled" => Action::Disabled,
            "Back" => Action::Back,
            "Forward" => Action::Forward,
            "Middle Click" => Action::MiddleClick,
            "Double Click" => Action::DoubleClick,
            _ => Action::Default,
        }
    }
}
