pub mod chunker;
pub mod cleaner;
pub mod clipboard;
pub mod privacy;
pub mod types;
pub mod url_extractor;
pub mod window_monitor;
pub mod work_classifier;

#[cfg(target_os = "linux")]
pub mod linux_reader;

#[cfg(target_os = "macos")]
pub mod mac_reader;

#[cfg(target_os = "windows")]
pub mod ocr;

#[cfg(target_os = "windows")]
pub mod windows_reader;
