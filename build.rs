#[cfg(windows)]
fn main() {
    windows::build! {
        Windows::Win32::System::Power::EXECUTION_STATE,
        Windows::Win32::System::Power::SetThreadExecutionState,
        Windows::Win32::UI::KeyboardAndMouseInput::GetDoubleClickTime,
    };

    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon/icon.ico")
        .set("ProductName", "sldshow")
        .set("FileDescription", "sldshow");
    res.compile().unwrap();
}

#[cfg(not(windows))]
fn main() {}
