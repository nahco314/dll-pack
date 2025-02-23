use std::path::Path;

pub(crate) fn get_available_drives() -> Vec<char> {
    let mut drives = Vec::new();

    for drive_letter in b'A'..=b'Z' {
        let drive_path = format!("{}:\\", drive_letter as char);
        if Path::new(&drive_path).exists() {
            drives.push(drive_letter as char);
        }
    }

    drives
}
