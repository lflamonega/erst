use anyhow::{Result, bail};

/// One whole CPU, in the units the runtime expects.
const NANOS_PER_CPU: i64 = 1_000_000_000;

const MEGABYTE: i64 = 1024 * 1024;
const GIGABYTE: i64 = 1024 * MEGABYTE;

/// Parse a memory cap such as `512m`, `1g` or a bare `512`.
///
/// A bare number means megabytes: read as bytes, `--memory 512` would kill the
/// app immediately, and the container convention is exactly the trap this tool
/// exists to keep people out of.
pub fn parse_memory(value: &str) -> Result<i64> {
    let text = value.trim().to_ascii_lowercase();
    let split = text
        .char_indices()
        .find(|(_, character)| !character.is_ascii_digit() && *character != '.')
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let (amount, unit) = text.split_at(split);

    let amount: f64 = amount.trim().parse().map_err(|_| {
        anyhow::anyhow!("invalid memory `{value}`, expected something like 512m or 1g")
    })?;
    if !amount.is_finite() || amount <= 0.0 {
        bail!("memory `{value}` has to be a positive amount");
    }

    let megabytes = match unit.trim() {
        "" | "m" | "mb" => amount,
        "k" | "kb" => amount / 1024.0,
        "g" | "gb" => amount * 1024.0,
        other => bail!(
            "unknown memory unit `{other}`: use m (megabytes) or g (gigabytes), \
             as in 512m, 1g or a bare 512"
        ),
    };

    let bytes = (megabytes * MEGABYTE as f64).round() as i64;
    if bytes < MEGABYTE {
        bail!("memory `{value}` is less than a megabyte; use at least 1m");
    }
    Ok(bytes)
}

/// Parse a CPU allowance into nano-CPUs: `1` is a whole CPU, `0.5` is half.
pub fn parse_cpu(value: &str) -> Result<i64> {
    let cpus: f64 = value.trim().parse().map_err(|_| {
        anyhow::anyhow!("invalid cpu `{value}`, expected a number of CPUs such as 1, 0.5 or 2.5")
    })?;
    if !cpus.is_finite() || cpus <= 0.0 {
        bail!("cpu `{value}` has to be a positive number of CPUs");
    }
    if cpus > 512.0 {
        bail!("cpu `{value}` is more than any machine has; use at most 512");
    }
    Ok((cpus * NANOS_PER_CPU as f64).round() as i64)
}

/// Whether a value means "no limit at all".
pub fn unlimited(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "unlimited" | "none" | "off"
    )
}

/// One memory limit as it arrives from a flag: `Some(bytes)`, or `None` when
/// the value says there should be no limit.
pub fn memory_value(value: &str) -> Result<Option<i64>> {
    if unlimited(value) {
        Ok(None)
    } else {
        parse_memory(value).map(Some)
    }
}

/// One CPU limit as it arrives from a flag, in nano-CPUs.
pub fn cpu_value(value: &str) -> Result<Option<i64>> {
    if unlimited(value) {
        Ok(None)
    } else {
        parse_cpu(value).map(Some)
    }
}

/// How a pair of limits reads: `512m · 1.5cpu`, or `-` when nothing is capped.
pub fn show(memory: Option<i64>, cpu_nanos: Option<i64>) -> String {
    let mut parts = Vec::new();
    if let Some(bytes) = memory {
        parts.push(memory_display(bytes));
    }
    if let Some(nanos) = cpu_nanos {
        parts.push(format!("{}cpu", cpu_display(nanos)));
    }

    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(" · ")
    }
}

/// Back to the shortest form that round-trips: `1g` rather than `1024m`.
fn memory_display(bytes: i64) -> String {
    if bytes % GIGABYTE == 0 {
        format!("{}g", bytes / GIGABYTE)
    } else if bytes % MEGABYTE == 0 {
        format!("{}m", bytes / MEGABYTE)
    } else {
        format!("{bytes}b")
    }
}

/// Whole CPUs, without the trailing zeros a float would print.
fn cpu_display(nanos: i64) -> String {
    let cpus = nanos as f64 / NANOS_PER_CPU as f64;
    format!("{}", (cpus * 1000.0).round() / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_memory_amount_is_megabytes() {
        assert_eq!(parse_memory("512").unwrap(), 512 * MEGABYTE);
        assert_eq!(parse_memory("512m").unwrap(), 512 * MEGABYTE);
        assert_eq!(parse_memory("512MB").unwrap(), 512 * MEGABYTE);
        assert_eq!(parse_memory(" 2G ").unwrap(), 2 * GIGABYTE);
        assert_eq!(parse_memory("1gb").unwrap(), GIGABYTE);
    }

    #[test]
    fn memory_that_cannot_be_read_says_why() {
        assert!(parse_memory("lots").is_err());
        assert!(parse_memory("512t").is_err());
        assert!(parse_memory("0").is_err());
        assert!(parse_memory("-1m").is_err());
        // The trap this exists to prevent: bytes are not accepted silently.
        assert!(parse_memory("512b").is_err());
        assert!(parse_memory("512k").is_err());
    }

    #[test]
    fn cpu_counts_whole_and_partial_cpus() {
        assert_eq!(parse_cpu("1").unwrap(), NANOS_PER_CPU);
        assert_eq!(parse_cpu("1.5").unwrap(), NANOS_PER_CPU * 3 / 2);
        assert_eq!(parse_cpu("0.5").unwrap(), NANOS_PER_CPU / 2);
        assert!(parse_cpu("zero").is_err());
        assert!(parse_cpu("0").is_err());
        assert!(parse_cpu("100000").is_err());
    }

    #[test]
    fn a_flag_can_also_lift_the_limit() {
        assert_eq!(memory_value("unlimited").unwrap(), None);
        assert_eq!(memory_value("NONE").unwrap(), None);
        assert_eq!(memory_value("512m").unwrap(), Some(512 * MEGABYTE));
        assert_eq!(cpu_value("unlimited").unwrap(), None);
        assert_eq!(cpu_value("0.5").unwrap(), Some(NANOS_PER_CPU / 2));
    }

    #[test]
    fn limits_read_the_way_they_were_written() {
        assert_eq!(
            show(Some(512 * MEGABYTE), Some(NANOS_PER_CPU * 3 / 2)),
            "512m · 1.5cpu"
        );
        assert_eq!(show(Some(GIGABYTE), None), "1g");
        assert_eq!(show(None, Some(NANOS_PER_CPU)), "1cpu");
        assert_eq!(show(None, None), "-");
    }

    #[test]
    fn limits_round_trip_through_display() {
        for value in ["512m", "1g", "2048m", "100m"] {
            let bytes = parse_memory(value).unwrap();
            let shown = show(Some(bytes), None);
            assert_eq!(
                parse_memory(&shown).unwrap(),
                bytes,
                "{value} became {shown}"
            );
        }
    }
}
