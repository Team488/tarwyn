//! What each role measures, and the lines it reports it in.

use crate::sim::profile::Class;
use anyhow::anyhow;
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

/// How a receiver asked for a topic's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    /// Every value, so a gap in the sequence is a loss.
    All,
    /// The latest value per period, so gaps are expected.
    Coalesce,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::All => "all",
            Mode::Coalesce => "coalesce",
        }
    }
}

impl FromStr for Mode {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> anyhow::Result<Self> {
        match text {
            "all" => Ok(Mode::All),
            "coalesce" => Ok(Mode::Coalesce),
            other => Err(anyhow!("{other} is not a subscription mode")),
        }
    }
}

/// The latencies one receiver saw for one class, and the gaps between them.
pub struct Fold {
    hist: Histogram<u64>,
    pub received: u64,
    pub lost: u64,
    last: HashMap<String, u64>,
}

impl Fold {
    pub fn new() -> Self {
        Fold {
            hist: Histogram::new_with_bounds(1, 60_000_000_000, 3)
                .expect("histogram bounds are valid"),
            received: 0,
            lost: 0,
            last: HashMap::new(),
        }
    }

    /// Record one value of `topic`. A jump in `seq` counts as lost values
    /// when the receiver asked for every value.
    pub fn record(&mut self, topic: &str, seq: u64, latency_ns: u64, mode: Mode) {
        self.hist.saturating_record(latency_ns.max(1));
        self.received += 1;
        if mode != Mode::All {
            return;
        }
        if let Some(last) = self.last.get(topic)
            && seq > last + 1
        {
            self.lost += seq - last - 1;
        }
        let last = self.last.entry(topic.to_string()).or_insert(seq);
        *last = (*last).max(seq);
    }

    /// The line this fold reports as.
    pub fn line(
        &self,
        receiver: &str,
        class: Class,
        mode: Mode,
        expected: f64,
        allow_us: f64,
    ) -> SimLine {
        let us = |nanos: u64| nanos as f64 / 1000.0;
        SimLine {
            receiver: receiver.to_string(),
            class,
            mode,
            received: self.received,
            expected,
            lost: self.lost,
            p50_us: us(self.hist.value_at_quantile(0.50)),
            p99_us: us(self.hist.value_at_quantile(0.99)),
            max_us: us(self.hist.max()),
            allow_us,
        }
    }
}

impl Default for Fold {
    fn default() -> Self {
        Self::new()
    }
}

/// What one receiver saw of one class, as `SIM` and nine tab-separated fields.
#[derive(Debug, Clone, PartialEq)]
pub struct SimLine {
    pub receiver: String,
    pub class: Class,
    pub mode: Mode,
    pub received: u64,
    /// How many values the receiver should have seen, for an `all` receiver.
    pub expected: f64,
    pub lost: u64,
    pub p50_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
    /// The p99 latency past which the class broke for this receiver.
    pub allow_us: f64,
}

impl fmt::Display for SimLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SIM\t{}\t{}\t{}\t{}\t{:.1}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.1}",
            self.receiver,
            self.class.as_str(),
            self.mode.as_str(),
            self.received,
            self.expected,
            self.lost,
            self.p50_us,
            self.p99_us,
            self.max_us,
            self.allow_us
        )
    }
}

/// The tab-separated fields after `tag`, refusing any other count.
fn fields<'a>(text: &'a str, tag: &str, count: usize) -> anyhow::Result<Vec<&'a str>> {
    let rest = text
        .strip_prefix(tag)
        .and_then(|rest| rest.strip_prefix('\t'))
        .ok_or_else(|| anyhow!("not a {tag} line: {text}"))?;
    let fields: Vec<&str> = rest.split('\t').map(str::trim).collect();
    if fields.len() != count {
        return Err(anyhow!(
            "a {tag} line has {} fields, not {count}: {text}",
            fields.len()
        ));
    }
    Ok(fields)
}

impl FromStr for SimLine {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> anyhow::Result<Self> {
        let f = fields(text, "SIM", 10)?;
        Ok(SimLine {
            receiver: f[0].to_string(),
            class: f[1].parse()?,
            mode: f[2].parse()?,
            received: f[3].parse()?,
            expected: f[4].parse()?,
            lost: f[5].parse()?,
            p50_us: f[6].parse()?,
            p99_us: f[7].parse()?,
            max_us: f[8].parse()?,
            allow_us: f[9].parse()?,
        })
    }
}

/// How much of one class a publisher sent against what it was asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct PubLine {
    pub client: String,
    pub class: Class,
    pub asked: f64,
    pub sent: u64,
}

impl fmt::Display for PubLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PUB\t{}\t{}\t{:.0}\t{}",
            self.client,
            self.class.as_str(),
            self.asked,
            self.sent
        )
    }
}

impl FromStr for PubLine {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> anyhow::Result<Self> {
        let f = fields(text, "PUB", 4)?;
        Ok(PubLine {
            client: f[0].to_string(),
            class: f[1].parse()?,
            asked: f[2].parse()?,
            sent: f[3].parse()?,
        })
    }
}

/// How long a fresh dashboard took to see every topic, then a value for each.
/// `None` when it gave up first.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnLine {
    pub announced_ms: Option<f64>,
    pub values_ms: Option<f64>,
}

impl fmt::Display for ConnLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms =
            |value: Option<f64>| value.map_or_else(|| "-".to_string(), |ms| format!("{ms:.1}"));
        write!(f, "CONN\t{}\t{}", ms(self.announced_ms), ms(self.values_ms))
    }
}

impl FromStr for ConnLine {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> anyhow::Result<Self> {
        let f = fields(text, "CONN", 2)?;
        let ms = |field: &str| -> anyhow::Result<Option<f64>> {
            if field == "-" {
                Ok(None)
            } else {
                Ok(Some(field.parse()?))
            }
        };
        Ok(ConnLine {
            announced_ms: ms(f[0])?,
            values_ms: ms(f[1])?,
        })
    }
}

/// Every line a role log carried that the sweep reads.
#[derive(Debug, Default)]
pub struct Lines {
    pub sims: Vec<SimLine>,
    pub pubs: Vec<PubLine>,
    pub conns: Vec<ConnLine>,
}

/// The `SIM`, `PUB` and `CONN` lines in a role's log. Other lines and lines
/// that do not parse are left out.
pub fn parse(text: &str) -> Lines {
    let mut lines = Lines::default();
    for line in text.lines() {
        if line.starts_with("SIM\t") {
            lines.sims.extend(line.parse().ok());
        } else if line.starts_with("PUB\t") {
            lines.pubs.extend(line.parse().ok());
        } else if line.starts_with("CONN\t") {
            lines.conns.extend(line.parse().ok());
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::profile::Class;

    #[test]
    fn a_sim_line_reads_back_as_itself() {
        let line = SimLine {
            receiver: "advantagescope".into(),
            class: Class::Struct,
            mode: Mode::All,
            received: 740,
            expected: 750.0,
            lost: 10,
            p50_us: 31.5,
            p99_us: 210.25,
            max_us: 900.0,
            allow_us: 60_000.0,
        };
        let text = line.to_string();
        assert!(
            text.starts_with("SIM\tadvantagescope\tstruct\tall\t740\t"),
            "{text}"
        );
        assert_eq!(text.parse::<SimLine>().unwrap(), line);
    }

    #[test]
    fn a_fold_counts_gaps_only_where_every_value_was_asked_for() {
        let mut fold = Fold::new();
        for seq in [1, 4] {
            fold.record("/a", seq, 1_000, Mode::All);
            fold.record("/b", seq, 1_000, Mode::Coalesce);
        }
        assert_eq!((fold.received, fold.lost), (4, 2));
    }

    #[test]
    fn a_fold_reports_its_percentiles_in_microseconds() {
        let mut fold = Fold::new();
        for seq in 0..100 {
            fold.record("/a", seq, (seq + 1) * 1_000, Mode::All);
        }
        let line = fold.line("robot", Class::Camera, Mode::All, 100.0, 16_666.0);
        assert!((49.0..52.0).contains(&line.p50_us), "{line}");
        assert!((99.0..101.0).contains(&line.max_us), "{line}");
    }

    #[test]
    fn the_parser_keeps_its_own_lines_and_ignores_the_rest() {
        let text = "subscribed\nSIM\trobot\tcamera\tall\t10\t10\t0\t1.0\t2.0\t3.0\t16666.7\n\
                    PUB\tcamera-front\tcamera\t600\t598\nCONN\t-\t12.5\nROW\tpublish\n";
        let lines = parse(text);
        assert_eq!(lines.sims.len(), 1);
        assert_eq!(
            lines.pubs[0],
            PubLine {
                client: "camera-front".into(),
                class: Class::Camera,
                asked: 600.0,
                sent: 598
            }
        );
        assert_eq!(
            lines.conns[0],
            ConnLine {
                announced_ms: None,
                values_ms: Some(12.5)
            }
        );
    }
}
