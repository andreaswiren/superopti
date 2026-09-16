use std::cmp::Ordering;

#[derive(Clone, Copy)]
pub struct Sort {
    pub column: usize,
    pub descending: bool,
}

impl Sort {
    pub fn toggled(self, column: usize) -> Self {
        Self {
            column,
            descending: self.column == column && !self.descending,
        }
    }

    pub fn compare(self, a: &str, b: &str) -> Ordering {
        let missing = |s: &str| {
            matches!(
                s.trim(),
                "" | "—"
                    | "N/A"
                    | "Unavailable"
                    | "Unknown"
                    | "Not captured"
                    | "No observation"
                    | "Run core trace"
            )
        };
        // Missing telemetry always belongs below measured values, in either order.
        match (missing(a), missing(b)) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            _ => {}
        }
        let order = match (number(a), number(b)) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            _ => natural(&a.to_lowercase(), &b.to_lowercase()),
        };
        if self.descending {
            order.reverse()
        } else {
            order
        }
    }
}

fn number(text: &str) -> Option<f64> {
    let text = text.trim();
    let end = text
        .find(|c: char| {
            !c.is_ascii_digit() && !matches!(c, '.' | ',' | '-' | '+' | ' ' | '\u{a0}' | '\u{202f}')
        })
        .unwrap_or(text.len());
    let raw: String = text[..end]
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    let value: f64 = raw.parse().ok()?;
    let multiplier = match text[end..].trim().to_lowercase().as_str() {
        "" | "%" | "bytes" | "b" => 1.,
        "kib" | "kib/s" => 1024.,
        "mib" | "mib/s" => 1024. * 1024.,
        "gib" | "gib/s" => 1024. * 1024. * 1024.,
        "kb" | "kb/s" => 1000.,
        "mb" | "mb/s" => 1_000_000.,
        "gb" | "gb/s" => 1_000_000_000.,
        "ms" => 0.001,
        "s" | "seconds" => 1.,
        "min" | "minutes" => 60.,
        "hours" => 3600.,
        "days" => 86400.,
        _ => return None,
    };
    value.is_finite().then_some(value * multiplier)
}

fn natural(mut a: &str, mut b: &str) -> Ordering {
    while !a.is_empty() && !b.is_empty() {
        let ad = a.as_bytes()[0].is_ascii_digit();
        let bd = b.as_bytes()[0].is_ascii_digit();
        let al = a
            .find(|c: char| c.is_ascii_digit() != ad)
            .unwrap_or(a.len());
        let bl = b
            .find(|c: char| c.is_ascii_digit() != bd)
            .unwrap_or(b.len());
        let order = if ad && bd {
            let x = a[..al].trim_start_matches('0');
            let y = b[..bl].trim_start_matches('0');
            x.len().cmp(&y.len()).then_with(|| x.cmp(y))
        } else {
            a[..al].cmp(&b[..bl])
        };
        if order != Ordering::Equal {
            return order;
        }
        a = &a[al..];
        b = &b[bl..];
    }
    a.len().cmp(&b.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metrics_units_missing_values_and_natural_names_sort_correctly() {
        let ascending = Sort {
            column: 0,
            descending: false,
        };
        assert_eq!(ascending.compare("9.5", "100"), Ordering::Less);
        assert_eq!(ascending.compare("2 GiB", "900 MiB"), Ordering::Greater);
        assert_eq!(ascending.compare("1,024", "999"), Ordering::Greater);
        assert_eq!(ascending.compare("worker2", "worker10"), Ordering::Less);
        assert_eq!(
            ascending.compare("192.168.1.9", "192.168.1.100"),
            Ordering::Less
        );
        for sort in [ascending, ascending.toggled(0)] {
            assert_eq!(sort.compare("N/A", "0"), Ordering::Greater);
        }
        assert!(!ascending.toggled(1).descending);
    }
}
