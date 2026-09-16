fn main() {
    windows();
}

#[cfg(windows)]
fn windows() {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/branding/logo.ico");
    res.compile().unwrap();
}

#[cfg(not(windows))]
fn windows() {}
