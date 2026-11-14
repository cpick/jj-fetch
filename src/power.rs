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
        let _ = power_type;
        todo!()
    }
}

impl fmt::Display for PowerSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!()
    }
}

/// Returns the current power source.
pub fn current() -> Result<PowerSource, String> {
    todo!()
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
