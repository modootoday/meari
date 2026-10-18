//! Development verification against the caller-selected showcase process, using Windows UIA.
use std::time::Duration;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            Variant::VARIANT,
        },
        UI::{Accessibility::*, WindowsAndMessaging::*},
    },
    core::{BOOL, Interface},
};

struct Com;
struct Target {
    pid: u32,
    window: Option<HWND>,
}
unsafe extern "system" fn owned_window(window: HWND, data: LPARAM) -> BOOL {
    // EnumWindows invokes this synchronously, so the stack target remains valid.
    let target = unsafe { &mut *(data.0 as *mut Target) };
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(window, Some(&mut pid));
    }
    if pid == target.pid && unsafe { IsWindowVisible(window).as_bool() } {
        let mut title = [0u16; 128];
        let length = unsafe { GetWindowTextW(window, &mut title) };
        if String::from_utf16_lossy(&title[..length as usize]) == "Meari Native" {
            target.window = Some(window);
        }
    }
    BOOL(1)
}
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn connect(pid: u32) -> Result<(Com, IUIAutomation, IUIAutomationElement), String> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        let com = Com;
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
        let mut target = Target { pid, window: None };
        EnumWindows(
            Some(owned_window),
            LPARAM((&mut target as *mut Target) as isize),
        )
        .map_err(|e| format!("enumerate owned native window: {e}"))?;
        let window = automation
            .ElementFromHandle(target.window.ok_or("owned native window not found")?)
            .map_err(|e| format!("UIA window handle: {e}"))?;
        let name = window.CurrentName().map_err(|e| e.to_string())?.to_string();
        if !name.contains("Meari Native") {
            return Err("UIA target is not the native showcase window".into());
        }
        Ok((com, automation, window))
    }
}

pub fn verify_tabs(pid: u32) -> Result<serde_json::Value, String> {
    // Every COM interface stays on this thread and drops before the COM guard.
    unsafe {
        let (_com, automation, window) = connect(pid)?;
        let condition = automation
            .CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_TabItemControlTypeId.0),
            )
            .map_err(|e| e.to_string())?;
        let elements = window
            .FindAll(TreeScope_Descendants, &condition)
            .map_err(|e| e.to_string())?;
        let length = elements.Length().map_err(|e| e.to_string())?;
        if length == 0 {
            return Err("no native TabItem controls in the caller-selected window".into());
        }
        let mut rows = Vec::new();
        let mut enabled = 0;
        for index in 0..length {
            let element = elements.GetElement(index).map_err(|e| e.to_string())?;
            let name = element
                .CurrentName()
                .map_err(|e| e.to_string())?
                .to_string();
            if name.trim().is_empty() {
                return Err(format!("native TabItem {index} has no accessible name"));
            }
            if !element
                .CurrentIsEnabled()
                .map_err(|e| e.to_string())?
                .as_bool()
            {
                rows.push(serde_json::json!({"name":name,"enabled":false,"action":"not-invoked-disabled"}));
                continue;
            }
            let selection: IUIAutomationSelectionItemPattern = element
                .GetCurrentPatternAs(UIA_SelectionItemPatternId)
                .map_err(|e| format!("{name} SelectionItem: {e}"))?;
            selection.Select().map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(150));
            let selected = selection
                .CurrentIsSelected()
                .map_err(|e| e.to_string())?
                .as_bool();
            let focused = element
                .CurrentHasKeyboardFocus()
                .map_err(|e| e.to_string())?
                .as_bool();
            if !selected || !focused {
                return Err(format!(
                    "UIA tab {name}: selected={selected}, focused={focused}"
                ));
            }
            rows.push(serde_json::json!({"name":name,"enabled":true,"selected":selected,"focused":focused}));
            enabled += 1;
        }
        if enabled == 0 {
            return Err(
                "all native TabItem controls are disabled; no selection scenario executed".into(),
            );
        }
        Ok(
            serde_json::json!({"pid":pid,"success":true,"input_transport":"Windows UIAutomation SelectionItem patterns","tabs":rows,"os_keyboard_pointer_verified":false,"screen_reader_verified":false,"upstream_provider_parity_verified":false}),
        )
    }
}

pub fn verify(pid: u32) -> Result<serde_json::Value, String> {
    // UIA is an OS COM boundary. All returned interfaces are used on this thread only.
    unsafe {
        let (_com, automation, window) = connect(pid)?;
        let name = window.CurrentName().map_err(|e| e.to_string())?.to_string();
        let find = |name: &str| -> Result<IUIAutomationElement, String> {
            let condition = automation
                .CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(name))
                .map_err(|e| e.to_string())?;
            window
                .FindFirst(TreeScope_Descendants, &condition)
                .map_err(|e| format!("UIA {name}: {e}"))
        };
        let details = find("Details")?;
        let selection: IUIAutomationSelectionItemPattern = details
            .GetCurrentPatternAs(UIA_SelectionItemPatternId)
            .map_err(|e| {
                format!(
                    "Details SelectionItem pattern (role {:?}): {e}",
                    details.CurrentControlType()
                )
            })?;
        selection.Select().map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(250));
        let details_selected = selection
            .CurrentIsSelected()
            .map_err(|e| e.to_string())?
            .as_bool();
        let trigger = find("Open dialog")?;
        let expand: IUIAutomationExpandCollapsePattern = trigger
            .GetCurrentPatternAs(UIA_ExpandCollapsePatternId)
            .map_err(|e| format!("Dialog trigger ExpandCollapse pattern: {e}"))?;
        expand.Expand().map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(250));
        let dialog = find("Native dialog")?;
        let dialog_role = dialog.CurrentControlType().map_err(|e| e.to_string())?;
        let input = find("Dialog value")?;
        let value: IUIAutomationValuePattern =
            input.GetCurrentPatternAs(UIA_ValuePatternId).map_err(|e| {
                format!(
                    "Input Value pattern (role {:?}): {e}",
                    input.CurrentControlType()
                )
            })?;
        value
            .SetValue(&windows::core::BSTR::from("Updated through Windows UIA"))
            .map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(250));
        let input_value = value.CurrentValue().map_err(|e| e.to_string())?.to_string();
        let close = find("Close dialog")?;
        let close: IUIAutomationInvokePattern = close
            .GetCurrentPatternAs(UIA_InvokePatternId)
            .map_err(|e| e.to_string())?;
        close.Invoke().map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(250));
        let trigger_focused = trigger
            .CurrentHasKeyboardFocus()
            .map_err(|e| e.to_string())?
            .as_bool();
        if !details_selected || input_value != "Updated through Windows UIA" || !trigger_focused {
            return Err(format!(
                "UIA assertion failed: selected={details_selected}, value={input_value}, focus_return={trigger_focused}"
            ));
        }
        let _ = details.as_raw(); // Interfaces remain alive through verification, released before COM uninitialization.
        Ok(
            serde_json::json!({"pid":pid,"window":name,"details_selected":details_selected,"dialog_control_type":dialog_role.0,"input_value":input_value,"focus_return":trigger_focused,"success":true,"input_transport":"Windows UIAutomation patterns"}),
        )
    }
}
