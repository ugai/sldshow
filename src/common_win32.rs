mod bindings {
    windows::include_bindings!();
}

use bindings::Windows::Win32::{
    System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
    },
    UI::KeyboardAndMouseInput::GetDoubleClickTime,
};

pub fn stop_screensaver() {
    unsafe {
        let _execution_state =
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
    }
}

pub fn get_double_click_time_ms() -> u32 {
    unsafe { GetDoubleClickTime() }
}
