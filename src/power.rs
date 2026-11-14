//! Reading the current power source from IOKit's power sources API.

use std::fmt;

/// What the computer is drawing power from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    /// Plugged in.
    Ac,
    /// Running on its internal battery.
    Battery,
    /// Running on an uninterruptible power supply's battery.
    Ups,
}

impl PowerSource {
    /// Interprets the power source type returned by `IOPSGetProvidingPowerSourceType`.
    pub fn from_iokit_type(power_type: &str) -> Option<PowerSource> {
        match power_type {
            "AC Power" => Some(PowerSource::Ac),
            "Battery Power" => Some(PowerSource::Battery),
            "UPS Power" => Some(PowerSource::Ups),
            _ => None,
        }
    }
}

impl fmt::Display for PowerSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PowerSource::Ac => "AC power",
            PowerSource::Battery => "battery power",
            PowerSource::Ups => "UPS power",
        })
    }
}

/// Returns the current power source.
#[cfg(target_os = "macos")]
pub fn current() -> Result<PowerSource, String> {
    use core_foundation::base::{CFType, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
        fn IOPSGetProvidingPowerSourceType(snapshot: CFTypeRef) -> CFStringRef;
    }

    // SAFETY: Takes no arguments, and returns a new reference (or null), which `CFType` releases.
    let snapshot = unsafe { IOPSCopyPowerSourcesInfo() };
    if snapshot.is_null() {
        return Err("IOPSCopyPowerSourcesInfo failed".to_owned());
    }
    // SAFETY: `snapshot` is a non-null reference that we own.
    let snapshot = unsafe { CFType::wrap_under_create_rule(snapshot) };
    // SAFETY: `snapshot` came from `IOPSCopyPowerSourcesInfo` and outlives the call. The result
    // is borrowed (or null), so `CFString` retains it.
    let power_type = unsafe { IOPSGetProvidingPowerSourceType(snapshot.as_CFTypeRef()) };
    if power_type.is_null() {
        return Err("IOPSGetProvidingPowerSourceType failed".to_owned());
    }
    // SAFETY: `power_type` is a non-null `CFStringRef` that we don't own.
    let power_type = unsafe { CFString::wrap_under_get_rule(power_type) }.to_string();
    PowerSource::from_iokit_type(&power_type)
        .ok_or_else(|| format!("unknown power source type {power_type:?}"))
}

/// Returns the current power source.
#[cfg(not(target_os = "macos"))]
pub fn current() -> Result<PowerSource, String> {
    Err("reading the power source is only supported on macOS".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interprets_iokit_power_source_types() {
        // The values of kIOPMACPowerKey, kIOPMBatteryPowerKey, and kIOPMUPSPowerKey.
        assert_eq!(
            PowerSource::from_iokit_type("AC Power"),
            Some(PowerSource::Ac)
        );
        assert_eq!(
            PowerSource::from_iokit_type("Battery Power"),
            Some(PowerSource::Battery)
        );
        assert_eq!(
            PowerSource::from_iokit_type("UPS Power"),
            Some(PowerSource::Ups)
        );
        assert_eq!(PowerSource::from_iokit_type("Solar Power"), None);
    }

    #[test]
    fn displays_power_sources() {
        assert_eq!(PowerSource::Ac.to_string(), "AC power");
        assert_eq!(PowerSource::Battery.to_string(), "battery power");
        assert_eq!(PowerSource::Ups.to_string(), "UPS power");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn reads_the_current_power_source() {
        current().unwrap();
    }
}
